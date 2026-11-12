//! Software Cinema Color Space & ACES Transform evaluator matching `effects/color_transform/shader.wgsl`
//! for CPU preview fallback, headless export, and test suites.

use crate::color_space::{ColorGamut, ColorPipeline, ColorPipelineConfig, TransferFunction};
use serde::{Deserialize, Serialize};

/// Parameters for software color space & ACES transform evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorTransformParams {
    pub source_gamut: ColorGamut,
    pub source_tf: TransferFunction,
    pub target_gamut: ColorGamut,
    pub target_tf: TransferFunction,
    pub exposure: f32,
    pub tone_mapping: bool,
}

impl Default for ColorTransformParams {
    fn default() -> Self {
        Self {
            source_gamut: ColorGamut::Rec709,
            source_tf: TransferFunction::SRGB,
            target_gamut: ColorGamut::Rec709,
            target_tf: TransferFunction::SRGB,
            exposure: 0.0,
            tone_mapping: false,
        }
    }
}

/// Apply color space transformation and tone mapping to an RGBA8 buffer.
pub fn apply_color_transform(
    input: &[u8],
    width: usize,
    height: usize,
    params: &ColorTransformParams,
    output: &mut [u8],
) {
    let size = width * height * 4;
    let len = input.len().min(output.len()).min(size);
    output[..len].copy_from_slice(&input[..len]);

    let config = ColorPipelineConfig {
        input_gamut: params.source_gamut,
        input_tf: params.source_tf,
        output_gamut: params.target_gamut,
        output_tf: params.target_tf,
        apply_acescc_cdl: false,
        cdl: Default::default(),
        apply_tone_mapping: params.tone_mapping,
        exposure_ev: params.exposure,
        temperature_kelvin_delta: 0.0,
    };

    let pipeline = ColorPipeline::new(config);
    pipeline.process_rgba8_buffer(&mut output[..len]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_transform_params_default() {
        let params = ColorTransformParams::default();
        assert_eq!(params.source_gamut, ColorGamut::Rec709);
        assert_eq!(params.source_tf, TransferFunction::SRGB);
        assert_eq!(params.target_gamut, ColorGamut::Rec709);
        assert_eq!(params.target_tf, TransferFunction::SRGB);
        assert_eq!(params.exposure, 0.0);
        assert!(!params.tone_mapping);
    }

    #[test]
    fn test_apply_color_transform_identity() {
        let input = vec![128u8, 64u8, 192u8, 255u8];
        let mut output = vec![0u8; 4];
        let params = ColorTransformParams::default();

        apply_color_transform(&input, 1, 1, &params, &mut output);
        assert_eq!(input, output);
    }
}
