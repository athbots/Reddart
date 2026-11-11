//! # rd-export: Export Pipeline
//!
//! Parallel pipeline: decode → GPU render → hardware encode.
//! Stages are joined by bounded queues so each stays busy.
//!
//! Technical Proposal Section 3.10:
//! - Separate export pipeline running at higher than real-time speeds.
//! - Parallel decode, render, encode stages.
//! - Presets calibrated for target social platforms.

pub mod pipeline;
pub mod preset;

pub use pipeline::{
    BackgroundExportHandle, ColorGradeSettings, ExportJob, ExportProgress, HardwareEncoderReport,
    MediaProbeResult, RenderRequest, probe_hardware_encoders, render_project_to_file,
    resample_and_grade_frame, select_encoder, start_export_project,
};
pub use preset::ExportPreset;
