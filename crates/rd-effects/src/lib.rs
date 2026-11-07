//! # rd-effects: WGSL Effect Library
//!
//! Manages effect definitions (WGSL shaders + declarative manifests),
//! loads and compiles effects, and provides the effect registry.

pub mod lut;
pub mod mask;
pub mod registry;
pub mod shader;
pub mod template;

pub use mask::{ShapeMaskParams, apply_shape_mask, evaluate_mask_alpha};

/// An effect's performance cost class, used by the quality governor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CostClass {
    /// Cheap: simple color adjustments, opacity.
    Low,
    /// Medium: blurs, color grading, LUTs.
    Medium,
    /// Expensive: multi-pass effects, chroma key.
    High,
}
