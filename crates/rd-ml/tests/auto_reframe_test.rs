//! End-to-end integration test for AI Auto-Reframe, Visual Saliency Tracking,
//! and Multi-Aspect Ratio Compositing (`rd-ml` / `rd-effects` / `rd-render`).

use rd_core::TimestampUs;
use rd_effects::{AutoReframeParams, EffectRegistry, apply_auto_reframe};
use rd_ml::{
    AutoReframer, BoundingBox, ReframeConfig, SaliencyConfig, SaliencyDetector, TargetAspect,
};
use rd_render::{CompositorLayer, composite_layers_over};

/// Helper to generate a 16:9 synthetic test frame (width x height) with a skin-toned subject circle.
fn generate_subject_frame(width: usize, height: usize, center_x: f32, center_y: f32) -> Vec<u8> {
    let mut buf = vec![0u8; width * height * 4];
    let cx = center_x * width as f32;
    let cy = center_y * height as f32;
    let radius = (width as f32 * 0.12).max(8.0);

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Realistic portrait skin locus tone (RGB ~ [210, 160, 135])
                buf[idx] = 210;
                buf[idx + 1] = 160;
                buf[idx + 2] = 135;
                buf[idx + 3] = 255;
            } else {
                // Neutral cool background with subtle gradient
                let bg = ((x + y) % 32) as u8 + 40;
                buf[idx] = bg;
                buf[idx + 1] = bg + 5;
                buf[idx + 2] = bg + 15;
                buf[idx + 3] = 255;
            }
        }
    }
    buf
}

#[test]
fn test_end_to_end_saliency_and_auto_reframe_path() {
    let width = 320;
    let height = 180; // 16:9 aspect ratio

    let detector = SaliencyDetector::new(SaliencyConfig::default());
    let reframer = AutoReframer::with_config(ReframeConfig {
        target_aspect: TargetAspect::Vertical9x16,
        smoothing_factor: 0.15,
        dead_zone_width: 0.08,
        dead_zone_height: 0.08,
        max_pan_speed: 1.5,
        scene_cut_threshold: 0.38,
        headroom_bias: 0.10,
    });

    // 1. Simulate 10 frames of a person walking smoothly from left (x = 0.30) to right (x = 0.70)
    let n_frames = 10;
    let mut detections: Vec<(TimestampUs, BoundingBox)> = Vec::new();

    for i in 0..n_frames {
        let t = i as f32 / (n_frames - 1) as f32;
        let subject_x = 0.30 + t * 0.40;
        let subject_y = 0.50;

        let frame = generate_subject_frame(width, height, subject_x, subject_y);
        let bbox = detector.detect_salient_subject(&frame, width, height, 4);

        // Assert saliency detector successfully centered on the moving subject
        let center = bbox.center();
        assert!(
            (center.0 - subject_x).abs() < 0.15,
            "Frame {i}: expected subject x around {subject_x}, got {}",
            center.0
        );
        let timestamp_us = (i as i64) * 33_333; // ~30 fps
        detections.push((timestamp_us, bbox));
    }

    // 2. Solve kinematic reframe camera path for Vertical 9:16 (TikTok / Reels / Shorts)
    let reframe_tracks =
        reframer.solve_kinematic_reframe_path(width as u32, height as u32, &detections);
    assert_eq!(reframe_tracks.pan_x.keyframes().len(), n_frames);

    // Verify pan path smoothly transitions leftwards to keep rightward subject centered
    let first_pan = reframe_tracks.pan_x.keyframes().first().unwrap().value;
    let last_pan = reframe_tracks.pan_x.keyframes().last().unwrap().value;
    assert!(
        first_pan > last_pan,
        "Pan offset must shift leftwards to compensate for rightward movement: first={}, last={}",
        first_pan,
        last_pan
    );

    // 3. Verify pan values stay within safe frame pixel bounds
    for kf in reframe_tracks.pan_x.keyframes() {
        assert!(kf.value.abs() <= width as f32 * 0.5);
    }
    for kf in reframe_tracks.pan_y.keyframes() {
        assert!(kf.value.abs() <= height as f32 * 0.5);
    }
    for kf in reframe_tracks.zoom.keyframes() {
        assert!(kf.value >= 0.8 && kf.value <= 2.5);
    }
}

#[test]
fn test_scene_cut_immediate_reframing() {
    let reframer = AutoReframer::with_config(ReframeConfig {
        target_aspect: TargetAspect::Vertical9x16,
        smoothing_factor: 0.10,
        scene_cut_threshold: 0.38,
        ..Default::default()
    });

    // Frame 0: Subject at left (center = 0.20)
    let box_a = BoundingBox {
        x: 0.10,
        y: 0.30,
        width: 0.20,
        height: 0.40,
    };
    // Frame 1: Abrupt cut, subject at right (center = 0.80) -> jump = 0.60 > 0.38
    let box_b = BoundingBox {
        x: 0.70,
        y: 0.30,
        width: 0.20,
        height: 0.40,
    };

    let detections = vec![(0, box_a), (33_333, box_b)];
    let tracks = reframer.solve_kinematic_reframe_path(320, 180, &detections);
    assert_eq!(tracks.pan_x.keyframes().len(), 2);

    // Frame 1 must instantly jump rather than smoothly lagging
    let cut_jump = (tracks.pan_x.keyframes()[1].value - tracks.pan_x.keyframes()[0].value).abs();
    assert!(
        cut_jump > 0.35,
        "Expected instantaneous scene cut jump > 0.35, got {}",
        cut_jump
    );
}

#[test]
fn test_compositor_layer_multi_aspect_rendering() {
    let width = 320;
    let height = 180;
    let src_frame = generate_subject_frame(width, height, 0.75, 0.50);

    // Case A: 9:16 Vertical Crop-to-Fill
    let v_params = AutoReframeParams {
        target_aspect: 0, // 9:16
        pan_x: 0.25,      // Pan toward right subject
        pan_y: 0.0,
        zoom: 1.0,
        fill_mode: 0, // Crop fill
        background_blur: 8.0,
        background_dim: 0.45,
    };
    let v_buf = apply_auto_reframe(&src_frame, width, height, 180, 320, &v_params);

    // Verify non-zero output and center pixel has skin tone from the right subject
    assert!(v_buf.iter().any(|&b| b > 0));
    let center_idx = ((320 / 2) * 180 + (180 / 2)) * 4;
    // With pan_x = 0.25, the subject at 0.75 in source is mapped to center in target
    assert!(
        v_buf[center_idx] > 150,
        "Center red channel should be skin tone > 150, got {}",
        v_buf[center_idx]
    );

    // Case B: 1:1 Square Pillarbox with Blurred Dimmed Background
    let sq_params = AutoReframeParams {
        target_aspect: 1, // 1:1 Square
        pan_x: 0.0,
        pan_y: 0.0,
        zoom: 1.0,
        fill_mode: 1, // Blurred background pillarbox
        background_blur: 10.0,
        background_dim: 0.50,
    };
    let sq_buf = apply_auto_reframe(&src_frame, width, height, 250, 250, &sq_params);

    // Verify pillarbox border pixels are dimmed and blurred
    let corner_idx = (5 * 250 + 5) * 4;
    assert_eq!(
        sq_buf[corner_idx + 3],
        255,
        "Pillarbox background alpha must be opaque"
    );

    // Case C: Multi-Layer Compositing Engine Integration
    let layer = CompositorLayer {
        rgba: src_frame.clone(),
        width: width as u32,
        height: height as u32,
        opacity: 1.0,
        pos_x: 0.0,
        pos_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        blend_mode: 0,
        shape_mask: None,
        neural_matte: None,
        auto_reframe: Some(v_params),
    };

    let mut target_canvas = vec![0u8; width * height * 4];
    composite_layers_over(&mut target_canvas, width as u32, height as u32, &[layer]);

    // Canvas should be completely populated
    let non_zero_count = target_canvas.iter().filter(|&&b| b > 0).count();
    assert!(
        non_zero_count > (width * height * 2),
        "Expected populated canvas from auto_reframe layer compositing"
    );
}

#[test]
fn test_effect_registry_auto_reframe() {
    let registry = EffectRegistry::new();
    let reframe_meta = registry
        .get("auto_reframe")
        .expect("auto_reframe effect must be registered");

    assert_eq!(reframe_meta.manifest.id, "auto_reframe");
    assert_eq!(reframe_meta.manifest.name, "AI Auto Reframe");
    assert_eq!(reframe_meta.manifest.parameters.len(), 7);

    // Verify all 7 parameters exist in manifest
    let param_ids: Vec<&str> = reframe_meta
        .manifest
        .parameters
        .iter()
        .map(|p| p.id.as_str())
        .collect();

    assert!(param_ids.contains(&"target_aspect"));
    assert!(param_ids.contains(&"pan_x"));
    assert!(param_ids.contains(&"pan_y"));
    assert!(param_ids.contains(&"zoom"));
    assert!(param_ids.contains(&"fill_mode"));
    assert!(param_ids.contains(&"background_blur"));
    assert!(param_ids.contains(&"background_dim"));
}
