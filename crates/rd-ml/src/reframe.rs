//! Auto Reframe: Intelligent Aspect Ratio Adaptation.
//!
//! Section W3.6 of the Technical Proposal:
//! Saliency and subject tracking path solver that converts 16:9 landscape to 9:16 vertical
//! or 1:1 square while maintaining dynamic framing and smooth camera panning.

use rd_core::{Easing, Keyframe, KeyframeTrack, TimestampUs};
use crate::tracking::BoundingBox;
use serde::{Deserialize, Serialize};

/// Target aspect ratio format for auto-reframing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TargetAspect {
    /// 9:16 Vertical (TikTok, Instagram Reels, YouTube Shorts).
    Vertical9x16,
    /// 1:1 Square (Instagram Feed).
    Square1x1,
    /// 4:5 Portrait.
    Portrait4x5,
}

impl TargetAspect {
    pub fn ratio(&self) -> f32 {
        match self {
            Self::Vertical9x16 => 9.0 / 16.0,
            Self::Square1x1 => 1.0,
            Self::Portrait4x5 => 4.0 / 5.0,
        }
    }
}

/// Auto Reframe solver.
pub struct AutoReframer {
    target: TargetAspect,
    smoothing_factor: f32,
}

impl AutoReframer {
    pub fn new(target: TargetAspect) -> Self {
        Self {
            target,
            smoothing_factor: 0.15,
        }
    }

    /// Compute pan position keyframes to keep the subject centered in the re-framed viewport.
    /// Input is a series of tracked subject bounding boxes in source coordinates.
    pub fn solve_reframe_path(
        &self,
        source_width: u32,
        source_height: u32,
        detections: &[(TimestampUs, BoundingBox)],
    ) -> KeyframeTrack {
        let mut track_x = KeyframeTrack::new("reframe_pan_x");

        if detections.is_empty() {
            return track_x;
        }

        let src_aspect = source_width as f32 / source_height as f32;
        let dst_aspect = self.target.ratio();

        // Effective visible width fraction in normalized coordinates [0.0, 1.0]
        let crop_width_fraction = (dst_aspect / src_aspect).clamp(0.1, 1.0);
        let max_pan = (1.0 - crop_width_fraction) * 0.5;

        let mut smoothed_center = detections[0].1.center().0;

        for &(ts, bbox) in detections {
            let (target_cx, _) = bbox.center();
            // Smoothly move camera towards target center (EMA)
            smoothed_center = smoothed_center * (1.0 - self.smoothing_factor) + target_cx * self.smoothing_factor;

            // Compute normalized offset from center (0.5)
            let pan_offset = (smoothed_center - 0.5).clamp(-max_pan, max_pan);
            // Translate into pixel shift
            let pixel_shift = -pan_offset * source_width as f32;

            track_x.add_keyframe(Keyframe {
                time_us: ts,
                value: pixel_shift,
                easing: Easing::Linear,
            });
        }

        track_x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_reframe_pan_path() {
        let reframer = AutoReframer::new(TargetAspect::Vertical9x16);

        let detections = vec![
            (0, BoundingBox { x: 0.1, y: 0.2, width: 0.2, height: 0.4 }),
            (1_000_000, BoundingBox { x: 0.4, y: 0.2, width: 0.2, height: 0.4 }),
            (2_000_000, BoundingBox { x: 0.7, y: 0.2, width: 0.2, height: 0.4 }),
        ];

        let track = reframer.solve_reframe_path(1920, 1080, &detections);
        assert_eq!(track.keyframes().len(), 3);

        // Subject moves from left (0.2 center) to right (0.8 center).
        // Pan offset must shift to follow the subject.
        let k0 = track.keyframes()[0].value;
        let k2 = track.keyframes()[2].value;
        assert!(k0 > k2, "Pan offset must shift leftwards to compensate for rightward movement");
    }
}

