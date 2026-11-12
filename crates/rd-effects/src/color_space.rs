//! Professional Cinema Color Space Pipeline, ACEScc Grading, and HDR Transforms.
//!
//! Provides mathematically rigorous 32-bit floating point color transforms:
//! - Gamuts: Rec.709, Rec.2020, Display P3, ACEScg, ACES2065-1 (AP0).
//! - Transfer Functions: Linear, sRGB, Rec.709, SMPTE ST 2084 PQ (HDR10), HLG (ARIB STD-B67), ACEScc.
//! - Analytical 3x3 gamut transformation matrices.
//! - ACES Filmic tone mapping and CDL (Slope, Offset, Power, Saturation) operations.

#![allow(clippy::excessive_precision)]

use serde::{Deserialize, Serialize};

/// Supported broadcast and digital cinema color gamuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorGamut {
    /// ITU-R BT.709 / sRGB broadcast standard.
    #[default]
    Rec709,
    /// DCI-P3 / Display P3 wide color gamut (Apple / Cinema).
    DisplayP3,
    /// ITU-R BT.2020 ultra-wide color gamut for UHD and HDR10.
    Rec2020,
    /// Academy Color Encoding System AP1 working gamut (ACEScg).
    ACEScg,
    /// Academy Color Encoding System AP0 archival gamut (ACES2065-1).
    ACES2065,
}

/// Electro-Optical and Opto-Electrical Transfer Functions (EOTF / OETF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TransferFunction {
    /// Scene-referred linear light ($E = L$).
    Linear,
    /// IEC 61966-2-1 standard sRGB transfer curve.
    #[default]
    SRGB,
    /// ITU-R BT.709 standard broadcast camera curve.
    Rec709,
    /// SMPTE ST 2084 Perceptual Quantizer (PQ / HDR10 up to 10,000 nits).
    PQ,
    /// ARIB STD-B67 Hybrid Log-Gamma (HLG broadcast HDR).
    HLG,
    /// ACEScc logarithmic encoding for digital color grading.
    ACEScc,
}

/// 3x3 Matrix for 32-bit floating point linear color space conversions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    pub m: [f32; 9], // row-major: [m00, m01, m02, m10, m11, m12, m20, m21, m22]
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat3 {
    /// 3x3 Identity matrix.
    pub const fn identity() -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Multiply matrix by 3D vector [r, g, b].
    #[inline]
    pub fn mul_vec3(&self, rgb: [f32; 3]) -> [f32; 3] {
        [
            self.m[0] * rgb[0] + self.m[1] * rgb[1] + self.m[2] * rgb[2],
            self.m[3] * rgb[0] + self.m[4] * rgb[1] + self.m[5] * rgb[2],
            self.m[6] * rgb[0] + self.m[7] * rgb[1] + self.m[8] * rgb[2],
        ]
    }

    /// Compute matrix inverse if non-singular.
    pub fn inverse(&self) -> Option<Self> {
        let m = &self.m;
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6]);

        if det.abs() < 1e-12 {
            return None;
        }

        let inv_det = 1.0 / det;
        Some(Self {
            m: [
                (m[4] * m[8] - m[5] * m[7]) * inv_det,
                (m[2] * m[7] - m[1] * m[8]) * inv_det,
                (m[1] * m[5] - m[2] * m[4]) * inv_det,
                (m[5] * m[6] - m[3] * m[8]) * inv_det,
                (m[0] * m[8] - m[2] * m[6]) * inv_det,
                (m[2] * m[3] - m[0] * m[5]) * inv_det,
                (m[3] * m[7] - m[4] * m[6]) * inv_det,
                (m[1] * m[6] - m[0] * m[7]) * inv_det,
                (m[0] * m[4] - m[1] * m[3]) * inv_det,
            ],
        })
    }

    /// Multiply two 3x3 matrices: self * rhs.
    pub fn mul_mat3(&self, rhs: &Self) -> Self {
        let mut out = [0.0f32; 9];
        for row in 0..3 {
            for col in 0..3 {
                let mut sum = 0.0f32;
                for k in 0..3 {
                    sum += self.m[row * 3 + k] * rhs.m[k * 3 + col];
                }
                out[row * 3 + col] = sum;
            }
        }
        Self { m: out }
    }
}

// -----------------------------------------------------------------------------
// Precomputed Primary-to-XYZ (D65 or D60) Matrices
// -----------------------------------------------------------------------------

/// Rec.709 Linear to CIE XYZ (D65).
pub const MAT_REC709_TO_XYZ: Mat3 = Mat3 {
    m: [
        0.4124564, 0.3575761, 0.1804375, // row 0
        0.2126729, 0.7151522, 0.0721750, // row 1
        0.0193339, 0.1191920, 0.9503041, // row 2
    ],
};

/// Rec.2020 Linear to CIE XYZ (D65).
pub const MAT_REC2020_TO_XYZ: Mat3 = Mat3 {
    m: [
        0.6369580, 0.1446169, 0.1688810, // row 0
        0.2627002, 0.6779981, 0.0593017, // row 1
        0.0000000, 0.0280727, 1.0609851, // row 2
    ],
};

/// Display P3 Linear to CIE XYZ (D65).
pub const MAT_P3_TO_XYZ: Mat3 = Mat3 {
    m: [
        0.4865709, 0.2656677, 0.1982173, // row 0
        0.2289746, 0.6917385, 0.0792869, // row 1
        0.0000000, 0.0451134, 1.0439444, // row 2
    ],
};

/// ACEScg (AP1, D60 adapted to D65 via Bradford CAT02) to CIE XYZ (D65).
pub const MAT_ACESCG_TO_XYZ_D65: Mat3 = Mat3 {
    m: [
        0.6624542, 0.1340042, 0.1540306, // row 0
        0.2722287, 0.6740818, 0.0536895, // row 1
        -0.0055053, 0.0005705, 1.0101736, // row 2
    ],
};

/// ACES2065-1 (AP0, D60 adapted to D65) to CIE XYZ (D65).
pub const MAT_ACES2065_TO_XYZ_D65: Mat3 = Mat3 {
    m: [
        0.9525524, 0.0000000, 0.0000937, // row 0
        0.3439664, 0.7281661, -0.0721325, // row 1
        0.0000000, 0.0000000, 1.0088252, // row 2
    ],
};

/// Compute direct 3x3 transformation matrix from source gamut to target gamut.
pub fn gamut_transform_matrix(from: ColorGamut, to: ColorGamut) -> Mat3 {
    if from == to {
        return Mat3::identity();
    }

    let to_xyz = match from {
        ColorGamut::Rec709 => MAT_REC709_TO_XYZ,
        ColorGamut::DisplayP3 => MAT_P3_TO_XYZ,
        ColorGamut::Rec2020 => MAT_REC2020_TO_XYZ,
        ColorGamut::ACEScg => MAT_ACESCG_TO_XYZ_D65,
        ColorGamut::ACES2065 => MAT_ACES2065_TO_XYZ_D65,
    };

    let target_to_xyz = match to {
        ColorGamut::Rec709 => MAT_REC709_TO_XYZ,
        ColorGamut::DisplayP3 => MAT_P3_TO_XYZ,
        ColorGamut::Rec2020 => MAT_REC2020_TO_XYZ,
        ColorGamut::ACEScg => MAT_ACESCG_TO_XYZ_D65,
        ColorGamut::ACES2065 => MAT_ACES2065_TO_XYZ_D65,
    };

    let xyz_to_target = target_to_xyz.inverse().unwrap_or(Mat3::identity());
    xyz_to_target.mul_mat3(&to_xyz)
}

// -----------------------------------------------------------------------------
// Transfer Functions (EOTF & OETF)
// -----------------------------------------------------------------------------

/// Convert encoded nonlinear pixel value $V$ to linear light $L$.
#[inline]
pub fn decode_transfer_function(v: f32, tf: TransferFunction) -> f32 {
    match tf {
        TransferFunction::Linear => v,
        TransferFunction::SRGB => {
            let v_clamped = v.max(0.0);
            if v_clamped <= 0.04045 {
                v_clamped / 12.92
            } else {
                ((v_clamped + 0.055) / 1.055).powf(2.4)
            }
        }
        TransferFunction::Rec709 => {
            let v_clamped = v.max(0.0);
            if v_clamped < 0.081 {
                v_clamped / 4.5
            } else {
                ((v_clamped + 0.099) / 1.099).powf(1.0 / 0.45)
            }
        }
        TransferFunction::PQ => {
            // SMPTE ST 2084: 0.0 .. 1.0 maps to 0 .. 10,000 nits.
            // 1.0 in normalized cinema space corresponds to 100 nits (diffuse white), so 10,000 nits = 100.0.
            let n = v.clamp(0.0, 1.0);
            const M1: f32 = 0.1593017578125;
            const M2: f32 = 78.84375;
            const C1: f32 = 0.8359375;
            const C2: f32 = 18.8515625;
            const C3: f32 = 18.6875;

            let vp = n.powf(1.0 / M2);
            let num = (vp - C1).max(0.0);
            let den = (C2 - C3 * vp).max(1e-8);
            let l_norm = (num / den).powf(1.0 / M1);

            // Scale such that 100 nits (0.01 of 10,000 nits) = 1.0
            l_norm * 100.0
        }
        TransferFunction::HLG => {
            let ep = v.clamp(0.0, 1.0);
            const A: f32 = 0.17883277;
            const B: f32 = 0.28466892;
            const C: f32 = 0.55991073;

            let e = if ep <= 0.5 {
                (2.0 * ep).powi(2) / 3.0
            } else {
                (((ep - C) / A).exp() + B) / 12.0
            };
            // Scale to diffuse white reference
            e * 3.162 // 1000 nits peak reference -> 100 nits diffuse
        }
        TransferFunction::ACEScc => {
            // Academy ACEScc log to linear
            const LOG_THRESH: f32 = (-15.0 + 9.72) / 17.52; // ~= -0.30137
            if v < LOG_THRESH {
                let exp_val = 2.0_f32.powf(v * 17.52 - 9.72);
                (exp_val - 2.0_f32.powf(-16.0)) * 2.0
            } else {
                2.0_f32.powf(v * 17.52 - 9.72)
            }
        }
    }
}

/// Convert linear light $L$ to encoded nonlinear pixel value $V$.
#[inline]
pub fn encode_transfer_function(l: f32, tf: TransferFunction) -> f32 {
    match tf {
        TransferFunction::Linear => l,
        TransferFunction::SRGB => {
            let l_clamped = l.max(0.0);
            if l_clamped <= 0.0031308 {
                l_clamped * 12.92
            } else {
                1.055 * l_clamped.powf(1.0 / 2.4) - 0.055
            }
        }
        TransferFunction::Rec709 => {
            let l_clamped = l.max(0.0);
            if l_clamped < 0.018 {
                l_clamped * 4.5
            } else {
                1.099 * l_clamped.powf(0.45) - 0.099
            }
        }
        TransferFunction::PQ => {
            // Scale 1.0 diffuse white back to 0.01 normalized 10,000 nits
            let l_norm = (l / 100.0).clamp(0.0, 1.0);
            const M1: f32 = 0.1593017578125;
            const M2: f32 = 78.84375;
            const C1: f32 = 0.8359375;
            const C2: f32 = 18.8515625;
            const C3: f32 = 18.6875;

            let yp = l_norm.powf(M1);
            let num = C1 + C2 * yp;
            let den = 1.0 + C3 * yp;
            (num / den).powf(M2)
        }
        TransferFunction::HLG => {
            let e = (l / 3.162).max(0.0);
            const A: f32 = 0.17883277;
            const B: f32 = 0.28466892;
            const C: f32 = 0.55991073;

            if e <= (1.0 / 12.0) {
                (3.0 * e).sqrt() * 0.5
            } else {
                A * (12.0 * e - B).max(1e-8).ln() + C
            }
        }
        TransferFunction::ACEScc => {
            // Linear to Academy ACEScc log
            if l <= 0.0 {
                (-16.0 + 9.72) / 17.52
            } else if l < 2.0_f32.powf(-15.0) {
                (2.0_f32.powf(-16.0) + l * 0.5).log2() * (1.0 / 17.52) + (9.72 / 17.52)
            } else {
                l.log2() * (1.0 / 17.52) + (9.72 / 17.52)
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Cinema Tone Mapping & CDL Grading
// -----------------------------------------------------------------------------

/// ACES Filmic Tone Mapping Curve (Narkowicz / Stephen Hill fit).
/// Maps scene-referred high-dynamic-range linear light $[0.0, \infty)$ gracefully
/// into display-referred $[0.0, 1.0]$ with filmic highlight rolloff and toe.
#[inline]
pub fn aces_filmic_tone_map(x: f32) -> f32 {
    let x_pos = x.max(0.0);
    const A: f32 = 2.51;
    const B: f32 = 0.03;
    const C: f32 = 2.43;
    const D: f32 = 0.59;
    const E: f32 = 0.14;
    ((x_pos * (A * x_pos + B)) / (x_pos * (C * x_pos + D) + E)).clamp(0.0, 1.0)
}

/// Color Decision List (ASC CDL) Grading Parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AscCdlParams {
    pub slope: [f32; 3],  // RGB gain multiplier (default 1.0)
    pub offset: [f32; 3], // RGB black level pedestal (default 0.0)
    pub power: [f32; 3],  // RGB gamma power (default 1.0)
    pub saturation: f32,  // Overall chroma saturation (default 1.0)
}

impl Default for AscCdlParams {
    fn default() -> Self {
        Self {
            slope: [1.0, 1.0, 1.0],
            offset: [0.0, 0.0, 0.0],
            power: [1.0, 1.0, 1.0],
            saturation: 1.0,
        }
    }
}

impl AscCdlParams {
    /// Apply ASC CDL transfer function to an ACEScc log-encoded pixel:
    /// $out = ((in \cdot slope) + offset)^{power}$
    /// followed by saturation scaling.
    pub fn apply_cdl_acescc(&self, acescc_rgb: [f32; 3]) -> [f32; 3] {
        let mut out = [0.0f32; 3];
        for c in 0..3 {
            let val = acescc_rgb[c] * self.slope[c] + self.offset[c];
            out[c] = if val <= 0.0 {
                0.0
            } else {
                val.powf(self.power[c].max(0.01))
            };
        }

        // Apply saturation in ACEScc log space using AP1 luminance weights
        if (self.saturation - 1.0).abs() > 1e-4 {
            let luma = 0.2722287 * out[0] + 0.6740818 * out[1] + 0.0536895 * out[2];
            for item in &mut out {
                *item = luma + (*item - luma) * self.saturation;
            }
        }

        out
    }
}

// -----------------------------------------------------------------------------
// End-to-End High-Precision Color Transformation Pipeline
// -----------------------------------------------------------------------------

/// End-to-End Cinema Color Transform Configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPipelineConfig {
    pub input_gamut: ColorGamut,
    pub input_tf: TransferFunction,
    pub output_gamut: ColorGamut,
    pub output_tf: TransferFunction,
    pub apply_acescc_cdl: bool,
    pub cdl: AscCdlParams,
    pub apply_tone_mapping: bool,
    pub exposure_ev: f32,
    pub temperature_kelvin_delta: f32, // -2000.0 (cooler/blue) to +2000.0 (warmer/amber)
}

impl Default for ColorPipelineConfig {
    fn default() -> Self {
        Self {
            input_gamut: ColorGamut::Rec709,
            input_tf: TransferFunction::SRGB,
            output_gamut: ColorGamut::Rec709,
            output_tf: TransferFunction::SRGB,
            apply_acescc_cdl: false,
            cdl: AscCdlParams::default(),
            apply_tone_mapping: false,
            exposure_ev: 0.0,
            temperature_kelvin_delta: 0.0,
        }
    }
}

/// Color Pipeline Processor.
pub struct ColorPipeline {
    config: ColorPipelineConfig,
    to_linear_matrix: Mat3,
    acescg_to_output_matrix: Mat3,
}

impl ColorPipeline {
    pub fn new(config: ColorPipelineConfig) -> Self {
        // Matrix 1: Input Gamut -> ACEScg (Working Space)
        let to_linear_matrix = gamut_transform_matrix(config.input_gamut, ColorGamut::ACEScg);
        // Matrix 2: ACEScg -> Output Gamut
        let acescg_to_output_matrix =
            gamut_transform_matrix(ColorGamut::ACEScg, config.output_gamut);

        Self {
            config,
            to_linear_matrix,
            acescg_to_output_matrix,
        }
    }

    /// Process a single 32-bit floating point RGBA pixel through the full cinema color pipeline.
    pub fn process_pixel(&self, rgba: [f32; 4]) -> [f32; 4] {
        // 1. Decode input transfer function to linear light
        let mut linear_rgb = [
            decode_transfer_function(rgba[0], self.config.input_tf),
            decode_transfer_function(rgba[1], self.config.input_tf),
            decode_transfer_function(rgba[2], self.config.input_tf),
        ];

        // 2. Exposure scaling in scene-referred linear space: $L = L \cdot 2^{EV}$
        if self.config.exposure_ev.abs() > 1e-4 {
            let scale = 2.0_f32.powf(self.config.exposure_ev);
            linear_rgb[0] *= scale;
            linear_rgb[1] *= scale;
            linear_rgb[2] *= scale;
        }

        // 3. White balance chromaticity adjustment in linear space
        if self.config.temperature_kelvin_delta.abs() > 1.0 {
            let t = self.config.temperature_kelvin_delta / 2000.0;
            // Warmth: boosts red, attenuates blue; Coolness: boosts blue, attenuates red
            let r_mult = 1.0 + t * 0.15;
            let b_mult = 1.0 - t * 0.15;
            linear_rgb[0] *= r_mult.max(0.0);
            linear_rgb[2] *= b_mult.max(0.0);
        }

        // 4. Gamut transform from input to ACEScg linear working space
        let mut acescg_rgb = self.to_linear_matrix.mul_vec3(linear_rgb);

        // 5. Optional ACEScc Log Color Decision List (CDL) grading
        if self.config.apply_acescc_cdl {
            // Linear ACEScg -> ACEScc log
            let acescc_rgb = [
                encode_transfer_function(acescg_rgb[0], TransferFunction::ACEScc),
                encode_transfer_function(acescg_rgb[1], TransferFunction::ACEScc),
                encode_transfer_function(acescg_rgb[2], TransferFunction::ACEScc),
            ];

            // Apply CDL slope, offset, power, saturation
            let graded_acescc = self.config.cdl.apply_cdl_acescc(acescc_rgb);

            // ACEScc log -> Linear ACEScg
            acescg_rgb = [
                decode_transfer_function(graded_acescc[0], TransferFunction::ACEScc),
                decode_transfer_function(graded_acescc[1], TransferFunction::ACEScc),
                decode_transfer_function(graded_acescc[2], TransferFunction::ACEScc),
            ];
        }

        // 6. Optional ACES filmic tone mapping (for HDR to SDR roll-off)
        if self.config.apply_tone_mapping {
            acescg_rgb = [
                aces_filmic_tone_map(acescg_rgb[0]),
                aces_filmic_tone_map(acescg_rgb[1]),
                aces_filmic_tone_map(acescg_rgb[2]),
            ];
        }

        // 7. Gamut transform from ACEScg to target output gamut
        let target_linear_rgb = self.acescg_to_output_matrix.mul_vec3(acescg_rgb);

        // 8. Encode to target transfer function
        let out_r = encode_transfer_function(target_linear_rgb[0], self.config.output_tf);
        let out_g = encode_transfer_function(target_linear_rgb[1], self.config.output_tf);
        let out_b = encode_transfer_function(target_linear_rgb[2], self.config.output_tf);

        [out_r, out_g, out_b, rgba[3]]
    }

    /// Process an entire RGBA8 buffer in-place.
    pub fn process_rgba8_buffer(&self, buffer: &mut [u8]) {
        for chunk in buffer.chunks_exact_mut(4) {
            let r = chunk[0] as f32 / 255.0;
            let g = chunk[1] as f32 / 255.0;
            let b = chunk[2] as f32 / 255.0;
            let a = chunk[3] as f32 / 255.0;

            let out = self.process_pixel([r, g, b, a]);

            chunk[0] = (out[0] * 255.0).round().clamp(0.0, 255.0) as u8;
            chunk[1] = (out[1] * 255.0).round().clamp(0.0, 255.0) as u8;
            chunk[2] = (out[2] * 255.0).round().clamp(0.0, 255.0) as u8;
            chunk[3] = (out[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_inversion_and_identity() {
        let m = MAT_REC709_TO_XYZ;
        let inv = m.inverse().expect("Rec.709 matrix should be invertible");
        let id = m.mul_mat3(&inv);

        for (i, &v) in id.m.iter().enumerate() {
            let row = i / 3;
            let col = i % 3;
            let expected = if row == col { 1.0 } else { 0.0 };
            assert!(
                (v - expected).abs() < 1e-4,
                "Element at ({}, {}) = {} deviates from identity",
                row,
                col,
                v
            );
        }
    }

    #[test]
    fn test_srgb_transfer_function_roundtrip() {
        for &val in &[0.0, 0.001, 0.01, 0.04, 0.18, 0.5, 0.8, 1.0] {
            let linear = decode_transfer_function(val, TransferFunction::SRGB);
            let reconstructed = encode_transfer_function(linear, TransferFunction::SRGB);
            assert!(
                (reconstructed - val).abs() < 1e-4,
                "sRGB roundtrip error for {}: got {}",
                val,
                reconstructed
            );
        }
    }

    #[test]
    fn test_pq_hdr10_transfer_function_roundtrip() {
        for &val in &[0.05, 0.1, 0.3, 0.5, 0.75, 0.95] {
            let linear = decode_transfer_function(val, TransferFunction::PQ);
            let reconstructed = encode_transfer_function(linear, TransferFunction::PQ);
            assert!(
                (reconstructed - val).abs() < 1e-3,
                "PQ roundtrip error for {}: got {}",
                val,
                reconstructed
            );
        }
    }

    #[test]
    fn test_acescc_log_transfer_function_roundtrip() {
        for &lin in &[0.001, 0.01, 0.18, 1.0, 5.0, 10.0] {
            let log_val = encode_transfer_function(lin, TransferFunction::ACEScc);
            let reconstructed = decode_transfer_function(log_val, TransferFunction::ACEScc);
            assert!(
                (reconstructed - lin).abs() < 1e-3 * lin,
                "ACEScc roundtrip error for {}: got {}",
                lin,
                reconstructed
            );
        }
    }

    #[test]
    fn test_color_pipeline_identity_passthrough() {
        let config = ColorPipelineConfig::default(); // Rec.709 sRGB -> Rec.709 sRGB
        let pipeline = ColorPipeline::new(config);

        let input_pixel = [0.5, 0.25, 0.75, 1.0];
        let output_pixel = pipeline.process_pixel(input_pixel);

        for c in 0..4 {
            assert!(
                (output_pixel[c] - input_pixel[c]).abs() < 1e-3,
                "Channel {} deviates: {} vs {}",
                c,
                output_pixel[c],
                input_pixel[c]
            );
        }
    }

    #[test]
    fn test_color_pipeline_exposure_ev_boost() {
        let config = ColorPipelineConfig {
            exposure_ev: 1.0, // +1 EV = double linear light
            ..Default::default()
        };
        let pipeline = ColorPipeline::new(config);

        let input_pixel = [0.18, 0.18, 0.18, 1.0]; // 18% middle gray
        let output_pixel = pipeline.process_pixel(input_pixel);

        // Middle gray linear is ~0.026. Doubled is ~0.052, which in sRGB is ~0.26
        assert!(
            output_pixel[0] > input_pixel[0],
            "+1 EV should increase brightness"
        );
        assert!((output_pixel[0] - output_pixel[1]).abs() < 1e-4);
    }
}
