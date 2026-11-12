//! Integration tests for Initiative 9: Cinema-Grade HDR10 / Rec.2020 & ACEScc Color Pipeline.

use rd_effects::{
    AscCdlParams, ColorGamut, ColorPipeline, ColorPipelineConfig, ColorTransformParams,
    EffectRegistry, TransferFunction, aces_filmic_tone_map, apply_color_transform,
    decode_transfer_function, encode_transfer_function, gamut_transform_matrix,
};

#[test]
fn test_color_transform_effect_registry_manifest() {
    let registry = EffectRegistry::new();
    let entry = registry
        .get("color_transform")
        .expect("color_transform must be registered");

    assert_eq!(entry.manifest.name, "Cinema Color Space & ACES Transform");
    assert_eq!(entry.manifest.cost_class, rd_effects::CostClass::Medium);
    assert_eq!(entry.manifest.parameters.len(), 6);

    let param_ids: Vec<&str> = entry
        .manifest
        .parameters
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(
        param_ids,
        vec![
            "source_gamut",
            "source_tf",
            "target_gamut",
            "target_tf",
            "exposure",
            "tone_mapping"
        ]
    );
}

#[test]
fn test_gamut_conversion_matrices() {
    // Rec.709 to Rec.2020 matrix
    let mat_709_to_2020 = gamut_transform_matrix(ColorGamut::Rec709, ColorGamut::Rec2020);
    // Rec.2020 to Rec.709 matrix
    let mat_2020_to_709 = gamut_transform_matrix(ColorGamut::Rec2020, ColorGamut::Rec709);

    let composite = mat_709_to_2020.mul_mat3(&mat_2020_to_709);

    // Verify composite product is approximately identity
    for (i, &v) in composite.m.iter().enumerate() {
        let row = i / 3;
        let col = i % 3;
        let expected = if row == col { 1.0 } else { 0.0 };
        assert!(
            (v - expected).abs() < 1e-3,
            "Deviates from identity at ({}, {}): {}",
            row,
            col,
            v
        );
    }
}

#[test]
fn test_hdr10_pq_transfer_function() {
    // In normalized cinema coordinates, 1.0 = 100 nits (diffuse white).
    // Test diffuse white encode / decode roundtrip
    let linear_100_nits = 1.0f32;
    let pq_encoded = encode_transfer_function(linear_100_nits, TransferFunction::PQ);
    let decoded = decode_transfer_function(pq_encoded, TransferFunction::PQ);

    assert!(
        (decoded - linear_100_nits).abs() < 1e-3,
        "PQ 100 nits roundtrip error: decoded={}",
        decoded
    );

    // Peak highlight: 1000 nits = 10.0 in linear scale
    let linear_1000_nits = 10.0f32;
    let pq_encoded_1000 = encode_transfer_function(linear_1000_nits, TransferFunction::PQ);
    assert!(
        pq_encoded_1000 > pq_encoded,
        "1000 nits must encode to a higher PQ code value than 100 nits"
    );
    let decoded_1000 = decode_transfer_function(pq_encoded_1000, TransferFunction::PQ);
    assert!(
        (decoded_1000 - linear_1000_nits).abs() < 5e-2,
        "PQ 1000 nits roundtrip error: decoded={}",
        decoded_1000
    );
}

#[test]
fn test_aces_filmic_tone_mapping_compression() {
    // Tone mapping should preserve near-zero linearly and smoothly roll off super-whites
    let low = aces_filmic_tone_map(0.1);
    let mid = aces_filmic_tone_map(0.5);
    let white = aces_filmic_tone_map(1.0);
    let bright = aces_filmic_tone_map(5.0);
    let extreme = aces_filmic_tone_map(50.0);

    assert!(low < mid);
    assert!(mid < white);
    assert!(white < bright);
    assert!(bright < extreme);
    assert!(extreme <= 1.0, "Extreme highlights must roll off <= 1.0");
    assert!(
        extreme > 0.95,
        "Extreme highlights should asymptote close to 1.0"
    );
}

#[test]
fn test_acescc_cdl_grading_pipeline() {
    let config = ColorPipelineConfig {
        apply_acescc_cdl: true,
        cdl: AscCdlParams {
            slope: [1.2, 1.0, 0.8], // warm gain
            offset: [0.02, 0.0, -0.02],
            power: [1.0, 1.0, 1.0],
            saturation: 1.1,
        },
        ..Default::default()
    };

    let pipeline = ColorPipeline::new(config);
    let in_pixel = [0.5, 0.5, 0.5, 1.0];
    let out_pixel = pipeline.process_pixel(in_pixel);

    // Red gain was increased and blue gain was decreased
    assert!(
        out_pixel[0] > out_pixel[1],
        "Red channel should be higher than green with warm slope"
    );
    assert!(
        out_pixel[1] > out_pixel[2],
        "Green channel should be higher than blue with warm slope"
    );
}

#[test]
fn test_apply_color_transform_buffer_execution() {
    let width = 4;
    let height = 4;
    let mut input = vec![128u8; width * height * 4];
    for chunk in input.chunks_exact_mut(4) {
        chunk[3] = 255;
    }

    let mut output = vec![0u8; input.len()];
    let params = ColorTransformParams {
        source_gamut: ColorGamut::Rec709,
        source_tf: TransferFunction::SRGB,
        target_gamut: ColorGamut::DisplayP3,
        target_tf: TransferFunction::SRGB,
        exposure: 0.5,
        tone_mapping: true,
    };

    apply_color_transform(&input, width, height, &params, &mut output);

    assert_eq!(output[3], 255);
    // +0.5 EV exposure boost should increase brightness over input
    assert!(
        output[0] > input[0],
        "Output pixel {} should be brighter than input {}",
        output[0],
        input[0]
    );
}
