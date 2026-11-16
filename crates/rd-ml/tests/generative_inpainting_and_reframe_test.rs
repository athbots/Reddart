//! Comprehensive Verification Suite for AI Generative Video Inpainting & Smart Saliency Reframe
//!
//! Validates:
//! 1. Inpainting mask morphological operations, boundary dilation, feathering, and bounding boxes.
//! 2. Spatial exemplar patch synthesis for object, watermark, and wire removal.
//! 3. Temporal optical flow-guided video inpainting across multi-frame sequences.
//! 4. Content-aware energy seam carving for object removal and aspect ratio retargeting.
//! 5. Smart saliency multi-subject detection, dynamic framing, and speaker switch detection.
//! 6. Integration with rd-effects and rd-render compositor layer.

use rd_effects::{InpaintParams, apply_inpaint_effect};
use rd_ml::inpainting::{
    InpaintConfig, InpaintMask, InpaintMode, SpatialInpainter, TemporalVideoInpainter,
};
use rd_ml::reframe::{AutoReframer, TargetAspect};
use rd_ml::saliency::{SaliencyConfig, SaliencyDetector};
use rd_ml::seam_carving::SeamCarver;
use rd_render::CompositorLayer;

#[test]
fn test_inpaint_mask_creation_dilation_and_feathering() {
    let width = 64;
    let height = 64;

    // 1. Create rectangular mask
    let mut mask = InpaintMask::new(width, height);
    mask.fill_rect(20, 20, 10, 10, 255);

    assert_eq!(mask.count_masked(), 100);
    assert!(mask.is_masked(20, 20));
    assert!(mask.is_masked(29, 29));
    assert!(!mask.is_masked(19, 20));
    assert!(!mask.is_masked(30, 20));

    // 2. Bounding box computation
    let bbox = mask.bounding_box().expect("Bounding box must be present");
    assert!((bbox.x - 20.0 / 64.0).abs() < 1e-3);
    assert!((bbox.y - 20.0 / 64.0).abs() < 1e-3);
    assert!((bbox.width - 10.0 / 64.0).abs() < 1e-3);
    assert!((bbox.height - 10.0 / 64.0).abs() < 1e-3);

    // 3. Binary dilation: radius 2 expands bounds
    let dilated = mask.dilate(2);
    assert!(dilated.count_masked() > mask.count_masked());
    assert!(dilated.is_masked(18, 20));
    assert!(dilated.is_masked(31, 20));

    // 4. Feathering produces smooth alpha gradient
    let feathered = mask.feather(3);
    assert_eq!(feathered.len(), width * height);
    let center_val = feathered[25 * width + 25];
    assert!(center_val > 0.8, "Core hole pixel must have high alpha");
    let edge_val = feathered[20 * width + 20];
    assert!(
        edge_val < center_val,
        "Perimeter pixel must have feathered intermediate alpha"
    );

    // 5. Mask inversion
    let inverted = mask.invert();
    assert_eq!(inverted.count_masked(), width * height - 100);
    assert!(!inverted.is_masked(25, 25));
    assert!(inverted.is_masked(0, 0));
}

#[test]
fn test_spatial_exemplar_inpaint_object_removal() {
    let width = 64;
    let height = 64;
    let channels = 4;

    // Create synthetic patterned background (gradient + texture)
    let mut frame = vec![0u8; width * height * channels];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * channels;
            frame[idx] = ((x * 4) % 256) as u8; // R
            frame[idx + 1] = ((y * 4) % 256) as u8; // G
            frame[idx + 2] = 180; // B
            frame[idx + 3] = 255; // A
        }
    }

    // Insert an occluding black rectangle (simulating a removed object or boom mic)
    let mask = InpaintMask::from_rect(width, height, 24, 24, 12, 12);
    for y in 24..36 {
        for x in 24..36 {
            let idx = (y * width + x) * channels;
            frame[idx] = 0;
            frame[idx + 1] = 0;
            frame[idx + 2] = 0;
            frame[idx + 3] = 255;
        }
    }

    // Before inpainting: hole is pitch black
    let hole_center = (30 * width + 30) * channels;
    assert_eq!(frame[hole_center], 0);
    assert_eq!(frame[hole_center + 1], 0);
    assert_eq!(frame[hole_center + 2], 0);

    // Run spatial inpainting
    let config = InpaintConfig {
        mode: InpaintMode::ObjectRemoval,
        patch_radius: 3,
        search_radius: 20,
        feather_radius: 2,
        ..Default::default()
    };
    let inpainter = SpatialInpainter::new(config);
    inpainter.inpaint_frame(&mut frame, width, height, channels, &mask);

    // After inpainting: hole pixels must be reconstructed with genuine non-black texture
    let r_filled = frame[hole_center] as f32;
    let g_filled = frame[hole_center + 1] as f32;
    let b_filled = frame[hole_center + 2] as f32;

    assert!(
        r_filled > 10.0,
        "Red channel in hole should be filled from context, got {}",
        r_filled
    );
    assert!(
        g_filled > 10.0,
        "Green channel in hole should be filled from context, got {}",
        g_filled
    );
    assert!(
        b_filled > 50.0,
        "Blue channel in hole should be reconstructed from surrounding 180 blue, got {}",
        b_filled
    );
}

#[test]
fn test_directional_wire_removal() {
    let width = 48;
    let height = 48;
    let channels = 4;

    // Uniform sky/wall background with high luminance
    let mut frame = vec![220u8; width * height * channels];

    // Place a dark 1-pixel wire diagonally across the frame
    let mut mask = InpaintMask::new(width, height);
    for i in 10..38 {
        mask.set(i, i, 255);
        let idx = (i * width + i) * channels;
        frame[idx] = 20; // Dark wire
        frame[idx + 1] = 20;
        frame[idx + 2] = 20;
        frame[idx + 3] = 255;
    }

    let config = InpaintConfig {
        mode: InpaintMode::WireRemoval,
        feather_radius: 1,
        ..Default::default()
    };
    let inpainter = SpatialInpainter::new(config);
    inpainter.inpaint_frame(&mut frame, width, height, channels, &mask);

    // Verify the wire has been smoothly interpolated
    let wire_pixel = (20 * width + 20) * channels;
    let restored_lum = frame[wire_pixel];
    assert!(
        restored_lum > 180,
        "Wire pixel should be restored close to surrounding 220, got {}",
        restored_lum
    );
}

#[test]
fn test_temporal_optical_flow_video_completion() {
    let width = 64;
    let height = 64;
    let channels = 4;

    // Synthesize a 3-frame sequence where the background translates horizontally:
    // Frame 0: shifted left by 4px
    // Frame 1: center
    // Frame 2: shifted right by 4px
    let mut frames = Vec::new();
    for shift in [-4, 0, 4] {
        let mut f = vec![0u8; width * height * channels];
        for y in 0..height {
            for x in 0..width {
                let shifted_x = (x as i32 + shift).clamp(0, width as i32 - 1) as usize;
                let idx = (y * width + x) * channels;
                f[idx] = ((shifted_x * 8) % 256) as u8;
                f[idx + 1] = (((y + 10) * 8) % 256) as u8;
                f[idx + 2] = 160;
                f[idx + 3] = 255;
            }
        }
        frames.push(f);
    }

    // Mask an object in Frame 1 (center frame) at x: 28..36, y: 28..36
    let mask_hole = InpaintMask::from_rect(width, height, 28, 28, 8, 8);
    let mask_clean = InpaintMask::new(width, height);

    let masks = vec![mask_clean.clone(), mask_hole.clone(), mask_clean];

    // Corrupt Frame 1 inside the hole
    for y in 28..36 {
        for x in 28..36 {
            let idx = (y * width + x) * channels;
            frames[1][idx] = 0;
            frames[1][idx + 1] = 0;
            frames[1][idx + 2] = 0;
            frames[1][idx + 3] = 255;
        }
    }

    let config = InpaintConfig {
        mode: InpaintMode::ObjectRemoval,
        enable_temporal_flow: true,
        temporal_window: 1,
        temporal_coherence: true,
        ..Default::default()
    };
    let temporal_inpainter = TemporalVideoInpainter::new(config);

    let completed_frame = temporal_inpainter.inpaint_sequence_frame(
        &frames, &masks, 1, // target center frame
        width, height, channels,
    );

    // Verify hole pixels were recovered from adjacent clean frames
    let p_idx = (32 * width + 32) * channels;
    assert!(
        completed_frame[p_idx] > 20,
        "Red channel in hole recovered via optical flow"
    );
    assert!(
        completed_frame[p_idx + 1] > 20,
        "Green channel in hole recovered via optical flow"
    );
    assert!(
        completed_frame[p_idx + 2] > 100,
        "Blue channel in hole recovered via optical flow"
    );
}

#[test]
fn test_seam_carving_energy_aware_object_removal_and_retargeting() {
    let width = 32;
    let height = 32;
    let channels = 4;

    let mut img = vec![128u8; width * height * channels];

    // Add a dark object strip at x=10..14
    let mut object_mask = InpaintMask::new(width, height);
    object_mask.fill_rect(10, 0, 4, height, 255);
    for y in 0..height {
        for x in 10..14 {
            let idx = (y * width + x) * channels;
            img[idx] = 20;
            img[idx + 1] = 20;
            img[idx + 2] = 20;
        }
    }

    // 1. Energy map must reflect -100,000 in masked object region
    let energy =
        SeamCarver::compute_energy_map(&img, width, height, channels, Some(&object_mask), None);
    assert_eq!(energy[11], -100_000.0);

    // 2. Carve object: iteratively removes seams passing through object
    let (carved_img, new_w, new_h) =
        SeamCarver::carve_object(&img, width, height, channels, &object_mask);

    assert_eq!(new_h, height);
    assert!(
        new_w < width,
        "Width must be reduced after carving object seams"
    );
    assert_eq!(
        carved_img.len(),
        new_w * new_h * channels,
        "Carved buffer length must match new dimensions"
    );

    // 3. Retargeting width with protected subject
    let mut protect_mask = InpaintMask::new(width, height);
    protect_mask.fill_rect(20, 0, 4, height, 255);
    let retargeted =
        SeamCarver::retarget_width(&img, width, height, channels, 28, Some(&protect_mask));
    assert_eq!(retargeted.len(), 28 * height * channels);
}

#[test]
fn test_smart_saliency_multi_subject_detection_and_dynamic_framing() {
    let width = 200;
    let height = 100;
    let channels = 3;
    let mut frame = vec![30u8; width * height * channels]; // Dark background

    // Place Subject 1 (Host) on left: x=30..60, y=20..70 (Skin tone)
    for y in 20..70 {
        for x in 30..60 {
            let idx = (y * width + x) * channels;
            frame[idx] = 230;
            frame[idx + 1] = 160;
            frame[idx + 2] = 120;
        }
    }

    // Place Subject 2 (Guest) on right: x=140..170, y=20..70 (Skin tone)
    for y in 20..70 {
        for x in 140..170 {
            let idx = (y * width + x) * channels;
            frame[idx] = 230;
            frame[idx + 1] = 160;
            frame[idx + 2] = 120;
        }
    }

    let detector = SaliencyDetector::new(SaliencyConfig::default());

    // Detect multiple subjects
    let subjects = detector.detect_multiple_salient_subjects(&frame, width, height, channels, 2);
    assert!(
        subjects.len() >= 2,
        "Expected at least 2 distinct subjects detected, got {}",
        subjects.len()
    );

    let (c1_x, _) = subjects[0].center();
    let (c2_x, _) = subjects[1].center();
    assert!(
        (c1_x - c2_x).abs() > 0.35,
        "Two subjects must be spatially separated across the frame"
    );

    // Compute unified framing box for 9:16 vertical reframe
    let unified = detector.compute_multi_subject_framing_box(&subjects, 9.0 / 16.0);
    assert!(
        unified.width >= 0.50,
        "Framing box must span both subjects across the frame"
    );

    // Test speaker switch detection
    let reframer = AutoReframer::new(TargetAspect::Vertical9x16);
    let detections = vec![
        (0, subjects[0]),
        (1_000_000, subjects[0]),
        (2_000_000, subjects[1]), // Active speaker switch to guest on right
        (3_000_000, subjects[1]),
    ];

    let switches = reframer.detect_speaker_switches(&detections, 0.35);
    assert_eq!(
        switches.len(),
        1,
        "Should detect exactly 1 active speaker switch"
    );
    assert_eq!(switches[0], 2_000_000);
}

#[test]
fn test_inpaint_effect_and_compositor_layer_integration() {
    let w = 32u32;
    let h = 32u32;
    let mut frame = vec![180u8; (w * h * 4) as usize];

    // Corrupt center with black hole
    for y in 12..20 {
        for x in 12..20 {
            let idx = ((y * w + x) * 4) as usize;
            frame[idx] = 0;
            frame[idx + 1] = 0;
            frame[idx + 2] = 0;
            frame[idx + 3] = 255;
        }
    }

    // 1. Direct apply_inpaint_effect
    let params = InpaintParams {
        mode: 0,
        hole_rect: Some([12.0 / 32.0, 12.0 / 32.0, 8.0 / 32.0, 8.0 / 32.0]),
        feather: 0.05,
        search_radius: 16,
    };
    apply_inpaint_effect(&mut frame, w, h, &params);

    let center_idx = ((16 * w + 16) * 4) as usize;
    assert!(
        frame[center_idx] > 100,
        "Hole pixel must be filled by inpaint effect"
    );

    // 2. Compositor layer with inpaint effect
    let mut layer = CompositorLayer::new(frame.clone(), w, h);
    layer.inpaint = Some(params);
    assert!(layer.inpaint.is_some());

    let mut target = vec![0u8; (w * h * 4) as usize];
    rd_render::composite_layers_over(&mut target, w, h, &[layer]);

    assert!(
        target[center_idx] > 100,
        "Composited target buffer must contain inpainted pixels"
    );
}
