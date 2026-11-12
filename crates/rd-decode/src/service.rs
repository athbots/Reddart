//! Asynchronous video frame decoding pipeline with generational stale-request cancellation
//! and bounded LRU frame caching.
//!
//! Technical Proposal Sections 3.1 & 3.2:
//! - Background worker thread isolates decoding from the UI render loop.
//! - Generational cancellation drops obsolete requests during rapid playhead scrubbing.
//! - Bounded frame cache delivers zero-latency repeat frame access.
//! - Resilient error boundary handles corrupted or unsupported media without panics.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rd_core::{ClipId, TimestampUs};

use crate::prefetch::PlayDirection;
use crate::{DecodedFrame, FfmpegDecoder};

/// A request to asynchronously decode a frame for a clip at a given timestamp.
#[derive(Debug, Clone)]
pub struct FrameRequest {
    /// Unique identifier for this specific request.
    pub request_id: u64,
    /// Generation index at the time the request was issued.
    pub generation: u64,
    /// Timeline clip associated with this request.
    pub clip_id: ClipId,
    /// Absolute or relative path to the media file on disk.
    pub source_path: PathBuf,
    /// Requested presentation timestamp in microseconds.
    pub source_time_us: TimestampUs,
}

/// A response emitted by the asynchronous decoder worker.
#[derive(Debug, Clone)]
pub enum FrameResponse {
    /// Frame was successfully decoded and cached.
    Ready {
        request_id: u64,
        generation: u64,
        clip_id: ClipId,
        source_path: PathBuf,
        source_time_us: TimestampUs,
        frame: Arc<DecodedFrame>,
    },
    /// Decoding failed or media was invalid; application continues without crashing.
    Error {
        request_id: u64,
        generation: u64,
        clip_id: ClipId,
        source_path: PathBuf,
        source_time_us: TimestampUs,
        error: String,
    },
}

/// An entry in the decoded frame ring cache.
#[derive(Clone)]
struct CacheEntry {
    source_path: PathBuf,
    pts_us: TimestampUs,
    frame: Arc<DecodedFrame>,
}

/// Bounded LRU cache for decoded video frames.
pub struct FrameCache {
    entries: VecDeque<CacheEntry>,
    capacity: usize,
}

impl FrameCache {
    /// Create a new frame cache with a fixed upper bound of frames.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    /// Find a cached frame matching `source_path` within `tolerance_us` of `time_us`.
    pub fn get(
        &self,
        source_path: &Path,
        time_us: TimestampUs,
        tolerance_us: i64,
    ) -> Option<Arc<DecodedFrame>> {
        self.entries
            .iter()
            .filter(|e| e.source_path == source_path)
            .min_by_key(|e| (e.pts_us - time_us).abs())
            .filter(|e| (e.pts_us - time_us).abs() <= tolerance_us)
            .map(|e| e.frame.clone())
    }

    /// Insert a newly decoded frame, evicting the oldest entry when at capacity.
    pub fn insert(&mut self, source_path: PathBuf, frame: Arc<DecodedFrame>) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(CacheEntry {
            pts_us: frame.pts_us,
            source_path,
            frame,
        });
    }

    /// Empty the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of frames currently cached.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is currently empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Asynchronous decoder service coordinating background decoding workers.
pub struct AsyncDecoderService {
    req_tx: Sender<FrameRequest>,
    resp_rx: Receiver<FrameResponse>,
    generation: Arc<AtomicU64>,
    cache: Arc<Mutex<FrameCache>>,
    running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
    next_req_id: AtomicU64,
}

impl AsyncDecoderService {
    /// Initialize the asynchronous decoder service with a given frame cache capacity.
    pub fn new(cache_capacity: usize) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<FrameRequest>();
        let (resp_tx, resp_rx) = mpsc::channel::<FrameResponse>();

        let generation = Arc::new(AtomicU64::new(1));
        let cache = Arc::new(Mutex::new(FrameCache::new(cache_capacity)));
        let running = Arc::new(AtomicBool::new(true));

        let worker_generation = generation.clone();
        let worker_cache = cache.clone();
        let worker_running = running.clone();

        let handle = thread::Builder::new()
            .name("reddart-async-decoder".into())
            .spawn(move || {
                let mut decoders: HashMap<PathBuf, FfmpegDecoder> = HashMap::new();

                while worker_running.load(Ordering::Relaxed) {
                    let req = match req_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(r) => r,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };

                    // Generational cancellation check: drop obsolete requests immediately
                    let cur_gen = worker_generation.load(Ordering::Acquire);
                    if req.generation < cur_gen {
                        log::trace!(
                            "AsyncDecoder: dropped stale request #{} (gen {} < cur {})",
                            req.request_id,
                            req.generation,
                            cur_gen
                        );
                        continue;
                    }

                    // Check if already present in cache
                    if let Ok(c) = worker_cache.lock() {
                        if let Some(cached_frame) =
                            c.get(&req.source_path, req.source_time_us, 33_333)
                        {
                            let _ = resp_tx.send(FrameResponse::Ready {
                                request_id: req.request_id,
                                generation: req.generation,
                                clip_id: req.clip_id,
                                source_path: req.source_path,
                                source_time_us: req.source_time_us,
                                frame: cached_frame,
                            });
                            continue;
                        }
                    }

                    // Open decoder if not already opened
                    let decoder_result = if let Some(decoder) = decoders.get_mut(&req.source_path) {
                        Ok(decoder)
                    } else {
                        match FfmpegDecoder::open(&req.source_path) {
                            Ok(d) => {
                                decoders.insert(req.source_path.clone(), d);
                                Ok(decoders.get_mut(&req.source_path).unwrap())
                            }
                            Err(e) => Err(e),
                        }
                    };

                    match decoder_result {
                        Ok(decoder) => {
                            match decoder.decode_frame_at(req.source_time_us) {
                                Ok(Some(frame)) => {
                                    // Re-check generation after decode: if stale, don't send
                                    let cur_gen_post = worker_generation.load(Ordering::Acquire);
                                    if req.generation < cur_gen_post {
                                        continue;
                                    }

                                    let frame_arc = Arc::new(frame);
                                    if let Ok(mut c) = worker_cache.lock() {
                                        c.insert(req.source_path.clone(), frame_arc.clone());
                                    }

                                    let _ = resp_tx.send(FrameResponse::Ready {
                                        request_id: req.request_id,
                                        generation: req.generation,
                                        clip_id: req.clip_id,
                                        source_path: req.source_path,
                                        source_time_us: req.source_time_us,
                                        frame: frame_arc,
                                    });
                                }
                                Ok(None) => {
                                    let _ = resp_tx.send(FrameResponse::Error {
                                        request_id: req.request_id,
                                        generation: req.generation,
                                        clip_id: req.clip_id,
                                        source_path: req.source_path,
                                        source_time_us: req.source_time_us,
                                        error: "End of stream reached or frame unavailable".into(),
                                    });
                                }
                                Err(e) => {
                                    let _ = resp_tx.send(FrameResponse::Error {
                                        request_id: req.request_id,
                                        generation: req.generation,
                                        clip_id: req.clip_id,
                                        source_path: req.source_path,
                                        source_time_us: req.source_time_us,
                                        error: format!("Decode error: {}", e),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            let _ = resp_tx.send(FrameResponse::Error {
                                request_id: req.request_id,
                                generation: req.generation,
                                clip_id: req.clip_id,
                                source_path: req.source_path,
                                source_time_us: req.source_time_us,
                                error: format!("Failed to open media decoder: {}", e),
                            });
                        }
                    }
                }

                log::debug!("AsyncDecoder worker thread stopped");
            })
            .expect("Failed to spawn async decoder worker thread");

        Self {
            req_tx,
            resp_rx,
            generation,
            cache,
            running,
            worker_handle: Some(handle),
            next_req_id: AtomicU64::new(1),
        }
    }

    /// Advance the generation counter, invalidating all currently pending queued requests.
    /// Call this whenever scrubbing begins or playhead abruptly seeks.
    pub fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get current generation number.
    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Look up a frame in the cache without issuing a background request.
    pub fn get_cached(
        &self,
        source_path: &Path,
        time_us: TimestampUs,
        tolerance_us: i64,
    ) -> Option<Arc<DecodedFrame>> {
        self.cache
            .lock()
            .ok()?
            .get(source_path, time_us, tolerance_us)
    }

    /// Submit a non-blocking request to decode a frame.
    /// If the frame is already in cache, it will be returned immediately via `get_cached`.
    /// Returns the assigned unique request ID.
    pub fn request_frame(
        &self,
        clip_id: ClipId,
        source_path: PathBuf,
        source_time_us: TimestampUs,
    ) -> u64 {
        let request_id = self.next_req_id.fetch_add(1, Ordering::Relaxed);
        let generation = self.generation.load(Ordering::Acquire);

        let req = FrameRequest {
            request_id,
            generation,
            clip_id,
            source_path,
            source_time_us,
        };

        let _ = self.req_tx.send(req);
        request_id
    }

    /// Non-blocking check for the next available decoded frame response.
    pub fn try_recv(&self) -> Option<FrameResponse> {
        match self.resp_rx.try_recv() {
            Ok(resp) => Some(resp),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Drain all currently pending responses.
    pub fn drain_responses(&self) -> Vec<FrameResponse> {
        let mut responses = Vec::new();
        while let Ok(resp) = self.resp_rx.try_recv() {
            responses.push(resp);
        }
        responses
    }

    /// Clear all cached frames.
    pub fn clear_cache(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
    }

    /// Predictive lookahead: queue background decode requests for upcoming frames along the playhead's path.
    /// Skips any timestamp that is already present in the frame cache.
    /// Returns the number of prefetch requests actually queued.
    pub fn prefetch_lookahead(
        &self,
        clip_id: ClipId,
        source_path: PathBuf,
        base_time_us: TimestampUs,
        step_us: i64,
        count: usize,
    ) -> usize {
        if count == 0 || step_us == 0 {
            return 0;
        }

        let cache = match self.cache.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let mut queued = 0;
        let generation = self.generation.load(Ordering::Acquire);
        let tolerance_us = (step_us.abs() / 2).max(10_000);

        for i in 1..=count {
            let target_time_us = base_time_us + (i as i64 * step_us);
            if target_time_us < 0 {
                continue;
            }

            if cache
                .get(&source_path, target_time_us, tolerance_us)
                .is_none()
            {
                let request_id = self.next_req_id.fetch_add(1, Ordering::Relaxed);
                let req = FrameRequest {
                    request_id,
                    generation,
                    clip_id,
                    source_path: source_path.clone(),
                    source_time_us: target_time_us,
                };
                if self.req_tx.send(req).is_ok() {
                    queued += 1;
                }
            }
        }

        queued
    }

    /// Predictively prefetch ahead of the playhead based on playhead direction.
    /// When forward, prefetches next frames (+step_us).
    /// When reverse, prefetches previous frames (-step_us).
    /// When stationary, prefetches immediate surrounding frames (+/- step_us).
    pub fn prefetch_directional(
        &self,
        clip_id: ClipId,
        source_path: PathBuf,
        current_time_us: TimestampUs,
        direction: PlayDirection,
        step_us: i64,
        count: usize,
    ) -> usize {
        let abs_step = step_us.abs();
        match direction {
            PlayDirection::Forward => {
                self.prefetch_lookahead(clip_id, source_path, current_time_us, abs_step, count)
            }
            PlayDirection::Reverse => {
                self.prefetch_lookahead(clip_id, source_path, current_time_us, -abs_step, count)
            }
            PlayDirection::Stationary => {
                let half = (count / 2).max(1);
                let forward = self.prefetch_lookahead(
                    clip_id,
                    source_path.clone(),
                    current_time_us,
                    abs_step,
                    half,
                );
                let reverse =
                    self.prefetch_lookahead(clip_id, source_path, current_time_us, -abs_step, half);
                forward + reverse
            }
        }
    }
}

impl Drop for AsyncDecoderService {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_cache_lru() {
        let mut cache = FrameCache::new(2);
        let path = PathBuf::from("video.mp4");

        let frame1 = Arc::new(DecodedFrame {
            frame_index: 0,
            pts_us: 0,
            width: 1920,
            height: 1080,
            rgba_data: Some(vec![1; 16]),
            is_keyframe: true,
        });

        let frame2 = Arc::new(DecodedFrame {
            frame_index: 1,
            pts_us: 33_333,
            width: 1920,
            height: 1080,
            rgba_data: Some(vec![2; 16]),
            is_keyframe: false,
        });

        let frame3 = Arc::new(DecodedFrame {
            frame_index: 2,
            pts_us: 66_666,
            width: 1920,
            height: 1080,
            rgba_data: Some(vec![3; 16]),
            is_keyframe: false,
        });

        cache.insert(path.clone(), frame1);
        cache.insert(path.clone(), frame2);
        assert_eq!(cache.len(), 2);

        // Frame 1 is evicted when Frame 3 is added
        cache.insert(path.clone(), frame3);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&path, 0, 5_000).is_none());
        assert!(cache.get(&path, 33_333, 5_000).is_some());
        assert!(cache.get(&path, 66_666, 5_000).is_some());
    }

    #[test]
    fn test_prefetch_lookahead_queues_and_skips_cached() {
        let service = AsyncDecoderService::new(16);
        let path = PathBuf::from("test_clip.mp4");
        let clip_id = ClipId::new();

        // Prefetch 4 frames ahead starting from 0 us at 33.3ms intervals
        let queued = service.prefetch_lookahead(clip_id, path.clone(), 0, 33_333, 4);
        assert_eq!(queued, 4);

        // Insert a frame at 33_333 into the cache manually
        let frame = Arc::new(DecodedFrame {
            frame_index: 1,
            pts_us: 33_333,
            width: 1920,
            height: 1080,
            rgba_data: None,
            is_keyframe: false,
        });
        service.cache.lock().unwrap().insert(path.clone(), frame);

        // Prefetching again should skip the frame at 33_333
        let queued_again = service.prefetch_lookahead(clip_id, path.clone(), 0, 33_333, 2);
        // Frame 1 (33_333) is skipped, Frame 2 (66_666) is queued
        assert_eq!(queued_again, 1);
    }

    #[test]
    fn test_prefetch_directional() {
        let service = AsyncDecoderService::new(16);
        let path = PathBuf::from("test_clip.mp4");
        let clip_id = ClipId::new();

        let queued_forward = service.prefetch_directional(
            clip_id,
            path.clone(),
            100_000,
            PlayDirection::Forward,
            33_333,
            3,
        );
        assert_eq!(queued_forward, 3);

        let queued_reverse = service.prefetch_directional(
            clip_id,
            path.clone(),
            100_000,
            PlayDirection::Reverse,
            33_333,
            2,
        );
        assert_eq!(queued_reverse, 2);

        let queued_stationary = service.prefetch_directional(
            clip_id,
            path,
            100_000,
            PlayDirection::Stationary,
            33_333,
            4,
        );
        // Half forward (2) + half reverse (2) = 4
        assert_eq!(queued_stationary, 4);
    }

    #[test]
    fn test_bump_generation_lifecycle() {
        let service = AsyncDecoderService::new(16);
        let gen1 = service.current_generation();
        assert_eq!(gen1, 1);

        let gen2 = service.bump_generation();
        assert_eq!(gen2, 2);
        assert_eq!(service.current_generation(), 2);
    }
}
