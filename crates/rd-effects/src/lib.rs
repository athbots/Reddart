//! # rd-effects: WGSL Effect Library
//!
//! Manages effect definitions (WGSL shaders + declarative manifests),
//! loads and compiles effects, and provides the effect registry.

pub mod lut;
pub mod mask;
pub mod registry;
pub mod shader;
pub mod template;
pub mod text;

pub use mask::{ShapeMaskParams, apply_shape_mask, evaluate_mask_alpha};
pub use template::{BeatDetector, MusicCue, TemplateSlot, TemplateSpec};
pub use text::{
    TextAlignment, TextOverlayParams, get_glyph_8x8, render_text_clip_overlay, render_text_overlay,
};

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
