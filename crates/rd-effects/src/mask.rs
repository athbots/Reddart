//! Software shape mask evaluator (rectangle, ellipse, linear gradient, vignette)
//! matching `effects/mask_shape/shader.wgsl` for CPU fallback and frame grading.

/// Parameters for software shape mask evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeMaskParams {
    /// 0: Rectangle, 1: Ellipse, 2: Linear Gradient, 3: Vignette
    pub shape_type: u32,
    /// Center X (0.0 to 1.0, default 0.5)
    pub center_x: f32,
    /// Center Y (0.0 to 1.0, default 0.5)
    pub center_y: f32,
    /// Size X or radius (0.01 to 1.0, default 0.35)
    pub size_x: f32,
    /// Size Y (0.01 to 1.0, default 0.35)
    pub size_y: f32,
    /// Feather width (0.001 to 0.5, default 0.05)
    pub feather: f32,
    /// Invert mask
    pub invert: bool,
}

impl Default for ShapeMaskParams {
    fn default() -> Self {
        Self {
            shape_type: 0,
            center_x: 0.5,
            center_y: 0.5,
            size_x: 0.35,
            size_y: 0.35,
            feather: 0.05,
            invert: false,
        }
    }
}

/// Hermite interpolation matching GLSL/WGSL `smoothstep`.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Calculate mask alpha (0.0 to 1.0) at normalized UV coordinates.
pub fn evaluate_mask_alpha(uv_x: f32, uv_y: f32, params: &ShapeMaskParams) -> f32 {
    let dx = uv_x - params.center_x;
    let dy = uv_y - params.center_y;

    let mut mask_alpha = match params.shape_type {
        0 => {
            // Rectangle SDF
            let d_x = dx.abs() - params.size_x;
            let d_y = dy.abs() - params.size_y;
            let dist = d_x.max(d_y);
            1.0 - smoothstep(-params.feather, 0.0, dist)
        }
        1 => {
            // Ellipse SDF
            let norm_x = dx / params.size_x.max(0.001);
            let norm_y = dy / params.size_y.max(0.001);
            let dist = (norm_x * norm_x + norm_y * norm_y).sqrt() - 1.0;
            1.0 - smoothstep(-params.feather, 0.0, dist)
        }
        2 => {
            // Linear Gradient along Y
            let t = (uv_y - (params.center_y - params.size_y)) / (params.size_y * 2.0).max(0.001);
            smoothstep(0.0, 1.0, t.clamp(0.0, 1.0))
        }
        _ => {
            // Vignette radial falloff
            let dist = (dx * dx + dy * dy).sqrt();
            1.0 - smoothstep(
                params.size_x - params.feather,
                params.size_x + params.feather,
                dist,
            )
        }
    };

    if params.invert {
        mask_alpha = 1.0 - mask_alpha;
    }

    mask_alpha.clamp(0.0, 1.0)
}

/// Apply shape mask to an RGBA byte slice in-place.
pub fn apply_shape_mask(rgba: &mut [u8], width: u32, height: u32, params: &ShapeMaskParams) {
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
            let alpha = evaluate_mask_alpha(uv_x, uv_y, params);

            let idx = row_start + (x * 4) as usize;
            if idx + 4 <= rgba.len() {
                rgba[idx] = ((rgba[idx] as f32) * alpha).round() as u8;
                rgba[idx + 1] = ((rgba[idx + 1] as f32) * alpha).round() as u8;
                rgba[idx + 2] = ((rgba[idx + 2] as f32) * alpha).round() as u8;
                rgba[idx + 3] = ((rgba[idx + 3] as f32) * alpha).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_mask_center_and_edges() {
        let params = ShapeMaskParams {
            shape_type: 0,
            center_x: 0.5,
            center_y: 0.5,
            size_x: 0.2,
            size_y: 0.2,
            feather: 0.02,
            invert: false,
        };

        // Center should be fully opaque (alpha = 1.0)
        let center = evaluate_mask_alpha(0.5, 0.5, &params);
        assert!((center - 1.0).abs() < 1e-4);

        // Outside rectangle should be transparent (alpha = 0.0)
        let outside = evaluate_mask_alpha(0.8, 0.8, &params);
        assert!((outside - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_ellipse_mask_and_invert() {
        let mut params = ShapeMaskParams {
            shape_type: 1,
            center_x: 0.5,
            center_y: 0.5,
            size_x: 0.2,
            size_y: 0.2,
            feather: 0.02,
            invert: false,
        };

        let inside = evaluate_mask_alpha(0.5, 0.5, &params);
        assert_eq!(inside, 1.0);

        params.invert = true;
        let inside_inverted = evaluate_mask_alpha(0.5, 0.5, &params);
        assert_eq!(inside_inverted, 0.0);
    }

    #[test]
    fn test_apply_shape_mask_buffer() {
        let mut buffer = vec![255u8; 10 * 10 * 4];
        let params = ShapeMaskParams {
            shape_type: 0,
            center_x: 0.5,
            center_y: 0.5,
            size_x: 0.1,
            size_y: 0.1,
            feather: 0.01,
            invert: false,
        };

        apply_shape_mask(&mut buffer, 10, 10, &params);
        // Center pixel (5, 5) should stay bright
        let center_idx = (5 * 10 + 5) * 4;
        assert!(buffer[center_idx] > 200);

        // Corner pixel (0, 0) should be masked out
        assert_eq!(buffer[0], 0);
    }
}
