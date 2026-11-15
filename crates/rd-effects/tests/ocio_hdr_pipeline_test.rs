//! Integration test suite for HDR / ACES Color Pipeline Expansion (Phase II).
//!
//! Validates:
//! - OpenColorIO (OCIO) v2 configuration loading, parsing, serialization, and role lookup.
//! - ACES 1.2 / 1.3 built-in profiles (ACES2065-1, ACEScg, ACEScc, ACEScct).
//! - ACEScct quasi-logarithmic encoding with toe curve vs ACEScc.
//! - SMPTE ST 2084 PQ (HDR10 up to 10,000 nits) and ARIB STD-B67 HLG transfer curves.
//! - 10-bit R10G10B10A2 packed integer format (`pack_rgb10a2`, `unpack_rgb10a2`).
//! - 12-bit cinema master format (`pack_rgb12`, `unpack_rgb12`).
//! - TPDF (Triangular Probability Density Function) dithering for HDR banding elimination.
//! - End-to-end color pipeline resolution and buffer processing.

use rd_effects::color_space::{
    AscCdlParams, ColorGamut, ColorPipeline, ColorPipelineConfig, TransferFunction,
    decode_transfer_function, encode_transfer_function, pack_rgb10a2, pack_rgb12,
    quantize_rgba32f_to_rgb10a2, quantize_rgba32f_to_rgb12, unpack_rgb10a2, unpack_rgb12,
};
use rd_effects::ocio::{OcioBitDepth, OcioConfig};

#[test]
fn test_ocio_config_default_aces_hierarchy() {
    let config = OcioConfig::aces_default();

    assert_eq!(config.ocio_profile_version, 2);
    assert_eq!(config.search_path.as_deref(), Some("luts"));

    // Verify critical ACES roles
    assert_eq!(config.get_role_target("scene_linear"), Some("ACEScg"));
    assert_eq!(config.get_role_target("color_timing"), Some("ACEScc"));
    assert_eq!(config.get_role_target("compositing_log"), Some("ACEScct"));
    assert_eq!(config.get_role_target("reference"), Some("ACES2065-1"));
    assert_eq!(config.get_role_target("data"), Some("Raw"));

    // Verify displays and views
    let srgb_views = config
        .get_display_views("sRGB")
        .expect("sRGB display missing");
    assert!(srgb_views.iter().any(|v| v.name == "Standard"));
    assert!(srgb_views.iter().any(|v| v.name == "ACES 1.0 - SDR Video"));

    let rec2020_views = config
        .get_display_views("Rec.2020")
        .expect("Rec.2020 display missing");
    assert!(rec2020_views.iter().any(|v| v.name == "HDR10 - 1000 nits"));
    assert!(rec2020_views.iter().any(|v| v.name == "HLG - Broadcast"));

    // Verify color space parameters
    let acescg = config.find_colorspace("ACEScg").expect("ACEScg missing");
    assert_eq!(acescg.gamut, Some(ColorGamut::ACEScg));
    assert_eq!(acescg.transfer_function, Some(TransferFunction::Linear));
    assert_eq!(acescg.bitdepth, OcioBitDepth::Bit32f);
    assert!(!acescg.isdata);

    let raw = config.find_colorspace("Raw").expect("Raw missing");
    assert!(raw.isdata);
}

#[test]
fn test_ocio_config_from_yaml_roundtrip_and_serialization() {
    let original_yaml = r#"
ocio_profile_version: 2
search_path: "cinema_luts"

roles:
  color_timing: ACEScc
  compositing_log: ACEScct
  reference: ACES2065-1
  scene_linear: ACEScg

displays:
  Display P3:
    - !<View> {name: Standard, colorspace: Display P3}
  Rec.2020:
    - !<View> {name: HDR10, colorspace: Rec.2020 - PQ}

colorspaces:
  - !<ColorSpace>
    name: ACES2065-1
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES AP0 Archival Reference"

  - !<ColorSpace>
    name: ACEScg
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES AP1 Linear CG Working Space"

  - !<ColorSpace>
    name: ACEScc
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES Log Grading"

  - !<ColorSpace>
    name: ACEScct
    family: ACES
    bitdepth: 32f
    isdata: false
    description: "ACES Log with Toe"

  - !<ColorSpace>
    name: Display P3
    family: Displays
    bitdepth: 10i
    isdata: false

  - !<ColorSpace>
    name: Rec.2020 - PQ
    family: Displays
    bitdepth: 10i
    isdata: false
"#;

    let parsed = OcioConfig::from_yaml_str(original_yaml).expect("YAML parse failed");
    assert_eq!(parsed.ocio_profile_version, 2);
    assert_eq!(parsed.search_path.as_deref(), Some("cinema_luts"));
    assert_eq!(parsed.roles.len(), 4);
    assert_eq!(parsed.displays.len(), 2);
    assert_eq!(parsed.colorspaces.len(), 6);

    // Serialize to YAML
    let serialized = parsed.to_yaml_str();
    assert!(serialized.contains("ocio_profile_version: 2"));
    assert!(serialized.contains("cinema_luts"));
    assert!(serialized.contains("scene_linear: ACEScg"));

    // Re-parse serialized output
    let reparsed = OcioConfig::from_yaml_str(&serialized).expect("Re-parsing failed");
    assert_eq!(reparsed.ocio_profile_version, parsed.ocio_profile_version);
    assert_eq!(reparsed.search_path, parsed.search_path);
    assert_eq!(reparsed.roles, parsed.roles);
    assert_eq!(reparsed.displays.len(), parsed.displays.len());
}

#[test]
fn test_ocio_pipeline_resolution_end_to_end() {
    let config = OcioConfig::aces_default();

    // 1. Resolve scene_linear (ACEScg) -> sRGB (SDR Display with filmic tone map)
    let pipe_sdr = config
        .resolve_pipeline("scene_linear", "sRGB", "Standard")
        .expect("SDR pipeline resolution failed");
    assert_eq!(pipe_sdr.input_gamut, ColorGamut::ACEScg);
    assert_eq!(pipe_sdr.input_tf, TransferFunction::Linear);
    assert_eq!(pipe_sdr.output_gamut, ColorGamut::Rec709);
    assert_eq!(pipe_sdr.output_tf, TransferFunction::SRGB);
    assert!(pipe_sdr.apply_tone_mapping);

    // 2. Resolve scene_linear (ACEScg) -> Rec.2020 HDR10 PQ (1000 nits)
    let pipe_hdr10 = config
        .resolve_pipeline("scene_linear", "Rec.2020", "HDR10 - 1000 nits")
        .expect("HDR10 pipeline resolution failed");
    assert_eq!(pipe_hdr10.input_gamut, ColorGamut::ACEScg);
    assert_eq!(pipe_hdr10.input_tf, TransferFunction::Linear);
    assert_eq!(pipe_hdr10.output_gamut, ColorGamut::Rec2020);
    assert_eq!(pipe_hdr10.output_tf, TransferFunction::PQ);
    assert!(!pipe_hdr10.apply_tone_mapping); // HDR preserves extended range

    // 3. Resolve direct transform chain: ACEScc -> Rec.709
    let pipe_chain = config
        .resolve_transform_chain("ACEScc", "Rec.709")
        .expect("Direct chain failed");
    assert_eq!(pipe_chain.input_gamut, ColorGamut::ACEScg);
    assert_eq!(pipe_chain.input_tf, TransferFunction::ACEScc);
    assert_eq!(pipe_chain.output_gamut, ColorGamut::Rec709);
    assert_eq!(pipe_chain.output_tf, TransferFunction::Rec709);
}

#[test]
fn test_acescct_toe_curve_vs_acescc() {
    // 1. Above breakpoint (0.0078125), ACEScct and ACEScc match closely
    for &lin in &[0.01, 0.05, 0.18, 1.0, 5.0, 20.0] {
        let log_cc = encode_transfer_function(lin, TransferFunction::ACEScc);
        let log_cct = encode_transfer_function(lin, TransferFunction::ACEScct);
        assert!(
            (log_cc - log_cct).abs() < 1e-4,
            "Above breakpoint, ACEScc and ACEScct must match at {}: {} vs {}",
            lin,
            log_cc,
            log_cct
        );
    }

    // 2. Below breakpoint, ACEScc drops rapidly towards -inf, while ACEScct exhibits a gentle linear toe
    let deep_black = 0.0001f32;
    let log_cc_black = encode_transfer_function(deep_black, TransferFunction::ACEScc);
    let log_cct_black = encode_transfer_function(deep_black, TransferFunction::ACEScct);
    assert!(
        log_cct_black > log_cc_black,
        "ACEScct toe must lift deep shadow log values above ACEScc"
    );
    assert!(
        log_cct_black > 0.07,
        "ACEScct toe minimum offset is ~0.0729, got {}",
        log_cct_black
    );

    // 3. Bi-directional roundtrip accuracy of ACEScct across range
    for &lin in &[0.00005, 0.0005, 0.005, 0.0078125, 0.05, 0.18, 1.0, 10.0] {
        let enc = encode_transfer_function(lin, TransferFunction::ACEScct);
        let dec = decode_transfer_function(enc, TransferFunction::ACEScct);
        assert!(
            (dec - lin).abs() < 1e-3 * lin.max(0.001),
            "ACEScct roundtrip error at {}: got {}",
            lin,
            dec
        );
    }
}

#[test]
fn test_high_bitdepth_quantization_and_packed_formats() {
    // 1. 10-bit R10G10B10A2 packing & unpacking
    let test_pixels = [
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.5, 0.25, 0.75, 1.0],
        [0.18, 0.18, 0.18, 1.0], // 18% middle gray
        [0.001, 0.999, 0.500, 0.333],
    ];

    for &pix in &test_pixels {
        let packed = pack_rgb10a2(pix);
        let unpacked = unpack_rgb10a2(packed);

        // 10-bit quantization error step is 1/1023 ~= 0.000977
        for c in 0..3 {
            assert!(
                (unpacked[c] - pix[c]).abs() <= 1.0 / 1023.0 + 1e-5,
                "10-bit channel {} error for {}: got {}",
                c,
                pix[c],
                unpacked[c]
            );
        }
        // Alpha is 2-bit (quantization step 1/3)
        assert!(
            (unpacked[3] - pix[3]).abs() <= 1.0 / 3.0 + 1e-5,
            "2-bit alpha error for {}: got {}",
            pix[3],
            unpacked[3]
        );
    }

    // 2. 12-bit packing & unpacking
    for &pix in &test_pixels {
        let packed12 = pack_rgb12(pix);
        let unpacked12 = unpack_rgb12(packed12);

        // 12-bit quantization error step is 1/4095 ~= 0.000244
        for c in 0..4 {
            assert!(
                (unpacked12[c] - pix[c]).abs() <= 1.0 / 4095.0 + 1e-5,
                "12-bit channel {} error for {}: got {}",
                c,
                pix[c],
                unpacked12[c]
            );
        }
    }

    // 3. Buffer quantization
    let width = 4;
    let height = 2;
    let buffer_f32 = vec![[0.2, 0.4, 0.6, 1.0]; (width * height) as usize];

    let mut buffer_10bit = vec![0u32; (width * height) as usize];
    quantize_rgba32f_to_rgb10a2(&buffer_f32, &mut buffer_10bit, width, height, false);
    assert_eq!(buffer_10bit.len(), 8);
    let sample_unpacked = unpack_rgb10a2(buffer_10bit[0]);
    assert!((sample_unpacked[0] - 0.2).abs() < 0.002);
    assert!((sample_unpacked[1] - 0.4).abs() < 0.002);

    let mut buffer_12bit = vec![[0u16; 4]; (width * height) as usize];
    quantize_rgba32f_to_rgb12(&buffer_f32, &mut buffer_12bit, width, height, false);
    assert_eq!(buffer_12bit.len(), 8);
    let sample12_unpacked = unpack_rgb12(buffer_12bit[0]);
    assert!((sample12_unpacked[0] - 0.2).abs() < 0.0005);
}

#[test]
fn test_tpdf_dithering_banding_elimination() {
    let width = 64;
    let height = 16;
    let total_pixels = (width * height) as usize;

    // Create a very subtle gradient across 0.500 to 0.505 (sub-8-bit and fine 10-bit steps)
    let mut gradient_f32 = Vec::with_capacity(total_pixels);
    for _y in 0..height {
        for x in 0..width {
            let t = x as f32 / width as f32;
            let val = 0.500 + t * 0.005;
            gradient_f32.push([val, val, val, 1.0]);
        }
    }

    // Quantize without dither
    let mut non_dithered = vec![0u32; total_pixels];
    quantize_rgba32f_to_rgb10a2(&gradient_f32, &mut non_dithered, width, height, false);

    // Quantize with TPDF dither
    let mut dithered = vec![0u32; total_pixels];
    quantize_rgba32f_to_rgb10a2(&gradient_f32, &mut dithered, width, height, true);

    // Non-dithered should have discrete staircase runs
    // Dithered should have varying adjacent values breaking up boundaries
    let mut dither_differences = 0;
    for i in 0..total_pixels {
        if non_dithered[i] != dithered[i] {
            dither_differences += 1;
        }
    }

    assert!(
        dither_differences > 10,
        "TPDF dithering must disperse fine quantization steps: diff count = {}",
        dither_differences
    );

    // Verify bounded noise: unpacked dithered pixel must stay within 2 LSBs of truth
    for (i, &pix) in gradient_f32.iter().enumerate() {
        let unp = unpack_rgb10a2(dithered[i]);
        assert!(
            (unp[0] - pix[0]).abs() < 3.0 / 1023.0,
            "Dither noise exceeded acceptable 10-bit LSB envelope at pixel {}",
            i
        );
    }
}

#[test]
fn test_hdr10_pq_peak_luminance_and_eotf_precision() {
    // SMPTE ST 2084 PQ mapping:
    // 100 nits = 1.0 (diffuse white reference in standard cinema normalization)
    // 1000 nits = 10.0
    // 4000 nits = 40.0
    // 10,000 nits = 100.0 (peak PQ maximum)

    let test_luminances = [
        0.0001f32, // OLED minimum black (0.0001 nits)
        0.18f32,   // 18 nits (SDR dark scene)
        1.0f32,    // 100 nits (diffuse white)
        10.0f32,   // 1000 nits (HDR10 consumer OLED/miniLED peak)
        40.0f32,   // 4000 nits (Dolby Vision mastering monitor)
        100.0f32,  // 10,000 nits (full PQ scale)
    ];

    for &l_linear in &test_luminances {
        let encoded_pq = encode_transfer_function(l_linear, TransferFunction::PQ);
        assert!(
            (0.0..=1.0).contains(&encoded_pq),
            "PQ encoding must be bounded in [0.0, 1.0]: got {} for linear {}",
            encoded_pq,
            l_linear
        );

        let decoded_linear = decode_transfer_function(encoded_pq, TransferFunction::PQ);
        let relative_err = (decoded_linear - l_linear).abs() / l_linear;
        assert!(
            relative_err < 0.015,
            "PQ EOTF roundtrip relative error too large for {} nits: got {} (err: {:.4}%)",
            l_linear * 100.0,
            decoded_linear * 100.0,
            relative_err * 100.0
        );
    }
}

#[test]
fn test_color_pipeline_cdl_in_acescct() {
    let config = ColorPipelineConfig {
        input_gamut: ColorGamut::ACEScg,
        input_tf: TransferFunction::Linear,
        output_gamut: ColorGamut::Rec709,
        output_tf: TransferFunction::SRGB,
        apply_acescc_cdl: true,
        cdl: AscCdlParams {
            slope: [1.2, 1.0, 0.8], // Boost red, attenuate blue
            offset: [0.02, 0.0, -0.01],
            power: [1.0, 1.0, 1.0],
            saturation: 1.1, // Slight saturation boost
        },
        apply_tone_mapping: true,
        exposure_ev: 0.5,
        temperature_kelvin_delta: 0.0,
    };

    let pipeline = ColorPipeline::new(config.clone());
    let neutral_gray = [0.18, 0.18, 0.18, 1.0];
    let graded = pipeline.process_pixel(neutral_gray);

    // Because slope boosted red and reduced blue, graded output must have R > G > B
    assert!(
        graded[0] > graded[1],
        "Red channel ({}) should exceed Green ({}) after warm CDL slope",
        graded[0],
        graded[1]
    );
    assert!(
        graded[1] > graded[2],
        "Green channel ({}) should exceed Blue ({}) after warm CDL slope",
        graded[1],
        graded[2]
    );
    assert_eq!(graded[3], 1.0);
}
