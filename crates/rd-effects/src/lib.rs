//! # rd-effects: WGSL Effect Library
//!
//! Manages effect definitions (WGSL shaders + declarative manifests),
//! loads and compiles effects, and provides the effect registry.

pub mod color_space;
pub mod color_transform;
pub mod inpainting;
pub mod lut;
pub mod mask;
pub mod matte;
pub mod ocio;
pub mod optical_flow;
pub mod reframe;
pub mod registry;
pub mod shader;
pub mod stabilize;
pub mod template;
pub mod text;
pub mod wasm;

pub use color_space::{
    AscCdlParams, ColorGamut, ColorPipeline, ColorPipelineConfig, Mat3, TransferFunction,
    aces_filmic_tone_map, decode_transfer_function, encode_transfer_function,
    gamut_transform_matrix, pack_rgb10a2, pack_rgb10a2_dithered, pack_rgb12, pack_rgb12_dithered,
    quantize_rgba32f_to_rgb10a2, quantize_rgba32f_to_rgb12, triangular_dither_tpdf, unpack_rgb10a2,
    unpack_rgb12,
};
pub use color_transform::{ColorTransformParams, apply_color_transform};
pub use inpainting::{InpaintParams, apply_inpaint_effect};
pub use mask::{ShapeMaskParams, apply_shape_mask, evaluate_mask_alpha};
pub use matte::{
    NeuralMatteParams, VirtualGreenScreenParams, apply_neural_matte, apply_virtual_green_screen,
    evaluate_matte_alpha, evaluate_virtual_green_screen_alpha,
};
pub use ocio::{OcioBitDepth, OcioColorSpace, OcioConfig, OcioError, OcioTransform, OcioView};
pub use optical_flow::{OpticalFlowRetimeParams, apply_optical_flow_retime};
pub use reframe::{AutoReframeParams, apply_auto_reframe};
pub use registry::{
    EffectEntry, EffectManifest, EffectParamSpec, EffectRegistry, OPTICAL_FLOW_ESTIMATE_WGSL,
    OPTICAL_FLOW_WGSL,
};
pub use stabilize::{VideoStabilizerParams, apply_video_stabilizer};
pub use template::{BeatDetector, MusicCue, TemplateSlot, TemplateSpec};
pub use text::{
    SdfTypographyParams, TextAlignment, TextOverlayParams, compute_signed_distance_field, edt_2d,
    generate_text_sdf_field, get_glyph_8x8, render_sdf_text_overlay, render_text_clip_overlay,
    render_text_overlay,
};
pub use wasm::{
    WasmBinaryBuilder, WasmEffectParams, WasmError, WasmInstance, WasmMemory, WasmModule, WasmOp,
    WasmParamType, WasmPluginCategory, WasmPluginInstance, WasmPluginManifest, WasmPluginParam,
    WasmPluginRegistry, WasmType, WasmValue, create_chromatic_aberration_plugin,
    create_crt_scanlines_plugin, create_vignette_pulse_plugin, global_wasm_registry,
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
