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

pub use pipeline::{ExportJob, ExportProgress};
pub use preset::ExportPreset;
