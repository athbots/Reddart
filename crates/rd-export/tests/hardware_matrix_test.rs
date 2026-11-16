//! Multi-Vendor Hardware Encoder Validation Test
//!
//! Validates:
//! 1. NVIDIA NVENC (h264_nvenc, hevc_nvenc, av1_nvenc)
//! 2. AMD AMF (h264_amf, hevc_amf, av1_amf)
//! 3. Intel QuickSync (h264_qsv, hevc_qsv, av1_qsv)
//! 4. Apple VideoToolbox (h264_videotoolbox, hevc_videotoolbox, av1_videotoolbox)
//! 5. Pure CPU Software Fallback (libx264, libx265, libsvtav1, libaom-av1)

use rd_export::{probe_hardware_encoders, select_encoder};

#[test]
fn test_hardware_encoder_probing_and_selection() {
    let report = probe_hardware_encoders();

    println!("Detected Host Video Encoder Capabilities:");
    println!("  - NVIDIA NVENC H.264: {}", report.h264_nvenc);
    println!("  - NVIDIA NVENC HEVC:  {}", report.hevc_nvenc);
    println!("  - NVIDIA NVENC AV1:   {}", report.av1_nvenc);
    println!("  - AMD AMF H.264:      {}", report.h264_amf);
    println!("  - AMD AMF HEVC:       {}", report.hevc_amf);
    println!("  - AMD AMF AV1:        {}", report.av1_amf);
    println!("  - Intel QSV H.264:    {}", report.h264_qsv);
    println!("  - Intel QSV HEVC:     {}", report.hevc_qsv);
    println!("  - Intel QSV AV1:      {}", report.av1_qsv);
    println!("  - Apple VTB H.264:    {}", report.h264_videotoolbox);
    println!("  - Apple VTB HEVC:     {}", report.hevc_videotoolbox);
    println!("  - Apple VTB AV1:      {}", report.av1_videotoolbox);
    println!("  - CPU libx264:        {}", report.libx264);
    println!("  - CPU libx265:        {}", report.libx265);
    println!("  - CPU libsvtav1:      {}", report.libsvtav1);
    println!("  - CPU libaom-av1:     {}", report.libaom_av1);
    println!("  -> Selected H.264:    {}", report.selected_h264);
    println!("  -> Selected HEVC:     {}", report.selected_hevc);
    println!("  -> Selected AV1:      {}", report.selected_av1);

    let valid_h264 = [
        "h264_nvenc",
        "h264_amf",
        "h264_qsv",
        "h264_videotoolbox",
        "libx264",
    ];
    let valid_hevc = [
        "hevc_nvenc",
        "hevc_amf",
        "hevc_qsv",
        "hevc_videotoolbox",
        "libx265",
    ];
    let valid_av1 = [
        "av1_nvenc",
        "av1_amf",
        "av1_qsv",
        "av1_videotoolbox",
        "libsvtav1",
        "libaom-av1",
    ];

    assert!(
        valid_h264.contains(&report.selected_h264),
        "Selected H.264 encoder must be valid: {}",
        report.selected_h264
    );
    assert!(
        valid_hevc.contains(&report.selected_hevc),
        "Selected HEVC encoder must be valid: {}",
        report.selected_hevc
    );
    assert!(
        valid_av1.contains(&report.selected_av1),
        "Selected AV1 encoder must be valid: {}",
        report.selected_av1
    );

    assert_eq!(select_encoder("h264"), report.selected_h264);
    assert_eq!(select_encoder("hevc"), report.selected_hevc);
    assert_eq!(select_encoder("h265"), report.selected_hevc);
    assert_eq!(select_encoder("av1"), report.selected_av1);
}
