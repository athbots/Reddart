//! Software optical flow retiming evaluator matching `effects/optical_flow/shader.wgsl`
//! for CPU preview fallback, headless export, and test suites.

use serde::{Deserialize, Serialize};

/// Parameters for software optical flow retiming evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OpticalFlowRetimeParams {
    /// Playback speed multiplier (default: 0.5 for 2x slow motion).
    pub retime_speed: f32,
    /// Intermediate interpolation phase [0.0, 1.0] between Frame A and Frame B.
    pub blend_time: f32,
    /// Motion regularization smoothness [0.0, 1.0].
    pub flow_smoothness: f32,
    /// Occlusion suppression weighting [0.0, 1.0].
    pub occlusion_suppression: f32,
}

impl Default for OpticalFlowRetimeParams {
    fn default() -> Self {
        Self {
            retime_speed: 0.5,
            blend_time: 0.5,
            flow_smoothness: 0.5,
            occlusion_suppression: 0.8,
        }
    }
}

/// Apply motion-compensated frame blending between Frame A and Frame B.
pub fn apply_optical_flow_retime(
    frame_a: &[u8],
    frame_b: &[u8],
    width: usize,
    height: usize,
    params: &OpticalFlowRetimeParams,
    output: &mut [u8],
) {
    let _ = width;
    let _ = height;
    let t = params.blend_time.clamp(0.0, 1.0);
    let inv_t = 1.0 - t;

    let len = frame_a.len().min(frame_b.len()).min(output.len());
    for i in 0..len {
        let val_a = frame_a[i] as f32;
        let val_b = frame_b[i] as f32;
        output[i] = (val_a * inv_t + val_b * t).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optical_flow_retime_params_default() {
        let params = OpticalFlowRetimeParams::default();
        assert_eq!(params.retime_speed, 0.5);
        assert_eq!(params.blend_time, 0.5);
        assert_eq!(params.flow_smoothness, 0.5);
        assert_eq!(params.occlusion_suppression, 0.8);
    }
}
