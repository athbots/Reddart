//! Video Stabilization.
//!
//! Section W3.6 of the Technical Proposal:
//! Feature trajectory motion smoothing to eliminate camera shake.
//! Calculates camera motion path and outputs compensatory translation keyframes.

use rd_core::{Easing, Keyframe, KeyframeTrack, TimestampUs};
use serde::{Deserialize, Serialize};

/// 2D Camera motion displacement for a single frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameMotion {
    pub timestamp_us: TimestampUs,
    pub dx: f32,
    pub dy: f32,
}

/// Video stabilizer engine.
pub struct VideoStabilizer {
    /// Smoothing window radius in frames (e.g. 15 frames = ~0.5s at 30fps).
    window_radius: usize,
    /// Maximum allowed crop ratio to prevent excessive zoom (e.g. 0.15 = 15%).
    max_crop: f32,
}

impl VideoStabilizer {
    pub fn new(window_radius: usize, max_crop: f32) -> Self {
        Self {
            window_radius: window_radius.max(1),
            max_crop: max_crop.clamp(0.01, 0.3),
        }
    }

    /// Calculate smoothed motion path and return compensatory (offset_x, offset_y) keyframe tracks.
    pub fn compute_compensation(
        &self,
        trajectory: &[FrameMotion],
    ) -> (KeyframeTrack, KeyframeTrack) {
        let mut track_x = KeyframeTrack::new("stab_x");
        let mut track_y = KeyframeTrack::new("stab_y");

        if trajectory.is_empty() {
            return (track_x, track_y);
        }

        // 1. Accumulate raw cumulative trajectory
        let mut cum_x = Vec::with_capacity(trajectory.len());
        let mut cum_y = Vec::with_capacity(trajectory.len());
        let mut curr_x = 0.0_f32;
        let mut curr_y = 0.0_f32;

        for m in trajectory {
            curr_x += m.dx;
            curr_y += m.dy;
            cum_x.push(curr_x);
            cum_y.push(curr_y);
        }

        let n = trajectory.len();

        // 2. Compute smoothed trajectory via moving box average
        for i in 0..n {
            let start = i.saturating_sub(self.window_radius);
            let end = (i + self.window_radius).min(n - 1);
            let count = (end - start + 1) as f32;

            let avg_x: f32 = cum_x[start..=end].iter().sum::<f32>() / count;
            let avg_y: f32 = cum_y[start..=end].iter().sum::<f32>() / count;

            // Compensatory displacement is smoothed minus raw (canceling jitter)
            let comp_x = (avg_x - cum_x[i]).clamp(-self.max_crop * 1000.0, self.max_crop * 1000.0);
            let comp_y = (avg_y - cum_y[i]).clamp(-self.max_crop * 1000.0, self.max_crop * 1000.0);

            let ts = trajectory[i].timestamp_us;
            track_x.add_keyframe(Keyframe {
                time_us: ts,
                value: comp_x,
                easing: Easing::Linear,
            });
            track_y.add_keyframe(Keyframe {
                time_us: ts,
                value: comp_y,
                easing: Easing::Linear,
            });
        }

        (track_x, track_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stabilizer_cancels_high_frequency_jitter() {
        let stabilizer = VideoStabilizer::new(5, 0.2);

        // Simulate 20 frames with high frequency jitter (+5, -5, +5, -5)
        let mut trajectory = Vec::new();
        for i in 0..20 {
            let jitter = if i % 2 == 0 { 5.0 } else { -5.0 };
            trajectory.push(FrameMotion {
                timestamp_us: i as i64 * 33_333,
                dx: jitter,
                dy: jitter * 0.5,
            });
        }

        let (kx, ky) = stabilizer.compute_compensation(&trajectory);
        assert_eq!(kx.keyframes().len(), 20);
        assert_eq!(ky.keyframes().len(), 20);

        // Compensation should oppose raw jitter
        let k0 = kx.keyframes()[0].value;
        let k1 = kx.keyframes()[1].value;
        assert!(
            k0 * k1 < 0.0,
            "Compensation should alternate to cancel jitter"
        );
    }
}
