//! Software AI Portrait Matte evaluator matching `effects/neural_matte/shader.wgsl`
//! for CPU preview fallback, offline rendering, and background segmentation.

use serde::{Deserialize, Serialize};

/// Parameters for software AI portrait matte evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NeuralMatteParams {
    /// Foreground subject detection sensitivity threshold (0.1 to 0.9, default 0.45).
    pub threshold: f32,
    /// Edge feather blend width (0.001 to 0.5, default 0.05).
    pub feather: f32,
    /// Edge choke: negative = erode mask, positive = dilate mask (-0.5 to 0.5, default 0.0).
    pub edge_choke: f32,
    /// Green/blue spill suppression intensity (0.0 to 1.0, default 0.6).
    pub spill: f32,
    /// Invert matte mask.
    pub invert: bool,
    /// Background replacement mode:
    /// - 0: Transparent alpha
    /// - 1: Solid Black background
    /// - 2: Desaturated monochrome background
    /// - 3: Dimmed cinematic background
    pub bg_mode: u32,
}

impl Default for NeuralMatteParams {
    fn default() -> Self {
        Self {
            threshold: 0.45,
            feather: 0.05,
            edge_choke: 0.0,
            spill: 0.6,
            invert: false,
            bg_mode: 0,
        }
    }
}

/// Hermite interpolation matching GLSL/WGSL `smoothstep`.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Calculate matte alpha (0.0 to 1.0) at normalized UV and RGB coordinates.
pub fn evaluate_matte_alpha(
    uv_x: f32,
    uv_y: f32,
    r: f32,
    g: f32,
    b: f32,
    params: &NeuralMatteParams,
) -> f32 {
    // 1. Center prior: portraits are centered in camera frame
    let dx = uv_x - 0.5;
    let dy = uv_y - 0.5;
    let dist = (dx * dx + dy * dy).sqrt() / std::f32::consts::FRAC_1_SQRT_2;
    let center_prior = (1.0 - dist * 0.85).clamp(0.0, 1.0);

    // 2. YCbCr skin locus evaluation
    let cb = -0.168736 * r - 0.331264 * g + 0.5 * b + 0.5;
    let cr = 0.5 * r - 0.418688 * g - 0.081312 * b + 0.5;
    let chroma_dist = ((cb - 0.5) * (cb - 0.5) + (cr - 0.5) * (cr - 0.5)).sqrt();

    let mut skin_score = 0.0_f32;
    if chroma_dist > 0.035 && (0.32..=0.49).contains(&cb) && (0.52..=0.75).contains(&cr) {
        skin_score = 0.6;
    }

    let fg_prob = (center_prior * 0.4 + skin_score).clamp(0.0, 1.0);

    // 3. Threshold & Edge Feathering
    let eff_threshold = (params.threshold - params.edge_choke).clamp(0.05, 0.95);
    let edge_lo = eff_threshold - params.feather * 0.5;
    let edge_hi = eff_threshold + params.feather * 0.5;
    let mut alpha = smoothstep(edge_lo, edge_hi, fg_prob);

    if params.invert {
        alpha = 1.0 - alpha;
    }

    alpha.clamp(0.0, 1.0)
}

/// Apply AI portrait matte to an RGBA byte slice in-place.
pub fn apply_neural_matte(rgba: &mut [u8], width: u32, height: u32, params: &NeuralMatteParams) {
    if width == 0 || height == 0 {
        return;
    }
    let w_f = width as f32;
    let h_f = height as f32;

    for y in 0..height {
        let uv_y = (y as f32 + 0.5) / h_f;
        let row_start = (y * width * 4) as usize;

        for x in 0..width {
            let uv_x = (x as f32 + 0.5) / w_f;
            let px = row_start + (x * 4) as usize;

            let r = rgba[px] as f32 / 255.0;
            let g = rgba[px + 1] as f32 / 255.0;
            let b = rgba[px + 2] as f32 / 255.0;
            let a = rgba[px + 3] as f32 / 255.0;

            let alpha = evaluate_matte_alpha(uv_x, uv_y, r, g, b, params);

            // Despill: green/blue screen reflection removal
            let excess_g = (g - (r + b) * 0.5).max(0.0);
            let despilled_g = (g - excess_g * params.spill).clamp(0.0, 1.0);

            let (final_r, final_g, final_b, final_a) = match params.bg_mode {
                1 => {
                    // Solid black background
                    (r * alpha, despilled_g * alpha, b * alpha, 1.0)
                }
                2 => {
                    // Desaturate background
                    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    let out_r = lum * (1.0 - alpha) + r * alpha;
                    let out_g = lum * (1.0 - alpha) + despilled_g * alpha;
                    let out_b = lum * (1.0 - alpha) + b * alpha;
                    (out_r, out_g, out_b, a)
                }
                3 => {
                    // Dim background
                    let dim = 0.3;
                    let out_r = (r * dim) * (1.0 - alpha) + r * alpha;
                    let out_g = (g * dim) * (1.0 - alpha) + despilled_g * alpha;
                    let out_b = (b * dim) * (1.0 - alpha) + b * alpha;
                    (out_r, out_g, out_b, a)
                }
                _ => {
                    // Default 0: Transparent alpha
                    (r, despilled_g, b, a * alpha)
                }
            };

            rgba[px] = (final_r * 255.0).clamp(0.0, 255.0) as u8;
            rgba[px + 1] = (final_g * 255.0).clamp(0.0, 255.0) as u8;
            rgba[px + 2] = (final_b * 255.0).clamp(0.0, 255.0) as u8;
            rgba[px + 3] = (final_a * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neural_matte_evaluation() {
        let params = NeuralMatteParams::default();

        // Center portrait with warm skin color
        let center_alpha = evaluate_matte_alpha(0.5, 0.5, 0.85, 0.60, 0.50, &params);
        assert!(
            center_alpha > 0.8,
            "Expected high alpha in center, got {}",
            center_alpha
        );

        // Corner background pixel
        let corner_alpha = evaluate_matte_alpha(0.05, 0.05, 0.2, 0.2, 0.2, &params);
        assert!(
            corner_alpha < 0.2,
            "Expected low alpha at corner, got {}",
            corner_alpha
        );
    }

    #[test]
    fn test_apply_neural_matte_in_place() {
        let w = 16;
        let h = 16;
        let mut pixels = vec![30u8; w * h * 4];

        // Put warm subject in center
        for y in 6..10 {
            for x in 6..10 {
                let px = (y * w + x) * 4;
                pixels[px] = 220; // R
                pixels[px + 1] = 160; // G
                pixels[px + 2] = 130; // B
                pixels[px + 3] = 255; // A
            }
        }

        let params = NeuralMatteParams::default();
        apply_neural_matte(&mut pixels, w as u32, h as u32, &params);

        // Center subject should remain opaque
        let center_px = (8 * w + 8) * 4;
        assert!(pixels[center_px + 3] > 200);

        // Corner background should be transparent
        let corner_px = 0;
        assert!(pixels[corner_px + 3] < 50);
    }

    #[test]
    fn test_neural_matte_desaturate_mode() {
        let w = 8;
        let h = 8;
        let mut pixels = vec![0u8; w * h * 4];
        // Green corner background
        pixels[0] = 50;
        pixels[1] = 200;
        pixels[2] = 50;
        pixels[3] = 255;

        let params = NeuralMatteParams {
            bg_mode: 2, // Desaturate
            ..Default::default()
        };
        apply_neural_matte(&mut pixels, w as u32, h as u32, &params);

        // Background pixel should now be largely desaturated (R ~= G ~= B)
        let r = pixels[0] as i32;
        let g = pixels[1] as i32;
        let b = pixels[2] as i32;
        assert!((r - g).abs() < 30);
        assert!((g - b).abs() < 30);
    }
}
