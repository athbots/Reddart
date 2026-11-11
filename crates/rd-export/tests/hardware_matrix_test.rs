//! Multi-Vendor Hardware Encoder Validation Test
//!
//! Validates:
//! 1. NVIDIA NVENC (h264_nvenc, hevc_nvenc)
//! 2. AMD AMF (h264_amf, hevc_amf)
//! 3. Intel QuickSync (h264_qsv, hevc_qsv)
//! 4. Apple VideoToolbox (h264_videotoolbox, hevc_videotoolbox)
//! 5. Pure CPU Software Fallback (libx264, libx265)

use rd_export::{probe_hardware_encoders, select_encoder};

#[test]
fn test_hardware_encoder_probing_and_selection() {
    let report = probe_hardware_encoders();

    println!("Detected Host Video Encoder Capabilities:");
    println!("  - NVIDIA NVENC H.264: {}", report.h264_nvenc);
    println!("  - NVIDIA NVENC HEVC:  {}", report.hevc_nvenc);
    println!("  - AMD AMF H.264:      {}", report.h264_amf);
    println!("  - AMD AMF HEVC:       {}", report.hevc_amf);
    println!("  - Intel QSV H.264:    {}", report.h264_qsv);
    println!("  - Intel QSV HEVC:     {}", report.hevc_qsv);
    println!("  - Apple VTB H.264:    {}", report.h264_videotoolbox);
    println!("  - Apple VTB HEVC:     {}", report.hevc_videotoolbox);
    println!("  - CPU libx264:        {}", report.libx264);
    println!("  - CPU libx265:        {}", report.libx265);
    println!("  -> Selected H.264:    {}", report.selected_h264);
    println!("  -> Selected HEVC:     {}", report.selected_hevc);

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

    assert_eq!(select_encoder("h264"), report.selected_h264);
    assert_eq!(select_encoder("hevc"), report.selected_hevc);
    assert_eq!(select_encoder("h265"), report.selected_hevc);
}
