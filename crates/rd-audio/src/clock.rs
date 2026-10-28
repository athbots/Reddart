//! Master Audio Clock: lock-free, zero-allocation reference clock for video sync.
//!
//! Section 3.5 of the Technical Proposal:
//! Audio is decoded to PCM ahead of time and mixed in a native real-time thread.
//! No allocation, locks or logging in the audio callback.
//! Video follows the audio clock: late frames are dropped or held, so audio never stutters.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use rd_core::TimestampUs;

/// Lock-free master clock tracking current presentation time based on audio output.
#[derive(Debug)]
pub struct AudioClock {
    /// Current playhead position in microseconds.
    current_time_us: AtomicI64,
    /// Whether the clock is currently ticking (playing).
    is_playing: AtomicBool,
}

impl AudioClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            current_time_us: AtomicI64::new(0),
            is_playing: AtomicBool::new(false),
        })
    }

    /// Read the current presentation time in microseconds. Lock-free.
    #[inline]
    pub fn now_us(&self) -> TimestampUs {
        self.current_time_us.load(Ordering::Acquire)
    }

    /// Advance the master clock by a number of audio samples. Lock-free.
    /// Called directly inside the high-priority real-time audio callback.
    #[inline]
    pub fn advance_samples(&self, sample_frames: usize, sample_rate: u32) {
        if self.is_playing.load(Ordering::Relaxed) {
            let delta_us = (sample_frames as i64 * 1_000_000) / sample_rate as i64;
            self.current_time_us.fetch_add(delta_us, Ordering::Release);
        }
    }

    /// Seek to a target timestamp in microseconds. Lock-free.
    pub fn seek_to(&self, target_us: TimestampUs) {
        self.current_time_us.store(target_us, Ordering::Release);
    }

    /// Start or pause the clock.
    pub fn set_playing(&self, playing: bool) {
        self.is_playing.store(playing, Ordering::Release);
    }

    /// Whether the clock is actively playing.
    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Acquire)
    }

    /// Determine whether a video frame should be rendered, held, or dropped.
    /// - `frame_pts_us`: Presentation timestamp of the video frame.
    /// - `frame_duration_us`: Duration of one video frame (e.g. 16,666us for 60fps).
    pub fn sync_decision(&self, frame_pts_us: TimestampUs, frame_duration_us: i64) -> SyncDecision {
        let clock_us = self.now_us();
        let delta = frame_pts_us - clock_us;

        if delta < -frame_duration_us {
            // Frame is later than 1 frame period behind the master clock -> DROP
            SyncDecision::Drop
        } else if delta > frame_duration_us / 2 {
            // Frame is ahead of current audio position -> HOLD until clock catches up
            SyncDecision::Hold
        } else {
            // Frame is on time -> RENDER
            SyncDecision::Render
        }
    }
}

/// Synchronization decision for a video frame relative to the master audio clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    /// Frame is on time, display now.
    Render,
    /// Frame arrived too late, drop to avoid stutter.
    Drop,
    /// Frame is early, hold and render again next tick.
    Hold,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_advancement() {
        let clock = AudioClock::new();
        clock.seek_to(1_000_000);
        assert_eq!(clock.now_us(), 1_000_000);

        clock.set_playing(true);
        // Advance 480 samples at 48kHz = 10,000 us (10ms)
        clock.advance_samples(480, 48_000);
        assert_eq!(clock.now_us(), 1_010_000);
    }

    #[test]
    fn test_sync_decision() {
        let clock = AudioClock::new();
        clock.seek_to(1_000_000);

        let frame_duration_us = 16_667; // ~60fps

        // Exactly on time -> Render
        assert_eq!(
            clock.sync_decision(1_000_000, frame_duration_us),
            SyncDecision::Render
        );

        // Slightly ahead (e.g. +5ms) -> Render (within window)
        assert_eq!(
            clock.sync_decision(1_005_000, frame_duration_us),
            SyncDecision::Render
        );

        // Too early (+20ms) -> Hold
        assert_eq!(
            clock.sync_decision(1_020_000, frame_duration_us),
            SyncDecision::Hold
        );

        // Too late (-25ms) -> Drop
        assert_eq!(
            clock.sync_decision(975_000, frame_duration_us),
            SyncDecision::Drop
        );
    }
}

