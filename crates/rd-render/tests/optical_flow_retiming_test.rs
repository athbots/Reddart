//! End-to-end integration test for Cinema-Grade Optical Flow Retiming & Slow Motion
//! (`rd-render` / `rd-effects` / `rd-core`).

use rd_core::{Clip, RetimeMode};
use rd_effects::{CostClass, EffectRegistry};
use rd_render::{
    DenseOpticalFlowEstimator, OpticalFlowConfig, interpolate_optical_flow,
    interpolate_retimed_frame,
};

/// Helper to generate a 64x32 synthetic frame with a textured moving block.
fn generate_moving_scene(width: usize, height: usize, block_x: usize) -> Vec<u8> {
    let mut buf = vec![25u8; width * height * 4];

    // Subtle background gradient
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            buf[idx] = (x * 2) as u8 + 15;
            buf[idx + 1] = (y * 3) as u8 + 20;
            buf[idx + 2] = 40;
            buf[idx + 3] = 255;
        }
    }

    // Moving high-contrast block (width 10, height 16)
    for y in 8..24 {
        for x in block_x..(block_x + 10).min(width) {
            let idx = (y * width + x) * 4;
            buf[idx] = 235;
            buf[idx + 1] = 180;
            buf[idx + 2] = 70;
            buf[idx + 3] = 255;
        }
    }

    buf
}

#[test]
fn test_end_to_end_optical_flow_slow_motion() {
    let width = 64;
    let height = 32;

    // Frame A: Block starts at x = 12
    let frame_a = generate_moving_scene(width, height, 12);
    // Frame B: Block moved right by 8 pixels to x = 20
    let frame_b = generate_moving_scene(width, height, 20);

    // Test 1: Linear blend produces dual ghosting at midpoint (x=12 and x=20 both have half intensity)
    let mut blend_out = vec![0u8; width * height * 4];
    interpolate_retimed_frame(
        &frame_a,
        &frame_b,
        width,
        height,
        0.5,
        RetimeMode::Blend,
        &mut blend_out,
    )
    .unwrap();

    let cy = height / 2;
    let blend_at_12 = blend_out[(cy * width + 12) * 4];
    let blend_at_20 = blend_out[(cy * width + 20) * 4];
    // In linear blend, both old and new positions ghost with ~120-130 intensity
    assert!(
        blend_at_12 > 100 && blend_at_20 > 100,
        "Linear blend exhibits ghosting at both ends"
    );

    // Test 2: Optical flow retiming warps block to intermediate position x = 16!
    let mut of_out = vec![0u8; width * height * 4];
    interpolate_retimed_frame(
        &frame_a,
        &frame_b,
        width,
        height,
        0.5,
        RetimeMode::OpticalFlow,
        &mut of_out,
    )
    .unwrap();

    let of_at_16 = of_out[(cy * width + 16) * 4];
    // In optical flow, the block center (x=16) is fully illuminated
    assert!(
        of_at_16 > 180,
        "Optical flow should center block at x=16 with high brightness > 180, got {}",
        of_at_16
    );

    // Test 3: Quarter-step retiming at t = 0.25 (block at x = 14)
    let mut of_quarter = vec![0u8; width * height * 4];
    interpolate_retimed_frame(
        &frame_a,
        &frame_b,
        width,
        height,
        0.25,
        RetimeMode::OpticalFlow,
        &mut of_quarter,
    )
    .unwrap();
    let of_at_14 = of_quarter[(cy * width + 14) * 4];
    assert!(
        of_at_14 > 170,
        "Quarter retiming t=0.25 should illuminate x=14, got {}",
        of_at_14
    );
}

#[test]
fn test_occlusion_and_boundary_handling() {
    let width = 64;
    let height = 32;

    // Moving block from x = 15 to x = 25
    let frame_a = generate_moving_scene(width, height, 15);
    let frame_b = generate_moving_scene(width, height, 25);

    let estimator = DenseOpticalFlowEstimator::new(OpticalFlowConfig {
        pyramid_levels: 3,
        iterations: 15,
        alpha_sq: 0.02,
        occlusion_threshold: 1.5,
    });

    let (fwd, bwd, occlusion) =
        estimator.compute_bidirectional_flow(&frame_a, &frame_b, width, height);

    assert_eq!(occlusion.len(), width * height);

    // Occlusion mask should detect disoccluded background trailing edge
    let mut synthesized = vec![0u8; width * height * 4];
    interpolate_optical_flow(
        &frame_a,
        &frame_b,
        &fwd,
        &bwd,
        &occlusion,
        width,
        height,
        0.5,
        &mut synthesized,
    )
    .unwrap();

    // The output buffer must have zero holes or invalid alpha
    for chunk in synthesized.chunks_exact(4) {
        assert_eq!(
            chunk[3], 255,
            "Alpha channel must remain fully opaque across all pixels"
        );
    }
}

#[test]
fn test_optical_flow_effect_registry() {
    let registry = EffectRegistry::new();
    let entry = registry
        .get("optical_flow")
        .expect("optical_flow effect must be registered in EffectRegistry");

    assert_eq!(entry.manifest.name, "Optical Flow Retiming");
    assert_eq!(entry.manifest.cost_class, CostClass::High);
    assert_eq!(entry.manifest.parameters.len(), 4);

    let ids: Vec<&str> = entry
        .manifest
        .parameters
        .iter()
        .map(|p| p.id.as_str())
        .collect();

    assert!(ids.contains(&"retime_speed"));
    assert!(ids.contains(&"blend_time"));
    assert!(ids.contains(&"flow_smoothness"));
    assert!(ids.contains(&"occlusion_suppression"));
    assert!(entry.wgsl_source.contains("fn fs_main"));
}

#[test]
fn test_retime_mode_clip_persistence() {
    let mut clip = Clip::new_video("sample.mp4", 0, 0, 5_000_000);
    assert_eq!(clip.retime_mode, RetimeMode::Blend);

    clip.retime_mode = RetimeMode::OpticalFlow;
    clip.speed = 0.25; // 4x slow motion

    let json = serde_json::to_string(&clip).expect("Serialize clip");
    assert!(json.contains("\"retime_mode\":\"OpticalFlow\""));

    let deserialized: Clip = serde_json::from_str(&json).expect("Deserialize clip");
    assert_eq!(deserialized.retime_mode, RetimeMode::OpticalFlow);
    assert_eq!(deserialized.speed, 0.25);
}
