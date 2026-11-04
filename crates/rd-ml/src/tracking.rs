//! Object and Face Tracking.
//!
//! Section W3.4 of the Technical Proposal:
//! Classical Lucas-Kanade / small detector tracker running on 540p proxy frames.
//! Emits animated `KeyframeTrack`s so stickers, text, and PIP layers follow moving objects.

use rd_core::{Easing, Keyframe, KeyframeTrack, TimestampUs};
use serde::{Deserialize, Serialize};

/// 2D Normalized bounding box [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
}

/// A tracked trajectory over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedPath {
    pub points: Vec<(TimestampUs, BoundingBox)>,
}

impl TrackedPath {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    pub fn add_point(&mut self, timestamp_us: TimestampUs, bbox: BoundingBox) {
        self.points.push((timestamp_us, bbox));
    }

    /// Convert the tracked center positions into animated KeyframeTracks for X and Y position.
    pub fn to_keyframe_tracks(&self) -> (KeyframeTrack, KeyframeTrack) {
        let mut track_x = KeyframeTrack::new("pos_x");
        let mut track_y = KeyframeTrack::new("pos_y");

        for &(ts, bbox) in &self.points {
            let (cx, cy) = bbox.center();
            track_x.add_keyframe(Keyframe {
                time_us: ts,
                value: cx,
                easing: Easing::Linear,
            });
            track_y.add_keyframe(Keyframe {
                time_us: ts,
                value: cy,
                easing: Easing::Linear,
            });
        }

        (track_x, track_y)
    }
}

impl Default for TrackedPath {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracking_to_keyframes() {
        let mut path = TrackedPath::new();
        path.add_point(
            0,
            BoundingBox {
                x: 0.1,
                y: 0.1,
                width: 0.2,
                height: 0.2,
            },
        );
        path.add_point(
            1_000_000,
            BoundingBox {
                x: 0.5,
                y: 0.3,
                width: 0.2,
                height: 0.2,
            },
        );

        let (kx, ky) = path.to_keyframe_tracks();
        assert_eq!(kx.keyframes().len(), 2);
        assert_eq!(ky.keyframes().len(), 2);

        // Center at t=0: x=0.2, y=0.2
        assert!((kx.evaluate(0).unwrap() - 0.2).abs() < 1e-4);
        assert!((ky.evaluate(0).unwrap() - 0.2).abs() < 1e-4);

        // Center at t=0.5s: midpoint interpolation x=0.4, y=0.3
        assert!((kx.evaluate(500_000).unwrap() - 0.4).abs() < 1e-3);
        assert!((ky.evaluate(500_000).unwrap() - 0.3).abs() < 1e-3);
    }
}
