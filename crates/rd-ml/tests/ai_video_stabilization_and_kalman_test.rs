//! Comprehensive Integration Verification Suite for AI Video Stabilization,
//! Kalman Trajectory Smoothing, and Dynamic Margin Deshaking.

use rd_effects::{EffectRegistry, VideoStabilizerParams, apply_video_stabilizer};
use rd_ml::stabilize::{
    BorderFillMode, CornerConfig, CumulativeTrajectory, DynamicMarginManager, FeaturePoint,
    KalmanConfig, KalmanTrajectorySmoother, LucasKanadeConfig, MotionEstimator, OpticalFlowTracker,
    RansacConfig, ShiTomasiCornerDetector, VideoStabilizer, compute_luminance,
};
use rd_render::{CompositorLayer, composite_layers_over};

/// Helper: create a synthetic test frame with high-contrast corner patterns.
fn generate_synthetic_corner_frame(
    width: usize,
    height: usize,
    offset_x: f32,
    offset_y: f32,
) -> Vec<u8> {
    let mut buf = vec![40u8; width * height * 4];

    // Draw 4 distinct high-contrast textured blocks at known positions
    let centers = [
        (
            width as f32 * 0.25 + offset_x,
            height as f32 * 0.25 + offset_y,
        ),
        (
            width as f32 * 0.75 + offset_x,
            height as f32 * 0.25 + offset_y,
        ),
        (
            width as f32 * 0.25 + offset_x,
            height as f32 * 0.75 + offset_y,
        ),
        (
            width as f32 * 0.75 + offset_x,
            height as f32 * 0.75 + offset_y,
        ),
    ];

    for (cx, cy) in centers {
        for dy in -6..=6 {
            for dx in -6..=6 {
                let px = (cx + dx as f32).round() as isize;
                let py = (cy + dy as f32).round() as isize;

                if px >= 0 && px < width as isize && py >= 0 && py < height as isize {
                    let idx = (py as usize * width + px as usize) * 4;
                    // Checkerboard corner pattern within block
                    let is_white = (dx >= 0 && dy >= 0) || (dx < 0 && dy < 0);
                    let color = if is_white { 230u8 } else { 20u8 };
                    buf[idx] = color;
                    buf[idx + 1] = color;
                    buf[idx + 2] = color;
                    buf[idx + 3] = 255;
                }
            }
        }
    }

    buf
}

#[test]
fn test_shi_tomasi_corner_feature_detection_and_spatial_bucketing() {
    let w = 128;
    let h = 128;
    let frame = generate_synthetic_corner_frame(w, h, 0.0, 0.0);
    let luma = compute_luminance(&frame, w, h);

    let config = CornerConfig {
        max_corners: 50,
        quality_level: 0.05,
        min_distance: 10.0,
        grid_cols: 4,
        grid_rows: 4,
        margin_ratio: 0.05,
    };
    let detector = ShiTomasiCornerDetector::new(config);
    let features = detector.detect_features(&luma, w, h);

    assert!(
        features.len() >= 4,
        "Detector must find at least 4 corner features, found {}",
        features.len()
    );

    // Verify spatial bucketing distributes features across multiple quadrants
    let mut q1 = false;
    let mut q2 = false;
    let mut q3 = false;
    let mut q4 = false;

    let mid_x = w as f32 * 0.5;
    let mid_y = h as f32 * 0.5;

    for pt in &features {
        assert!(pt.response > 0.0, "Feature response must be positive");
        assert!(
            pt.x >= 6.0 && pt.x < (w - 6) as f32,
            "Feature x must be within margins"
        );
        assert!(
            pt.y >= 6.0 && pt.y < (h - 6) as f32,
            "Feature y must be within margins"
        );

        if pt.x < mid_x && pt.y < mid_y {
            q1 = true;
        } else if pt.x >= mid_x && pt.y < mid_y {
            q2 = true;
        } else if pt.x < mid_x && pt.y >= mid_y {
            q3 = true;
        } else {
            q4 = true;
        }
    }

    assert!(
        q1 && q2 && q3 && q4,
        "Features must span all 4 quadrants (q1:{}, q2:{}, q3:{}, q4:{})",
        q1,
        q2,
        q3,
        q4
    );
}

#[test]
fn test_lucas_kanade_subpixel_optical_flow_tracking() {
    let w = 128;
    let h = 128;

    // Shift frame 2 by subpixel vector (+2.0, -1.0)
    let shift_x = 2.0f32;
    let shift_y = -1.0f32;

    let frame1 = generate_synthetic_corner_frame(w, h, 0.0, 0.0);
    let frame2 = generate_synthetic_corner_frame(w, h, shift_x, shift_y);

    let luma1 = compute_luminance(&frame1, w, h);
    let luma2 = compute_luminance(&frame2, w, h);

    let detector = ShiTomasiCornerDetector::new(CornerConfig::default());
    let points = detector.detect_features(&luma1, w, h);
    assert!(!points.is_empty(), "Need points to track");

    let tracker = OpticalFlowTracker::new(LucasKanadeConfig {
        window_radius: 5,
        max_iterations: 12,
        epsilon: 0.01,
        min_eigenvalue: 1e-4,
        max_bidirectional_error: 1.5,
    });

    let matches = tracker.track_features(&luma1, &luma2, w, h, &points);
    assert!(
        matches.len() >= 4,
        "Expected at least 4 tracked matches, got {}",
        matches.len()
    );

    // Compute average tracking displacement error
    let mut total_err = 0.0f32;
    for (p1, p2) in &matches {
        let actual_dx = p2.x - p1.x;
        let actual_dy = p2.y - p1.y;

        let err_x = (actual_dx - shift_x).abs();
        let err_y = (actual_dy - shift_y).abs();
        total_err += (err_x * err_x + err_y * err_y).sqrt();
    }
    let mean_err = total_err / matches.len() as f32;

    assert!(
        mean_err < 0.25,
        "Mean subpixel tracking error must be < 0.25px, got {:.4}px",
        mean_err
    );
}

#[test]
fn test_bidirectional_consistency_rejects_occlusions() {
    let w = 128;
    let h = 128;

    let frame1 = generate_synthetic_corner_frame(w, h, 0.0, 0.0);
    let mut frame2 = generate_synthetic_corner_frame(w, h, 1.0, 1.0);

    // Occlude bottom-right quadrant completely with black square in frame 2
    for y in 64..128 {
        for x in 64..128 {
            let idx = (y * w + x) * 4;
            frame2[idx] = 0;
            frame2[idx + 1] = 0;
            frame2[idx + 2] = 0;
            frame2[idx + 3] = 255;
        }
    }

    let luma1 = compute_luminance(&frame1, w, h);
    let luma2 = compute_luminance(&frame2, w, h);

    let detector = ShiTomasiCornerDetector::new(CornerConfig::default());
    let points = detector.detect_features(&luma1, w, h);

    let tracker = OpticalFlowTracker::new(LucasKanadeConfig {
        window_radius: 5,
        max_iterations: 8,
        epsilon: 0.03,
        min_eigenvalue: 1e-4,
        max_bidirectional_error: 1.0,
    });

    let matches = tracker.track_features(&luma1, &luma2, w, h, &points);

    // Clean features in top-left/top-right should pass
    assert!(!matches.is_empty(), "Clean features should track");

    // Occluded quadrant points must have been rejected
    for (_, p2) in &matches {
        assert!(
            !(p2.x >= 64.0 && p2.y >= 64.0),
            "Occluded quadrant point at ({}, {}) should have been rejected!",
            p2.x,
            p2.y
        );
    }
}

#[test]
fn test_ransac_similarity_motion_estimation_rejects_outliers() {
    let mut matches = Vec::new();

    // 15 inliers moving with true camera motion: dx = 3.0, dy = -2.0, dtheta = 0.0, scale = 1.0
    for i in 0..15 {
        let x = 20.0 + (i as f32 * 6.0);
        let y = 30.0 + (i as f32 * 4.0);
        matches.push((
            FeaturePoint {
                x,
                y,
                response: 100.0,
            },
            FeaturePoint {
                x: x + 3.0,
                y: y - 2.0,
                response: 100.0,
            },
        ));
    }

    // 5 outliers moving in opposing directions (independently moving objects)
    for i in 0..5 {
        let x = 100.0 + (i as f32 * 5.0);
        let y = 80.0 + (i as f32 * 5.0);
        matches.push((
            FeaturePoint {
                x,
                y,
                response: 100.0,
            },
            FeaturePoint {
                x: x - 12.0, // Moving left rapidly
                y: y + 8.0,  // Moving down rapidly
                response: 100.0,
            },
        ));
    }

    let estimator = MotionEstimator::new(RansacConfig {
        max_iterations: 100,
        inlier_threshold_px: 1.5,
        min_inliers: 6,
    });

    let motion = estimator.estimate_motion(&matches, 200, 200);

    assert!(
        (motion.dx - 3.0).abs() < 0.2,
        "Estimated dx should be ~3.0, got {:.3}",
        motion.dx
    );
    assert!(
        (motion.dy - (-2.0)).abs() < 0.2,
        "Estimated dy should be ~ -2.0, got {:.3}",
        motion.dy
    );
    assert!(
        motion.dtheta.abs() < 0.02,
        "Estimated rotation should be ~0.0, got {:.4}",
        motion.dtheta
    );
    assert!(
        (motion.scale - 1.0).abs() < 0.05,
        "Estimated scale should be ~1.0, got {:.3}",
        motion.scale
    );
}

#[test]
fn test_kalman_rts_smoother_zero_phase_lag_and_jitter_suppression() {
    let smoother = KalmanTrajectorySmoother::new(KalmanConfig {
        process_noise_q: 0.02,
        measurement_noise_r: 40.0,
        enable_rts_backward_pass: true,
    });

    // 60-frame trajectory: slow intentional motion with high-frequency hand jitter
    let n = 60;
    let mut raw_x = Vec::with_capacity(n);
    let mut ground_truth = Vec::with_capacity(n);

    for i in 0..n {
        let t = i as f32;
        // Low-frequency sine wave camera pan
        let base = 50.0 * (t * 0.1).sin();
        ground_truth.push(base);

        // High frequency alternating hand tremor (+/- 6.0 pixels)
        let jitter = if i % 2 == 0 { 6.0 } else { -6.0 };
        raw_x.push(base + jitter);
    }

    let smoothed_x = smoother.smooth_channel(&raw_x);
    assert_eq!(smoothed_x.len(), n);

    let raw_traj = CumulativeTrajectory {
        points: raw_x
            .iter()
            .enumerate()
            .map(|(i, &x)| rd_ml::stabilize::TrajectoryPoint {
                timestamp_us: i as i64 * 33_333,
                x,
                y: 0.0,
                theta: 0.0,
                scale: 1.0,
            })
            .collect(),
    };

    let smooth_traj = CumulativeTrajectory {
        points: smoothed_x
            .iter()
            .enumerate()
            .map(|(i, &x)| rd_ml::stabilize::TrajectoryPoint {
                timestamp_us: i as i64 * 33_333,
                x,
                y: 0.0,
                theta: 0.0,
                scale: 1.0,
            })
            .collect(),
    };

    let suppression_db = CumulativeTrajectory::suppression_db(&raw_traj, &smooth_traj);
    assert!(
        suppression_db >= 12.0,
        "Expected jitter suppression >= 12 dB, achieved {:.2} dB",
        suppression_db
    );

    // Verify zero-phase lag: peak location in smoothed trajectory should align with ground truth peak
    let gt_peak_idx = ground_truth
        .iter()
        .enumerate()
        .take(30)
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(idx, _)| idx)
        .unwrap();

    let smooth_peak_idx = smoothed_x
        .iter()
        .enumerate()
        .take(30)
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(idx, _)| idx)
        .unwrap();

    assert!(
        (gt_peak_idx as isize - smooth_peak_idx as isize).abs() <= 1,
        "RTS smoothed peak index ({}) must align with ground truth peak ({}) with zero phase lag",
        smooth_peak_idx,
        gt_peak_idx
    );
}

#[test]
fn test_camera_intentional_panning_and_rotation_preservation() {
    let smoother = KalmanTrajectorySmoother::new(KalmanConfig::default());

    // 40 frames of intentional camera panning and rotation with hand jitter
    let n = 40;
    let mut pts = Vec::with_capacity(n);

    for i in 0..n {
        let t = i as f32;
        let jitter_x = if i % 2 == 0 { 2.0 } else { -2.0 };
        let jitter_y = if i % 3 == 0 { 1.5 } else { -1.5 };
        let jitter_th = if i % 2 == 0 { 0.005 } else { -0.005 };

        pts.push(rd_ml::stabilize::TrajectoryPoint {
            timestamp_us: i as i64 * 33_333,
            x: t * 4.0 + jitter_x,        // Monotonic right pan
            y: t * 1.5 + jitter_y,        // Monotonic down tilt
            theta: t * 0.005 + jitter_th, // Monotonic roll
            scale: 1.0,
        });
    }

    let raw = CumulativeTrajectory { points: pts };
    let smoothed = smoother.smooth_trajectory(&raw);

    assert_eq!(smoothed.points.len(), n);

    // Monotonic advance must be preserved across intentional motion
    for i in 1..n {
        assert!(
            smoothed.points[i].x >= smoothed.points[i - 1].x,
            "Smoothed x must monotonically increase with camera pan"
        );
        assert!(
            smoothed.points[i].y >= smoothed.points[i - 1].y,
            "Smoothed y must monotonically increase with camera tilt"
        );
    }
}

#[test]
fn test_dynamic_margin_auto_crop_and_border_modes() {
    let w = 64;
    let h = 64;

    let margin_mgr = DynamicMarginManager::new(0.20, BorderFillMode::AutoCrop);

    let raw_pts = vec![
        rd_ml::stabilize::TrajectoryPoint {
            timestamp_us: 0,
            x: 0.0,
            y: 0.0,
            theta: 0.0,
            scale: 1.0,
        },
        rd_ml::stabilize::TrajectoryPoint {
            timestamp_us: 33_333,
            x: 4.0,
            y: -3.0,
            theta: 0.02,
            scale: 1.0,
        },
    ];

    let smooth_pts = vec![
        rd_ml::stabilize::TrajectoryPoint {
            timestamp_us: 0,
            x: 0.0,
            y: 0.0,
            theta: 0.0,
            scale: 1.0,
        },
        rd_ml::stabilize::TrajectoryPoint {
            timestamp_us: 33_333,
            x: 0.5,
            y: -0.5,
            theta: 0.002,
            scale: 1.0,
        },
    ];

    let raw = CumulativeTrajectory { points: raw_pts };
    let smooth = CumulativeTrajectory { points: smooth_pts };

    let (comps, zoom) = margin_mgr.compute_compensations(&raw, &smooth, w, h);
    assert_eq!(comps.len(), 2);
    assert!(
        (1.0..=1.20).contains(&zoom),
        "Zoom must be clamped within max_crop: {}",
        zoom
    );

    // Test apply_video_stabilizer with all 5 border modes
    let src = vec![120u8; w * h * 4];
    for mode in 0..=4 {
        let params = VideoStabilizerParams {
            comp_dx: comps[1].dx / w as f32,
            comp_dy: comps[1].dy / h as f32,
            comp_dtheta: comps[1].dtheta,
            comp_scale: comps[1].scale,
            zoom,
            border_mode: mode,
            blur_radius: 8.0,
        };

        let out = apply_video_stabilizer(&src, w, h, &params);
        assert_eq!(
            out.len(),
            w * h * 4,
            "Output size mismatch for mode {}",
            mode
        );
    }
}

#[test]
fn test_end_to_end_frame_stabilization_pipeline() {
    let w = 64;
    let h = 64;

    // Generate 10 consecutive frames with synthetic alternating camera shake
    let mut frames = Vec::new();
    let frame_buffers: Vec<Vec<u8>> = (0..10)
        .map(|i| {
            let jx = if i % 2 == 0 { 2.5 } else { -2.5 };
            let jy = if i % 3 == 0 { 1.5 } else { -1.5 };
            generate_synthetic_corner_frame(w, h, jx, jy)
        })
        .collect();

    for buf in &frame_buffers {
        frames.push(buf.as_slice());
    }

    let stabilizer = VideoStabilizer::new(5, 0.20);
    let (compensations, report) = stabilizer.stabilize_frames(&frames, w, h, 30.0);

    assert_eq!(compensations.len(), 10);
    assert_eq!(report.frame_count, 10);
    assert!(
        report.jitter_reduction_db >= 10.0,
        "Jitter reduction was {:.2} dB",
        report.jitter_reduction_db
    );
    assert!(report.optimal_zoom >= 1.0);
    assert!(
        report.avg_tracked_features >= 3.0,
        "Tracked features: {}",
        report.avg_tracked_features
    );
}

#[test]
fn test_video_stabilizer_effect_registry_and_compositor_parity() {
    // 1. Registry verification
    let registry = EffectRegistry::new();
    let entry = registry
        .get("video_stabilizer")
        .expect("video_stabilizer must be registered");
    assert_eq!(entry.manifest.id, "video_stabilizer");
    assert_eq!(entry.manifest.parameters.len(), 7);

    // Verify parameter specifications
    let p_names: Vec<&str> = entry
        .manifest
        .parameters
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert!(p_names.contains(&"comp_dx"));
    assert!(p_names.contains(&"comp_dy"));
    assert!(p_names.contains(&"comp_dtheta"));
    assert!(p_names.contains(&"comp_scale"));
    assert!(p_names.contains(&"zoom"));
    assert!(p_names.contains(&"border_mode"));
    assert!(p_names.contains(&"blur_radius"));

    // 2. Multi-layer compositing verification
    let w = 16u32;
    let h = 16u32;
    let mut target = vec![0u8; (w * h * 4) as usize];

    let mut layer_pixels = vec![50u8; (w * h * 4) as usize];
    // Put white dot at center (8, 8)
    let dot_idx = ((8 * w + 8) * 4) as usize;
    layer_pixels[dot_idx..dot_idx + 4].copy_from_slice(&[255, 255, 255, 255]);

    let mut layer = CompositorLayer::new(layer_pixels, w, h);
    layer.video_stabilizer = Some(VideoStabilizerParams {
        comp_dx: 2.0 / w as f32, // Shift right by 2 pixels
        comp_dy: 0.0,
        comp_dtheta: 0.0,
        comp_scale: 1.0,
        zoom: 1.0,
        border_mode: 0,
        blur_radius: 8.0,
    });

    composite_layers_over(&mut target, w, h, &[layer]);

    let shifted_idx = ((8 * w + 10) * 4) as usize;
    assert!(
        target[shifted_idx] > 200,
        "Pixel at (10, 8) must be white from stabilization shift, got {}",
        target[shifted_idx]
    );
}
