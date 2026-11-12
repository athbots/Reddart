//! Auto Reframe: Intelligent Aspect Ratio Adaptation & Kinematic Camera Tracking.
//!
//! Converts 16:9 landscape video into 9:16 vertical (Shorts/Reels/TikTok), 1:1 square,
//! or 4:5 portrait formats. Employs dead-band kinematic camera path filtering, scene-cut
//! jump detection, and rule-of-thirds headroom preservation for professional framing.

use crate::saliency::{SaliencyConfig, SaliencyDetector};
use crate::tracking::BoundingBox;
use rd_core::{Easing, Keyframe, KeyframeTrack, TimestampUs};
use serde::{Deserialize, Serialize};

/// Target aspect ratio format for auto-reframing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TargetAspect {
    /// 9:16 Vertical (TikTok, Instagram Reels, YouTube Shorts).
    Vertical9x16,
    /// 1:1 Square (Instagram Feed, Profile Cards).
    Square1x1,
    /// 4:5 Portrait (Instagram Carousel).
    Portrait4x5,
    /// 21:9 Cinematic Ultrawide.
    Cinematic21x9,
    /// Custom aspect ratio (width / height).
    Custom(f32),
}

impl TargetAspect {
    pub fn ratio(&self) -> f32 {
        match self {
            Self::Vertical9x16 => 9.0 / 16.0,
            Self::Square1x1 => 1.0,
            Self::Portrait4x5 => 4.0 / 5.0,
            Self::Cinematic21x9 => 21.0 / 9.0,
            Self::Custom(r) => *r,
        }
    }
}

/// Kinematic motion and dead-band configuration for auto-reframing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReframeConfig {
    /// Target aspect ratio.
    pub target_aspect: TargetAspect,
    /// Dead-zone width fraction [0.0, 0.5]. Subject movement inside this window causes 0 camera movement.
    pub dead_zone_width: f32,
    /// Dead-zone height fraction [0.0, 0.5].
    pub dead_zone_height: f32,
    /// Exponential smoothing factor [0.01, 1.0].
    pub smoothing_factor: f32,
    /// Maximum pan velocity in normalized units per second.
    pub max_pan_speed: f32,
    /// Subject displacement jump that triggers an instantaneous scene-cut pan instead of dragging.
    pub scene_cut_threshold: f32,
    /// Rule-of-thirds headroom bias (shifts camera upward to place faces near the upper third line).
    pub headroom_bias: f32,
}

impl Default for ReframeConfig {
    fn default() -> Self {
        Self {
            target_aspect: TargetAspect::Vertical9x16,
            dead_zone_width: 0.15,
            dead_zone_height: 0.12,
            smoothing_factor: 0.15,
            max_pan_speed: 1.5,
            scene_cut_threshold: 0.38,
            headroom_bias: 0.10,
        }
    }
}

/// Keyframe tracks emitted by the reframe solver for camera pan and zoom.
#[derive(Debug, Clone)]
pub struct ReframeTracks {
    /// Normalized horizontal pan offset in pixels [-max_pan_px, max_pan_px].
    pub pan_x: KeyframeTrack,
    /// Normalized vertical pan offset in pixels.
    pub pan_y: KeyframeTrack,
    /// Dynamic zoom factor [1.0, 2.5].
    pub zoom: KeyframeTrack,
}

/// Intelligent Auto Reframe solver.
pub struct AutoReframer {
    pub config: ReframeConfig,
    saliency_detector: SaliencyDetector,
}

impl AutoReframer {
    pub fn new(target: TargetAspect) -> Self {
        Self {
            config: ReframeConfig {
                target_aspect: target,
                ..Default::default()
            },
            saliency_detector: SaliencyDetector::new(SaliencyConfig::default()),
        }
    }

    pub fn with_config(config: ReframeConfig) -> Self {
        Self {
            config,
            saliency_detector: SaliencyDetector::new(SaliencyConfig::default()),
        }
    }

    /// Legacy / simple API: Compute pan position keyframes to keep the subject centered in X.
    pub fn solve_reframe_path(
        &self,
        source_width: u32,
        source_height: u32,
        detections: &[(TimestampUs, BoundingBox)],
    ) -> KeyframeTrack {
        let tracks = self.solve_kinematic_reframe_path(source_width, source_height, detections);
        tracks.pan_x
    }

    /// Solve full kinematic camera trajectories with dead-band filtering, velocity damping, and cut handling.
    pub fn solve_kinematic_reframe_path(
        &self,
        source_width: u32,
        source_height: u32,
        detections: &[(TimestampUs, BoundingBox)],
    ) -> ReframeTracks {
        let mut track_x = KeyframeTrack::new("reframe_pan_x");
        let mut track_y = KeyframeTrack::new("reframe_pan_y");
        let mut track_zoom = KeyframeTrack::new("reframe_zoom");

        if detections.is_empty() {
            return ReframeTracks {
                pan_x: track_x,
                pan_y: track_y,
                zoom: track_zoom,
            };
        }

        let src_aspect = source_width as f32 / source_height as f32;
        let dst_aspect = self.config.target_aspect.ratio();

        // Effective visible width fraction in normalized coordinates [0.0, 1.0]
        let crop_width_fraction = (dst_aspect / src_aspect).clamp(0.05, 1.0);
        let max_pan_x = (1.0 - crop_width_fraction) * 0.5;

        // Effective visible height fraction
        let crop_height_fraction = (src_aspect / dst_aspect).clamp(0.05, 1.0);
        let max_pan_y = (1.0 - crop_height_fraction) * 0.5;

        let mut cam_x = detections[0].1.center().0;
        let mut cam_y = detections[0].1.center().1;
        let mut last_ts = detections[0].0;

        for &(ts, bbox) in detections {
            let (target_cx, target_cy) = bbox.center();
            let dt_sec = ((ts.saturating_sub(last_ts)) as f32 / 1_000_000.0).max(0.001);

            // 1. Scene cut / jump detection
            let dist = ((target_cx - cam_x).powi(2) + (target_cy - cam_y).powi(2)).sqrt();
            if dist > self.config.scene_cut_threshold {
                // Instantaneous camera jump cut
                cam_x = target_cx;
                cam_y = target_cy;
            } else {
                // 2. Dead-band filter: camera only moves if subject leaves the dead-zone
                let dead_w = self.config.dead_zone_width * crop_width_fraction;
                let dead_h = self.config.dead_zone_height * crop_height_fraction;

                let desired_cam_x = if target_cx > cam_x + dead_w {
                    target_cx - dead_w
                } else if target_cx < cam_x - dead_w {
                    target_cx + dead_w
                } else {
                    cam_x // inside dead-zone, stay still
                };

                let desired_cam_y = if target_cy > cam_y + dead_h {
                    target_cy - dead_h
                } else if target_cy < cam_y - dead_h {
                    target_cy + dead_h
                } else {
                    cam_y
                };

                // 3. Velocity-limited exponential smoothing
                let alpha = self.config.smoothing_factor;
                let smooth_step_x = (desired_cam_x - cam_x) * alpha;
                let smooth_step_y = (desired_cam_y - cam_y) * alpha;

                let max_step = self.config.max_pan_speed * dt_sec;
                let step_x = smooth_step_x.clamp(-max_step, max_step);
                let step_y = smooth_step_y.clamp(-max_step, max_step);

                cam_x += step_x;
                cam_y += step_y;
            }

            last_ts = ts;

            // 4. Apply headroom bias (rule-of-thirds face compensation)
            let biased_cam_y = cam_y - self.config.headroom_bias * crop_height_fraction;

            // 5. Clamp camera center so crop window stays inside frame boundaries
            let pan_offset_x = (cam_x - 0.5).clamp(-max_pan_x, max_pan_x);
            let pan_offset_y = (biased_cam_y - 0.5).clamp(-max_pan_y, max_pan_y);

            let pixel_shift_x = -pan_offset_x * source_width as f32;
            let pixel_shift_y = -pan_offset_y * source_height as f32;

            // Dynamic zoom adaptation if subject fills too much of viewport
            let subject_ratio = bbox.width / crop_width_fraction;
            let dynamic_zoom = if subject_ratio > 0.85 {
                (0.85 / subject_ratio).clamp(0.85, 1.0)
            } else {
                1.0_f32
            };

            track_x.add_keyframe(Keyframe {
                time_us: ts,
                value: pixel_shift_x,
                easing: Easing::Linear,
            });
            track_y.add_keyframe(Keyframe {
                time_us: ts,
                value: pixel_shift_y,
                easing: Easing::Linear,
            });
            track_zoom.add_keyframe(Keyframe {
                time_us: ts,
                value: dynamic_zoom,
                easing: Easing::Linear,
            });
        }

        ReframeTracks {
            pan_x: track_x,
            pan_y: track_y,
            zoom: track_zoom,
        }
    }

    /// Automatically reframe raw video frame buffers using spatial saliency detection.
    pub fn reframe_video_frames(
        &self,
        source_width: u32,
        source_height: u32,
        frames: &[(TimestampUs, &[u8])],
        channels: usize,
    ) -> ReframeTracks {
        let mut detections = Vec::with_capacity(frames.len());
        for &(ts, frame_bytes) in frames {
            let bbox = self.saliency_detector.detect_salient_subject(
                frame_bytes,
                source_width as usize,
                source_height as usize,
                channels,
            );
            detections.push((ts, bbox));
        }

        self.solve_kinematic_reframe_path(source_width, source_height, &detections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_reframe_pan_path() {
        let reframer = AutoReframer::new(TargetAspect::Vertical9x16);

        let detections = vec![
            (
                0,
                BoundingBox {
                    x: 0.1,
                    y: 0.2,
                    width: 0.2,
                    height: 0.4,
                },
            ),
            (
                1_000_000,
                BoundingBox {
                    x: 0.4,
                    y: 0.2,
                    width: 0.2,
                    height: 0.4,
                },
            ),
            (
                2_000_000,
                BoundingBox {
                    x: 0.7,
                    y: 0.2,
                    width: 0.2,
                    height: 0.4,
                },
            ),
        ];

        let track = reframer.solve_reframe_path(1920, 1080, &detections);
        assert_eq!(track.keyframes().len(), 3);

        let k0 = track.keyframes()[0].value;
        let k2 = track.keyframes()[2].value;
        assert!(
            k0 > k2,
            "Pan offset must shift leftwards to compensate for rightward movement"
        );
    }

    #[test]
    fn test_dead_zone_suppresses_micro_jitter() {
        let config = ReframeConfig {
            dead_zone_width: 0.20, // 20% dead-zone
            ..Default::default()
        };
        let reframer = AutoReframer::with_config(config);

        // Subject has slight tremor / breathing (moving by 0.02)
        let detections = vec![
            (
                0,
                BoundingBox {
                    x: 0.40,
                    y: 0.30,
                    width: 0.20,
                    height: 0.40,
                },
            ),
            (
                33_333,
                BoundingBox {
                    x: 0.41,
                    y: 0.30,
                    width: 0.20,
                    height: 0.40,
                },
            ),
            (
                66_666,
                BoundingBox {
                    x: 0.39,
                    y: 0.30,
                    width: 0.20,
                    height: 0.40,
                },
            ),
        ];

        let tracks = reframer.solve_kinematic_reframe_path(1920, 1080, &detections);
        let k0 = tracks.pan_x.keyframes()[0].value;
        let k1 = tracks.pan_x.keyframes()[1].value;
        let k2 = tracks.pan_x.keyframes()[2].value;

        // Micro-jitter must produce zero camera displacement!
        assert_eq!(
            k0, k1,
            "Camera should remain completely stationary during dead-zone micro-jitter"
        );
        assert_eq!(
            k1, k2,
            "Camera should remain completely stationary during dead-zone micro-jitter"
        );
    }

    #[test]
    fn test_scene_cut_detection_triggers_immediate_cut() {
        let reframer = AutoReframer::new(TargetAspect::Square1x1);

        // Subject abruptly teleports from left (x=0.1) to far right (x=0.8) across scene cut
        let detections = vec![
            (
                0,
                BoundingBox {
                    x: 0.05,
                    y: 0.2,
                    width: 0.1,
                    height: 0.4,
                },
            ),
            (
                1_000_000,
                BoundingBox {
                    x: 0.85,
                    y: 0.2,
                    width: 0.1,
                    height: 0.4,
                },
            ),
        ];

        let tracks = reframer.solve_kinematic_reframe_path(1920, 1080, &detections);
        let k0 = tracks.pan_x.keyframes()[0].value;
        let k1 = tracks.pan_x.keyframes()[1].value;

        // With scene cut threshold, camera must immediately snap to the rightward boundary
        assert!(
            (k0 - k1).abs() > 400.0,
            "Camera must cut instantly across scene break, delta: {}",
            (k0 - k1).abs()
        );
    }
}
