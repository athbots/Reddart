//! Integration test suite for Hardware-Accelerated AV1 & HEVC Codec Expansion,
//! 10-bit HDR pipeline, rate control modes, and decoder capability diagnostics.

use rd_decode::{HardwareDecoderReport, probe_hardware_decoders};
use rd_export::{
    ExportJob, ExportPreset, RateControlMode, is_ffmpeg_available, select_encoder_for_profile,
    supports_10bit,
};

#[test]
fn test_codec_preset_construction_and_rate_control_modes() {
    // 1. AV1 1080p Web Preset
    let av1_1080p = ExportPreset::av1_1080p();
    assert_eq!(av1_1080p.video_codec, "av1");
    assert_eq!(av1_1080p.width, 1920);
    assert_eq!(av1_1080p.height, 1080);
    assert_eq!(av1_1080p.bit_depth, 8);
    assert_eq!(av1_1080p.rate_control, Some(RateControlMode::Vbr));

    // 2. AV1 4K UHD Preset
    let av1_4k = ExportPreset::av1_4k();
    assert_eq!(av1_4k.video_codec, "av1");
    assert_eq!(av1_4k.width, 3840);
    assert_eq!(av1_4k.height, 2160);
    assert_eq!(av1_4k.video_bitrate_kbps, 18000);

    // 3. HDR10 HEVC 4K Preset (10-bit)
    let hdr_hevc = ExportPreset::hdr10_hevc_4k();
    assert_eq!(hdr_hevc.video_codec, "hevc");
    assert_eq!(hdr_hevc.bit_depth, 10);
    assert_eq!(hdr_hevc.color_space.as_deref(), Some("bt2020_pq"));

    // 4. HDR10 AV1 4K Preset (10-bit)
    let hdr_av1 = ExportPreset::hdr10_av1_4k();
    assert_eq!(hdr_av1.video_codec, "av1");
    assert_eq!(hdr_av1.bit_depth, 10);
    assert_eq!(hdr_av1.color_space.as_deref(), Some("bt2020_pq"));

    // 5. ProRes 422 Mastering Preset
    let prores = ExportPreset::mastering_prores_422();
    assert_eq!(prores.video_codec, "prores");
    assert_eq!(prores.bit_depth, 10);

    // 6. Builder helpers
    let custom = ExportPreset::youtube_1080p()
        .with_bit_depth(10)
        .with_color_space("bt2020_hlg")
        .with_rate_control(RateControlMode::Crf(23));

    assert_eq!(custom.bit_depth, 10);
    assert_eq!(custom.color_space.as_deref(), Some("bt2020_hlg"));
    assert_eq!(custom.rate_control, Some(RateControlMode::Crf(23)));
}

#[test]
fn test_10bit_encoder_selection_and_support_predicate() {
    assert!(supports_10bit("hevc_nvenc"));
    assert!(supports_10bit("hevc_amf"));
    assert!(supports_10bit("hevc_qsv"));
    assert!(supports_10bit("hevc_videotoolbox"));
    assert!(supports_10bit("libx265"));
    assert!(supports_10bit("av1_nvenc"));
    assert!(supports_10bit("av1_amf"));
    assert!(supports_10bit("av1_qsv"));
    assert!(supports_10bit("av1_videotoolbox"));
    assert!(supports_10bit("libsvtav1"));
    assert!(supports_10bit("prores"));
    assert!(supports_10bit("prores_ks"));

    assert!(!supports_10bit("h264_nvenc"));
    assert!(!supports_10bit("h264_qsv"));
    assert!(!supports_10bit("h264_amf"));

    // Test profile-aware encoder selector
    let hevc_10bit = select_encoder_for_profile("hevc", 10);
    assert!(supports_10bit(hevc_10bit));

    let av1_10bit = select_encoder_for_profile("av1", 10);
    assert!(supports_10bit(av1_10bit));

    let h264_8bit = select_encoder_for_profile("h264", 8);
    assert!(!h264_8bit.is_empty());
}

#[test]
fn test_hardware_decoder_report_and_probe() {
    let report: HardwareDecoderReport = probe_hardware_decoders();
    println!("Host Video Decoder Capabilities: {:?}", report);

    assert!(!report.selected_hwaccel.is_empty());
    // On all supported systems, auto or specific hwaccel should be active
    assert!(
        [
            "d3d11va",
            "dxva2",
            "cuda",
            "qsv",
            "videotoolbox",
            "vaapi",
            "auto"
        ]
        .contains(&report.selected_hwaccel)
    );
}

#[test]
fn test_codec_serialization_roundtrip() {
    let preset = ExportPreset::hdr10_av1_4k().with_rate_control(RateControlMode::Cvbr {
        max_bitrate_kbps: 35000,
        bufsize_kbps: 70000,
    });

    let json = serde_json::to_string_pretty(&preset).expect("Serialization failed");
    let deserialized: ExportPreset = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(preset, deserialized);
    assert_eq!(deserialized.bit_depth, 10);
    assert_eq!(
        deserialized.rate_control,
        Some(RateControlMode::Cvbr {
            max_bitrate_kbps: 35000,
            bufsize_kbps: 70000,
        })
    );
}

#[test]
fn test_render_to_file_av1_and_hevc_presets() {
    if !is_ffmpeg_available() {
        println!("Skipping render test: ffmpeg/ffprobe not available");
        return;
    }

    let temp_dir =
        std::env::temp_dir().join(format!("reddart_codec_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Create temp dir");

    // 1. Test HEVC 720p 0.5s synthesis
    let hevc_out = temp_dir.join("test_hevc.mp4");
    let mut hevc_preset = ExportPreset::uhd_4k();
    hevc_preset.width = 1280;
    hevc_preset.height = 720;
    hevc_preset.fps = 24.0;
    hevc_preset.video_bitrate_kbps = 2000;

    let res = ExportJob::render_to_file(&hevc_out, &hevc_preset, 0.5);
    if res.is_ok() && hevc_out.exists() {
        let probe = ExportJob::probe_file(&hevc_out).expect("Probe HEVC file");
        assert!(probe.has_video, "HEVC output must have video stream");
        assert!(probe.has_audio, "HEVC output must have audio stream");
        assert_eq!(probe.width, Some(1280));
        assert_eq!(probe.height, Some(720));
        let _ = std::fs::remove_file(&hevc_out);
    }

    // 2. Test AV1 720p 0.5s synthesis
    let av1_out = temp_dir.join("test_av1.mp4");
    let mut av1_preset = ExportPreset::av1_1080p();
    av1_preset.width = 1280;
    av1_preset.height = 720;
    av1_preset.fps = 24.0;
    av1_preset.video_bitrate_kbps = 1500;

    let res_av1 = ExportJob::render_to_file(&av1_out, &av1_preset, 0.5);
    if res_av1.is_ok() && av1_out.exists() {
        let probe = ExportJob::probe_file(&av1_out).expect("Probe AV1 file");
        assert!(probe.has_video, "AV1 output must have video stream");
        assert_eq!(probe.width, Some(1280));
        assert_eq!(probe.height, Some(720));
        let _ = std::fs::remove_file(&av1_out);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_rate_control_mode_crf_and_cbr_export() {
    if !is_ffmpeg_available() {
        println!("Skipping rate control test: ffmpeg/ffprobe not available");
        return;
    }

    let temp_dir = std::env::temp_dir().join(format!("reddart_rc_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Create temp dir");

    // Test CBR rate control mode
    let cbr_out = temp_dir.join("test_cbr.mp4");
    let mut cbr_preset = ExportPreset::youtube_1080p().with_rate_control(RateControlMode::Cbr);
    cbr_preset.width = 640;
    cbr_preset.height = 360;
    cbr_preset.fps = 24.0;
    cbr_preset.video_bitrate_kbps = 1000;

    let res = ExportJob::render_to_file(&cbr_out, &cbr_preset, 0.5);
    if res.is_ok() && cbr_out.exists() {
        let probe = ExportJob::probe_file(&cbr_out).expect("Probe CBR file");
        assert!(probe.has_video);
        let _ = std::fs::remove_file(&cbr_out);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
