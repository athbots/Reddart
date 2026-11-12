//! Effect registry: loads and manages available WGSL effects and declarative manifests.

use crate::CostClass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameter data type supported by effect shaders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    Float,
    Int,
    Bool,
    Color,
    Vec2,
}

/// Declarative parameter specification for an effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectParamSpec {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub keyframeable: bool,
}

/// Declarative effect manifest matching manifest.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectManifest {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub cost_class: CostClass,
    pub parameters: Vec<EffectParamSpec>,
    pub shader: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
}

/// Built-in shader sources bundled directly with the core.
pub const BRIGHTNESS_CONTRAST_WGSL: &str =
    include_str!("../../../effects/brightness_contrast/shader.wgsl");
pub const COMPOSITE_BLEND_WGSL: &str = include_str!("../../../effects/composite_blend/shader.wgsl");
pub const TEXT_SDF_WGSL: &str = include_str!("../../../effects/text_sdf/shader.wgsl");
pub const CROSS_DISSOLVE_WGSL: &str = include_str!("../../../effects/cross_dissolve/shader.wgsl");
pub const DIRECTIONAL_WIPE_WGSL: &str =
    include_str!("../../../effects/directional_wipe/shader.wgsl");
pub const CHROMA_KEY_WGSL: &str = include_str!("../../../effects/chroma_key/shader.wgsl");
pub const LUT_3D_WGSL: &str = include_str!("../../../effects/lut_3d/shader.wgsl");
pub const MASK_SHAPE_WGSL: &str = include_str!("../../../effects/mask_shape/shader.wgsl");
pub const NEURAL_MATTE_WGSL: &str = include_str!("../../../effects/neural_matte/shader.wgsl");
pub const AUTO_REFRAME_WGSL: &str = include_str!("../../../effects/auto_reframe/shader.wgsl");
pub const OPTICAL_FLOW_WGSL: &str = include_str!("../../../effects/optical_flow/shader.wgsl");

/// An effect entry registered in the engine.
pub struct EffectEntry {
    pub manifest: EffectManifest,
    pub wgsl_source: String,
}

/// Registry of all available effects.
pub struct EffectRegistry {
    effects: HashMap<String, EffectEntry>,
}

impl EffectRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            effects: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Register built-in default shaders.
    fn register_builtins(&mut self) {
        // 1. Brightness / Contrast
        self.register(
            EffectManifest {
                id: "brightness_contrast".into(),
                name: "Brightness / Contrast".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![
                    EffectParamSpec {
                        id: "brightness".into(),
                        name: "Brightness".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: -1.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "contrast".into(),
                        name: "Contrast".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: -1.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            BRIGHTNESS_CONTRAST_WGSL.into(),
        );

        // 2. Composite Blend
        self.register(
            EffectManifest {
                id: "composite_blend".into(),
                name: "Composite Blend".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![
                    EffectParamSpec {
                        id: "opacity".into(),
                        name: "Opacity".into(),
                        param_type: ParamType::Float,
                        default: 1.0,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "blend_mode".into(),
                        name: "Blend Mode".into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 3.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            COMPOSITE_BLEND_WGSL.into(),
        );

        // 3. Text SDF
        self.register(
            EffectManifest {
                id: "text_sdf".into(),
                name: "Text SDF".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![
                    EffectParamSpec {
                        id: "outline_width".into(),
                        name: "Outline Width".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: 0.0,
                        max: 0.3,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "smoothing".into(),
                        name: "Smoothing".into(),
                        param_type: ParamType::Float,
                        default: 0.04,
                        min: 0.01,
                        max: 0.1,
                        step: 0.005,
                        keyframeable: false,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            TEXT_SDF_WGSL.into(),
        );

        // 4. Cross Dissolve Transition
        self.register(
            EffectManifest {
                id: "cross_dissolve".into(),
                name: "Cross Dissolve".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![EffectParamSpec {
                    id: "progress".into(),
                    name: "Progress".into(),
                    param_type: ParamType::Float,
                    default: 0.0,
                    min: 0.0,
                    max: 1.0,
                    step: 0.01,
                    keyframeable: true,
                }],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            CROSS_DISSOLVE_WGSL.into(),
        );

        // 5. Directional Wipe Transition
        self.register(
            EffectManifest {
                id: "directional_wipe".into(),
                name: "Directional Wipe".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![
                    EffectParamSpec {
                        id: "progress".into(),
                        name: "Progress".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "feather".into(),
                        name: "Feather".into(),
                        param_type: ParamType::Float,
                        default: 0.05,
                        min: 0.001,
                        max: 0.5,
                        step: 0.01,
                        keyframeable: false,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            DIRECTIONAL_WIPE_WGSL.into(),
        );

        // 6. Chroma Key
        self.register(
            EffectManifest {
                id: "chroma_key".into(),
                name: "Chroma Key".into(),
                version: 1,
                cost_class: CostClass::Medium,
                parameters: vec![
                    EffectParamSpec {
                        id: "similarity".into(),
                        name: "Similarity".into(),
                        param_type: ParamType::Float,
                        default: 0.35,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "smoothness".into(),
                        name: "Smoothness".into(),
                        param_type: ParamType::Float,
                        default: 0.1,
                        min: 0.001,
                        max: 0.5,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "spill".into(),
                        name: "Spill Suppression".into(),
                        param_type: ParamType::Float,
                        default: 0.6,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            CHROMA_KEY_WGSL.into(),
        );

        // 7. 3D LUT Color Grade
        self.register(
            EffectManifest {
                id: "lut_3d".into(),
                name: "3D LUT Color Grade".into(),
                version: 1,
                cost_class: CostClass::Medium,
                parameters: vec![
                    EffectParamSpec {
                        id: "intensity".into(),
                        name: "Grade Intensity".into(),
                        param_type: ParamType::Float,
                        default: 1.0,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "lut_size".into(),
                        name: "LUT Size".into(),
                        param_type: ParamType::Float,
                        default: 33.0,
                        min: 16.0,
                        max: 64.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            LUT_3D_WGSL.into(),
        );

        // 8. Shape Mask (Rectangle, Ellipse, Linear Gradient, Vignette)
        self.register(
            EffectManifest {
                id: "mask_shape".into(),
                name: "Shape Mask".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![
                    EffectParamSpec {
                        id: "shape_type".into(),
                        name: "Shape (0:Rect, 1:Ellipse, 2:Gradient, 3:Vignette)".into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 3.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                    EffectParamSpec {
                        id: "center_x".into(),
                        name: "Center X".into(),
                        param_type: ParamType::Float,
                        default: 0.5,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "center_y".into(),
                        name: "Center Y".into(),
                        param_type: ParamType::Float,
                        default: 0.5,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "size_x".into(),
                        name: "Size X / Radius".into(),
                        param_type: ParamType::Float,
                        default: 0.35,
                        min: 0.01,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "size_y".into(),
                        name: "Size Y".into(),
                        param_type: ParamType::Float,
                        default: 0.35,
                        min: 0.01,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "feather".into(),
                        name: "Edge Feather".into(),
                        param_type: ParamType::Float,
                        default: 0.05,
                        min: 0.001,
                        max: 0.5,
                        step: 0.005,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "invert".into(),
                        name: "Invert Mask (0 or 1)".into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 1.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            MASK_SHAPE_WGSL.into(),
        );

        // 9. AI Portrait Matte (Neural Background Removal)
        self.register(
            EffectManifest {
                id: "neural_matte".into(),
                name: "AI Portrait Matte".into(),
                version: 1,
                cost_class: CostClass::Medium,
                parameters: vec![
                    EffectParamSpec {
                        id: "threshold".into(),
                        name: "Matte Threshold".into(),
                        param_type: ParamType::Float,
                        default: 0.45,
                        min: 0.1,
                        max: 0.9,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "feather".into(),
                        name: "Edge Feather".into(),
                        param_type: ParamType::Float,
                        default: 0.05,
                        min: 0.001,
                        max: 0.5,
                        step: 0.005,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "edge_choke".into(),
                        name: "Edge Choke".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: -0.5,
                        max: 0.5,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "spill".into(),
                        name: "Spill Suppression".into(),
                        param_type: ParamType::Float,
                        default: 0.6,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "invert".into(),
                        name: "Invert Matte (0 or 1)".into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 1.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                    EffectParamSpec {
                        id: "bg_mode".into(),
                        name: "Background Mode (0:Transparent, 1:Solid, 2:Desaturate, 3:Dim)"
                            .into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 3.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            NEURAL_MATTE_WGSL.into(),
        );

        // 10. AI Auto Reframe
        self.register(
            EffectManifest {
                id: "auto_reframe".into(),
                name: "AI Auto Reframe".into(),
                version: 1,
                cost_class: CostClass::Low,
                parameters: vec![
                    EffectParamSpec {
                        id: "target_aspect".into(),
                        name: "Target Aspect".into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 3.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                    EffectParamSpec {
                        id: "pan_x".into(),
                        name: "Horizontal Pan".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: -0.5,
                        max: 0.5,
                        step: 0.005,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "pan_y".into(),
                        name: "Vertical Pan".into(),
                        param_type: ParamType::Float,
                        default: 0.0,
                        min: -0.5,
                        max: 0.5,
                        step: 0.005,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "zoom".into(),
                        name: "Crop Zoom Scale".into(),
                        param_type: ParamType::Float,
                        default: 1.0,
                        min: 0.5,
                        max: 3.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "fill_mode".into(),
                        name: "Fill Mode".into(),
                        param_type: ParamType::Int,
                        default: 0.0,
                        min: 0.0,
                        max: 2.0,
                        step: 1.0,
                        keyframeable: false,
                    },
                    EffectParamSpec {
                        id: "background_blur".into(),
                        name: "Background Blur Radius".into(),
                        param_type: ParamType::Float,
                        default: 8.0,
                        min: 0.0,
                        max: 20.0,
                        step: 0.5,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "background_dim".into(),
                        name: "Background Dim Factor".into(),
                        param_type: ParamType::Float,
                        default: 0.45,
                        min: 0.0,
                        max: 1.0,
                        step: 0.05,
                        keyframeable: true,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            AUTO_REFRAME_WGSL.into(),
        );

        self.register(
            EffectManifest {
                id: "optical_flow".into(),
                name: "Optical Flow Retiming".into(),
                version: 1,
                cost_class: CostClass::High,
                parameters: vec![
                    EffectParamSpec {
                        id: "retime_speed".into(),
                        name: "Playback Speed".into(),
                        param_type: ParamType::Float,
                        default: 0.5,
                        min: 0.01,
                        max: 10.0,
                        step: 0.05,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "blend_time".into(),
                        name: "Interpolation Phase".into(),
                        param_type: ParamType::Float,
                        default: 0.5,
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "flow_smoothness".into(),
                        name: "Motion Regularization".into(),
                        param_type: ParamType::Float,
                        default: 0.5,
                        min: 0.0,
                        max: 1.0,
                        step: 0.05,
                        keyframeable: true,
                    },
                    EffectParamSpec {
                        id: "occlusion_suppression".into(),
                        name: "Occlusion Suppression".into(),
                        param_type: ParamType::Float,
                        default: 0.8,
                        min: 0.0,
                        max: 1.0,
                        step: 0.05,
                        keyframeable: true,
                    },
                ],
                shader: "shader.wgsl".into(),
                vertex_entry: "vs_main".into(),
                fragment_entry: "fs_main".into(),
            },
            OPTICAL_FLOW_WGSL.into(),
        );
    }

    /// Register an effect with its manifest and WGSL code.
    pub fn register(&mut self, manifest: EffectManifest, wgsl_source: String) {
        log::info!("Registered effect: {} ({})", manifest.name, manifest.id);
        self.effects.insert(
            manifest.id.clone(),
            EffectEntry {
                manifest,
                wgsl_source,
            },
        );
    }

    /// Retrieve an effect by ID.
    pub fn get(&self, id: &str) -> Option<&EffectEntry> {
        self.effects.get(id)
    }

    /// List all registered effects.
    pub fn list(&self) -> impl Iterator<Item = &EffectManifest> {
        self.effects.values().map(|entry| &entry.manifest)
    }
}

impl Default for EffectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtins_registered() {
        let registry = EffectRegistry::new();
        assert!(registry.get("brightness_contrast").is_some());
        assert!(registry.get("composite_blend").is_some());
        assert!(registry.get("text_sdf").is_some());
        assert!(registry.get("cross_dissolve").is_some());
        assert!(registry.get("directional_wipe").is_some());
        assert!(registry.get("chroma_key").is_some());
        assert!(registry.get("lut_3d").is_some());
        assert!(registry.get("mask_shape").is_some());
        assert!(registry.get("neural_matte").is_some());
        assert!(registry.get("auto_reframe").is_some());
        assert!(registry.get("optical_flow").is_some());

        let of = registry.get("optical_flow").unwrap();
        assert_eq!(of.manifest.cost_class, CostClass::High);
        assert_eq!(of.manifest.parameters.len(), 4);
        assert!(of.wgsl_source.contains("fn fs_main"));

        let ar = registry.get("auto_reframe").unwrap();
        assert_eq!(ar.manifest.cost_class, CostClass::Low);
        assert_eq!(ar.manifest.parameters.len(), 7);
        assert!(ar.wgsl_source.contains("fn fs_main"));

        let nm = registry.get("neural_matte").unwrap();
        assert_eq!(nm.manifest.cost_class, CostClass::Medium);
        assert_eq!(nm.manifest.parameters.len(), 6);
        assert!(nm.wgsl_source.contains("fn fs_main"));

        let mask = registry.get("mask_shape").unwrap();
        assert_eq!(mask.manifest.cost_class, CostClass::Low);
        assert_eq!(mask.manifest.parameters.len(), 7);
        assert!(mask.wgsl_source.contains("fn fs_main"));

        let ck = registry.get("chroma_key").unwrap();
        assert_eq!(ck.manifest.cost_class, CostClass::Medium);
        assert_eq!(ck.manifest.parameters.len(), 3);

        let bc = registry.get("brightness_contrast").unwrap();
        assert_eq!(bc.manifest.cost_class, CostClass::Low);
        assert_eq!(bc.manifest.parameters.len(), 2);
        assert!(bc.wgsl_source.contains("fn fs_main"));
    }
}
