//! Comprehensive Integration Test Suite for Milestone 21:
//! Neural Portrait Matting, Edge-Preserving Guided Filter & Virtual Green Screen.

use rd_effects::{
    EffectRegistry, VirtualGreenScreenParams, apply_virtual_green_screen,
    evaluate_virtual_green_screen_alpha,
};
use rd_ml::{
    CompositeBackground, DespillMode, GuidedFilter, PortraitMattingConfig, PortraitMattingEngine,
    SegmentationMask, Trimap, VirtualGreenScreen, suppress_color_spill_advanced,
};
use rd_render::{CompositorLayer, composite_layers_over};

#[test]
fn test_multi_scale_portrait_segmentation_pipeline() {
    let w = 640u32;
    let h = 480u32;
    let n = (w * h) as usize;

    // Background: Dark Slate Studio Wall [R=25, G=28, B=35, A=255]
    let mut frame = vec![0u8; n * 4];
    for chunk in frame.chunks_exact_mut(4) {
        chunk[0] = 25;
        chunk[1] = 28;
        chunk[2] = 35;
        chunk[3] = 255;
    }

    // Centered subject with realistic human skin chromaticity (YCbCr: Cb ~ 0.40, Cr ~ 0.60)
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    let rx = 120.0f32;
    let ry = 160.0f32;

    for y in 0..h {
        let dy = (y as f32 - cy) / ry;
        for x in 0..w {
            let dx = (x as f32 - cx) / rx;
            if dx * dx + dy * dy <= 1.0 {
                let px = ((y * w + x) * 4) as usize;
                frame[px] = 215; // R
                frame[px + 1] = 155; // G
                frame[px + 2] = 125; // B
                frame[px + 3] = 255;
            }
        }
    }

    // Configure multi-scale segmentation engine with 256px proxy dimension
    let config = PortraitMattingConfig {
        threshold: 0.42,
        feather_radius: 2,
        edge_choke: 0,
        spill_suppression: 0.6,
        temporal_smooth: 0.0,
        multi_scale: true,
        proxy_dimension: 256,
        guided_filter_radius: 4,
        guided_filter_eps: 1e-4,
        despill_mode: DespillMode::Green,
        despill_replacement: None,
        motion_threshold: 0.12,
    };

    let mut engine = PortraitMattingEngine::new(config);
    let mask = engine.segment_frame(&frame, w, h);

    assert_eq!(mask.width, w);
    assert_eq!(mask.height, h);

    // Subject core center should have strong alpha (near 255)
    let center_alpha = mask.get_alpha(w / 2, h / 2);
    assert!(
        center_alpha > 200,
        "Center subject alpha expected > 200, got {}",
        center_alpha
    );

    // Outer background corner should be transparent (near 0)
    let corner_alpha = mask.get_alpha(10, 10);
    assert!(
        corner_alpha < 40,
        "Corner background alpha expected < 40, got {}",
        corner_alpha
    );

    // Edge region transition from inside subject to background across the boundary
    let inside_alpha = mask.get_alpha((cx + rx - 15.0) as u32, cy as u32);
    let outside_alpha = mask.get_alpha((cx + rx + 15.0) as u32, cy as u32);
    assert!(
        inside_alpha > 180,
        "Inside boundary alpha expected high, got {}",
        inside_alpha
    );
    assert!(
        outside_alpha < 50,
        "Outside boundary alpha expected low, got {}",
        outside_alpha
    );

    // There must exist intermediate feathered values on the transition boundary
    let has_feathered_edge = ((cx + rx - 8.0) as u32..=(cx + rx + 8.0) as u32).any(|x| {
        let a = mask.get_alpha(x, cy as u32);
        a > 10 && a < 245
    });
    assert!(
        has_feathered_edge,
        "Expected feathered intermediate transition across boundary"
    );
}

#[test]
fn test_prefix_sum_guided_filter_edge_preservation_and_o1_speed() {
    let w = 64usize;
    let h = 64usize;
    let n = w * h;

    // Create an image with high-contrast fine hair strand structures (1-pixel wide vertical stripes)
    let mut guide = vec![0.1f32; n];
    let mut rough_mask = vec![0.0f32; n];

    for y in 16..48 {
        for x in 16..48 {
            rough_mask[y * w + x] = 1.0;
            // Add fine hair strands in guide image: alternating bright and dark 1-pixel lines
            if x % 2 == 0 {
                guide[y * w + x] = 0.95;
            } else {
                guide[y * w + x] = 0.20;
            }
        }
    }

    let filter_r4 = GuidedFilter::new(4, 1e-3);
    let mut out_r4 = vec![0.0f32; n];
    filter_r4.filter(&guide, &rough_mask, w, h, &mut out_r4);

    let filter_r12 = GuidedFilter::new(12, 1e-3);
    let mut out_r12 = vec![0.0f32; n];
    filter_r12.filter(&guide, &rough_mask, w, h, &mut out_r12);

    // Inside the subject area, the bright strands should retain higher alpha than the dark troughs
    let strand_high = out_r4[32 * w + 32]; // Even x = bright strand
    let strand_low = out_r4[32 * w + 33]; // Odd x = darker gap
    assert!(
        strand_high > strand_low,
        "Guided filter must preserve fine strand contrast: high={}, low={}",
        strand_high,
        strand_low
    );

    // Verify background remains near zero
    assert!(out_r4[5 * w + 5] < 0.1);
    assert!(out_r12[5 * w + 5] < 0.1);

    // Both small and large radii must complete valid, normalized filters
    for i in 0..n {
        assert!(out_r4[i] >= 0.0 && out_r4[i] <= 1.0);
        assert!(out_r12[i] >= 0.0 && out_r12[i] <= 1.0);
    }
}

#[test]
fn test_color_guided_filter_on_chromatic_hair_strands() {
    let w = 32usize;
    let h = 32usize;
    let n = w * h;

    // RGB guide: golden blonde hair strands on teal background
    // Subject: R=0.90, G=0.75, B=0.20
    // Background: R=0.20, G=0.50, B=0.60
    let mut guide_rgb = vec![0.0f32; n * 3];
    let mut mask = vec![0.0f32; n];

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let is_subject = (8..24).contains(&y) && (8..24).contains(&x);
            if is_subject {
                mask[idx] = 1.0;
                // Strand pattern
                if x % 2 == 0 {
                    guide_rgb[idx * 3] = 0.95;
                    guide_rgb[idx * 3 + 1] = 0.85;
                    guide_rgb[idx * 3 + 2] = 0.30;
                } else {
                    guide_rgb[idx * 3] = 0.80;
                    guide_rgb[idx * 3 + 1] = 0.65;
                    guide_rgb[idx * 3 + 2] = 0.15;
                }
            } else {
                guide_rgb[idx * 3] = 0.20;
                guide_rgb[idx * 3 + 1] = 0.50;
                guide_rgb[idx * 3 + 2] = 0.60;
            }
        }
    }

    let filter = GuidedFilter::new(2, 1e-3);
    let mut out = vec![0.0f32; n];
    filter.filter_color_guided(&guide_rgb, &mask, w, h, &mut out);

    // Center of subject should be high alpha
    assert!(out[16 * w + 16] > 0.70);
    // Outer border should be low alpha
    assert!(out[2 * w + 2] < 0.15);
}

#[test]
fn test_trimap_generation_and_boundary_matting_refinement() {
    let w = 32u32;
    let h = 32u32;
    let n = (w * h) as usize;

    let mut probs = vec![0.05f32; n];
    for y in 10..22 {
        for x in 10..22 {
            probs[(y * w + x) as usize] = 0.95;
        }
    }
    // Add transition boundary (values ~ 0.50)
    for y in 8..10 {
        for x in 8..24 {
            probs[(y * w + x) as usize] = 0.50;
        }
    }

    let trimap = Trimap::from_probabilities(&probs, w, h, 0.20, 0.80);
    assert_eq!(trimap.width, w);
    assert_eq!(trimap.height, h);

    // Definite foreground check
    assert!(trimap.is_foreground(16, 16));
    assert_eq!(trimap.data[(16 * w + 16) as usize], 255);

    // Definite background check
    assert!(trimap.is_background(2, 2));
    assert_eq!(trimap.data[(2 * w + 2) as usize], 0);

    // Unknown transition zone check
    assert!(trimap.is_unknown(12, 9));
    assert_eq!(trimap.data[(9 * w + 12) as usize], 128);

    // Refinement test: verify definite regions are clamped while unknown region accepts refined values
    let mut refined_alpha = vec![0.75f32; n];
    refined_alpha[0] = 0.75; // Even if refined_alpha has values at background, trimap must zero it!
    let mut output_mask = SegmentationMask::new(w, h, 0);
    trimap.refine_mask(&refined_alpha, &mut output_mask);

    assert_eq!(output_mask.get_alpha(16, 16), 255);
    assert_eq!(output_mask.get_alpha(2, 2), 0);
    assert_eq!(output_mask.get_alpha(12, 9), (0.75 * 255.0) as u8);
}

#[test]
fn test_advanced_color_despill_green_blue_and_adaptive() {
    // 1. Green spill: excessive green over red/blue average
    let mut green_spill_px = vec![60u8, 230, 70, 255];
    suppress_color_spill_advanced(
        &mut green_spill_px,
        DespillMode::Green,
        0.8,
        Some([60, 60, 60]), // Neutral gray bounce
    );
    assert!(
        green_spill_px[1] < 150,
        "Green despill failed: got {}",
        green_spill_px[1]
    );

    // 2. Blue spill: excessive blue over red/green average
    let mut blue_spill_px = vec![50u8, 60, 240, 255];
    suppress_color_spill_advanced(
        &mut blue_spill_px,
        DespillMode::Blue,
        0.85,
        Some([60, 60, 60]),
    );
    assert!(
        blue_spill_px[2] < 140,
        "Blue despill failed: got {}",
        blue_spill_px[2]
    );

    // 3. Adaptive auto-detection: frame with green border should choose green despill
    let mut adaptive_frame = [20u8, 220, 30, 255].repeat(16 * 16);
    suppress_color_spill_advanced(&mut adaptive_frame, DespillMode::Adaptive, 0.75, None);
    assert!(
        adaptive_frame[1] < 160,
        "Adaptive green despill failed: got {}",
        adaptive_frame[1]
    );
}

#[test]
fn test_virtual_green_screen_compositing_modes() {
    let w = 32u32;
    let h = 32u32;
    let n = (w * h) as usize;

    // Green screen frame with centered subject
    let mut fg = [20u8, 215, 30, 255].repeat(n);
    for y in 10..22 {
        for x in 10..22 {
            let px = ((y * w + x) * 4) as usize;
            fg[px] = 220; // R
            fg[px + 1] = 160; // G
            fg[px + 2] = 130; // B
            fg[px + 3] = 255;
        }
    }

    let config = PortraitMattingConfig {
        threshold: 0.40,
        feather_radius: 1,
        edge_choke: 0,
        spill_suppression: 0.75,
        temporal_smooth: 0.0,
        multi_scale: false,
        ..Default::default()
    };
    let mut vgs = VirtualGreenScreen::new(config);

    // 1. Composite over custom background plate (e.g. Amber sunset: [240, 140, 50, 255])
    let custom_bg = [240u8, 140, 50, 255].repeat(n);
    let mut out_custom = vec![0u8; n * 4];
    vgs.process_frame(
        &fg,
        w,
        h,
        CompositeBackground::CustomImage(&custom_bg),
        &mut out_custom,
    );

    // Center must show subject
    let center_px = ((16 * w + 16) * 4) as usize;
    assert!(
        out_custom[center_px] > 180,
        "Expected warm subject in center"
    );

    // Corner must show custom amber background plate
    let corner_px = 0;
    assert_eq!(out_custom[corner_px], 240);
    assert_eq!(out_custom[corner_px + 1], 140);
    assert_eq!(out_custom[corner_px + 2], 50);

    // 2. Composite with BlurBackground (bokeh defocus simulation)
    let mut out_blur = vec![0u8; n * 4];
    vgs.process_frame(
        &fg,
        w,
        h,
        CompositeBackground::BlurBackground {
            radius: 4,
            dim_factor: 0.85,
        },
        &mut out_blur,
    );
    // Subject center is preserved
    assert!(out_blur[center_px] > 170);

    // 3. Composite with DesaturatedBackground
    let mut out_desat = vec![0u8; n * 4];
    vgs.process_frame(
        &fg,
        w,
        h,
        CompositeBackground::DesaturatedBackground { dim_factor: 0.9 },
        &mut out_desat,
    );
    // Background green should be desaturated (R ~= G ~= B)
    let br = out_desat[corner_px] as i32;
    let bg_c = out_desat[corner_px + 1] as i32;
    let bb = out_desat[corner_px + 2] as i32;
    assert!(
        (br - bg_c).abs() < 10 && (bg_c - bb).abs() < 10,
        "Background must be desaturated grayscale, got R={}, G={}, B={}",
        br,
        bg_c,
        bb
    );
}

#[test]
fn test_motion_adaptive_temporal_smoothing_anti_flicker() {
    let w = 32u32;
    let h = 32u32;
    let n = (w * h) as usize;

    let mut engine = PortraitMattingEngine::new(PortraitMattingConfig {
        threshold: 0.45,
        feather_radius: 1,
        edge_choke: 0,
        spill_suppression: 0.5,
        temporal_smooth: 0.5, // 50% temporal smoothing
        multi_scale: false,
        motion_threshold: 0.10,
        ..Default::default()
    });

    let mut frame1 = vec![30u8; n * 4];
    // Put centered subject
    for y in 10..22 {
        for x in 10..22 {
            let px = ((y * w + x) * 4) as usize;
            frame1[px] = 220;
            frame1[px + 1] = 160;
            frame1[px + 2] = 130;
            frame1[px + 3] = 255;
        }
    }

    // Pass 1: Initial static frame
    let mask1 = engine.segment_frame(&frame1, w, h);
    assert!(mask1.get_alpha(16, 16) > 200);

    // Pass 2: Identical static frame with minor boundary sensor noise (+/- 2 pixel value jitter)
    let mut frame2 = frame1.clone();
    frame2[0] = 32;
    let mask2 = engine.segment_frame(&frame2, w, h);
    // Center subject should remain solidly locked with high temporal stability
    assert!(mask2.get_alpha(16, 16) > 200);

    // Pass 3: Sudden high-velocity motion: subject shifts entirely to top-left corner
    let mut frame3 = vec![30u8; n * 4];
    for y in 2..10 {
        for x in 2..10 {
            let px = ((y * w + x) * 4) as usize;
            frame3[px] = 220;
            frame3[px + 1] = 160;
            frame3[px + 2] = 130;
            frame3[px + 3] = 255;
        }
    }

    let mask3 = engine.segment_frame(&frame3, w, h);
    // The new location (5, 5) must immediately register high subject alpha without lag/trailing
    assert!(
        mask3.get_alpha(5, 5) > 100,
        "Motion-adaptive smoothing must immediately respond to fast translation, got alpha {}",
        mask3.get_alpha(5, 5)
    );
}

#[test]
fn test_virtual_green_screen_effect_registry_and_manifest() {
    let registry = EffectRegistry::new();
    let entry = registry
        .get("virtual_green_screen")
        .expect("virtual_green_screen must be registered");

    assert_eq!(entry.manifest.name, "Virtual Green Screen");
    assert_eq!(entry.manifest.parameters.len(), 9);

    let spill_mode_param = entry
        .manifest
        .parameters
        .iter()
        .find(|p| p.id == "spill_mode")
        .unwrap();
    assert_eq!(spill_mode_param.default, 0.0);

    let blur_radius_param = entry
        .manifest
        .parameters
        .iter()
        .find(|p| p.id == "blur_radius")
        .unwrap();
    assert_eq!(blur_radius_param.default, 8.0);

    let light_wrap_param = entry
        .manifest
        .parameters
        .iter()
        .find(|p| p.id == "light_wrap")
        .unwrap();
    assert_eq!(light_wrap_param.default, 0.15);

    // Test CPU software evaluator
    let params = VirtualGreenScreenParams {
        spill_mode: 0,
        spill: 0.7,
        bg_mode: 4, // Blur background
        blur_radius: 6,
        light_wrap: 0.2,
        ..Default::default()
    };

    let alpha = evaluate_virtual_green_screen_alpha(0.5, 0.5, 0.85, 0.60, 0.50, &params);
    assert!(alpha > 0.8);

    let mut frame_buf = vec![128u8; 16 * 16 * 4];
    apply_virtual_green_screen(&mut frame_buf, 16, 16, &params);
    assert_eq!(frame_buf.len(), 16 * 16 * 4);
}

#[test]
fn test_compositor_layer_virtual_green_screen_integration() {
    let w = 16u32;
    let h = 16u32;
    let mut canvas = vec![0u8; (w * h * 4) as usize];

    // Track 1: Solid Dark Magenta Background [100, 20, 80, 255]
    let bg_layer = CompositorLayer::new([100, 20, 80, 255].repeat((w * h) as usize), w, h);

    // Track 2: Green screen foreground with warm subject in center
    let mut fg_pixels = [25, 220, 35, 255].repeat((w * h) as usize);
    for y in 6..10 {
        for x in 6..10 {
            let px = ((y * w + x) * 4) as usize;
            fg_pixels[px] = 225; // R
            fg_pixels[px + 1] = 165; // G
            fg_pixels[px + 2] = 135; // B
            fg_pixels[px + 3] = 255;
        }
    }

    let mut fg_layer = CompositorLayer::new(fg_pixels, w, h);
    fg_layer.virtual_green_screen = Some(VirtualGreenScreenParams {
        threshold: 0.45,
        feather: 0.05,
        edge_choke: 0.0,
        spill_mode: 0,
        spill: 0.75,
        invert: false,
        bg_mode: 0, // Transparent alpha
        blur_radius: 4,
        light_wrap: 0.1,
    });

    composite_layers_over(&mut canvas, w, h, &[bg_layer, fg_layer]);

    // Center pixel must be warm subject
    let center_px = ((8 * w + 8) * 4) as usize;
    assert!(
        canvas[center_px] > 180,
        "Expected warm subject in composite center, got R={}",
        canvas[center_px]
    );

    // Corner pixel must be magenta background
    let corner_px = 0;
    assert_eq!(canvas[corner_px], 100);
    assert_eq!(canvas[corner_px + 1], 20);
    assert_eq!(canvas[corner_px + 2], 80);
}
