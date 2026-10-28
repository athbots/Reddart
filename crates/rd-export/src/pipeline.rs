//! Export pipeline: multi-stage parallel architecture.
//!
//! Section 3.10 of the Technical Proposal:
//! Decode, GPU render and hardware encode run in parallel stages joined
//! by bounded queues, so each unit stays busy.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::preset::ExportPreset;

/// Real-time export progress metrics.
#[derive(Debug, Clone)]
pub struct ExportProgress {
    /// Index of the frame currently being processed.
    pub current_frame: u64,
    /// Total frames in the project export.
    pub total_frames: u64,
    /// Progress fraction from 0.0 to 1.0.
    pub fraction: f32,
    /// Export speed multiplier relative to real-time (e.g. 1.8x).
    pub speed_multiplier: f64,
    /// Estimated time remaining in seconds.
    pub eta_seconds: f64,
}

/// Controller and state manager for an active export job.
pub struct ExportJob {
    preset: ExportPreset,
    total_frames: u64,
    start_time: Instant,
    cancel_flag: Arc<AtomicBool>,
}

impl ExportJob {
    pub fn new(preset: ExportPreset, total_frames: u64) -> Self {
        Self {
            preset,
            total_frames: total_frames.max(1),
            start_time: Instant::now(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Compute progress snapshot for frame `current_frame`.
    pub fn progress(&self, current_frame: u64) -> ExportProgress {
        let fraction = (current_frame as f32 / self.total_frames as f32).min(1.0);
        let elapsed = self.start_time.elapsed().as_secs_f64();

        let (speed_multiplier, eta_seconds) = if elapsed > 0.001 && current_frame > 0 {
            let processed_video_seconds = current_frame as f64 / self.preset.fps;
            let speed = processed_video_seconds / elapsed;

            let remaining_frames = self.total_frames.saturating_sub(current_frame);
            let frames_per_second = current_frame as f64 / elapsed;
            let eta = if frames_per_second > 0.001 {
                remaining_frames as f64 / frames_per_second
            } else {
                0.0
            };

            (speed, eta)
        } else {
            (0.0, 0.0)
        };

        ExportProgress {
            current_frame,
            total_frames: self.total_frames,
            fraction,
            speed_multiplier,
            eta_seconds,
        }
    }

    /// Cancel the ongoing export job.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Release);
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Acquire)
    }

    pub fn preset(&self) -> &ExportPreset {
        &self.preset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_progress_calculation() {
        let preset = ExportPreset::youtube_1080p();
        let job = ExportJob::new(preset, 300); // 10 seconds of 30fps

        let progress = job.progress(150);
        assert!((progress.fraction - 0.5).abs() < 1e-4);
        assert_eq!(progress.current_frame, 150);
        assert_eq!(progress.total_frames, 300);
    }

    #[test]
    fn test_export_cancellation() {
        let preset = ExportPreset::youtube_1080p();
        let job = ExportJob::new(preset, 100);

        assert!(!job.is_cancelled());
        job.cancel();
        assert!(job.is_cancelled());
    }
}
