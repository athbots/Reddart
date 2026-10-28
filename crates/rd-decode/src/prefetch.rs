//! Predictive prefetch queue and ring buffer for zero-latency seeking.
//!
//! Section 3.2 of the Technical Proposal:
//! The scheduler reads playhead velocity and direction and decodes 12 to 24
//! frames ahead (or behind, if scrubbing in reverse) into a ring buffer.

use std::collections::VecDeque;
use rd_core::TimestampUs;
use crate::DecodedFrame;

/// Direction of playhead movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayDirection {
    Forward,
    Reverse,
    Stationary,
}

/// Ring buffer holding prefetched frames around the playhead.
pub struct PrefetchRingBuffer {
    /// Ring buffer of prefetched frames, ordered by presentation timestamp.
    frames: VecDeque<DecodedFrame>,
    /// Capacity of the ring buffer (e.g. 16 to 24 frames).
    capacity: usize,
    /// Current estimated playback velocity (frames per second, negative for reverse).
    velocity_fps: f64,
    /// Current playback direction.
    direction: PlayDirection,
}

impl PrefetchRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
            velocity_fps: 30.0,
            direction: PlayDirection::Forward,
        }
    }

    /// Push a newly decoded frame into the buffer.
    /// If full, evicts the frame farthest from the playhead direction.
    pub fn push_frame(&mut self, frame: DecodedFrame) {
        if self.frames.len() >= self.capacity {
            match self.direction {
                PlayDirection::Forward => {
                    // Evict oldest (smallest PTS)
                    self.frames.pop_front();
                }
                PlayDirection::Reverse => {
                    // Evict newest (largest PTS)
                    self.frames.pop_back();
                }
                PlayDirection::Stationary => {
                    self.frames.pop_front();
                }
            }
        }
        self.frames.push_back(frame);
    }

    /// Look up a prefetched frame closest to the target timestamp (within tolerance).
    pub fn get_frame_at(&self, target_pts_us: TimestampUs, tolerance_us: i64) -> Option<&DecodedFrame> {
        self.frames
            .iter()
            .min_by_key(|f| (f.pts_us - target_pts_us).abs())
            .filter(|f| (f.pts_us - target_pts_us).abs() <= tolerance_us)
    }

    /// Update playhead tracking state to optimize prefetch direction.
    pub fn update_playhead(&mut self, current_pts_us: TimestampUs, prev_pts_us: TimestampUs, delta_time_s: f64) {
        if delta_time_s <= 0.0 {
            return;
        }

        let pts_delta_s = (current_pts_us - prev_pts_us) as f64 / 1_000_000.0;
        self.velocity_fps = (pts_delta_s / delta_time_s) * 30.0; // Normalized to 30fps baseline

        if pts_delta_s > 0.001 {
            self.direction = PlayDirection::Forward;
        } else if pts_delta_s < -0.001 {
            self.direction = PlayDirection::Reverse;
        } else {
            self.direction = PlayDirection::Stationary;
        }
    }

    /// Flush the prefetch buffer (e.g. on discontinuous jump/seek).
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn direction(&self) -> PlayDirection {
        self.direction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_push_and_lookup() {
        let mut buffer = PrefetchRingBuffer::new(5);

        for i in 0..5 {
            buffer.push_frame(DecodedFrame {
                frame_index: i,
                pts_us: i * 33_333,
                width: 1920,
                height: 1080,
                rgba_data: None,
                is_keyframe: i == 0,
            });
        }

        assert_eq!(buffer.len(), 5);

        // Find frame at ~66ms
        let found = buffer.get_frame_at(66_666, 5_000).expect("Frame found");
        assert_eq!(found.frame_index, 2);

        // Outside tolerance
        assert!(buffer.get_frame_at(500_000, 10_000).is_none());
    }

    #[test]
    fn test_ring_buffer_eviction() {
        let mut buffer = PrefetchRingBuffer::new(3);

        for i in 0..4 {
            buffer.push_frame(DecodedFrame {
                frame_index: i,
                pts_us: i * 33_333,
                width: 1920,
                height: 1080,
                rgba_data: None,
                is_keyframe: false,
            });
        }

        // Buffer length capped at 3
        assert_eq!(buffer.len(), 3);
        // Oldest frame (index 0) was evicted
        assert!(buffer.get_frame_at(0, 100).is_none());
        assert!(buffer.get_frame_at(33_333, 100).is_some());
    }
}
