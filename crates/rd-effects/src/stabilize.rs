//! Software AI Video Stabilizer evaluator matching `effects/video_stabilizer/shader.wgsl`
//! for CPU preview fallback, headless export, and test verification.

use serde::{Deserialize, Serialize};

/// Parameters for software video stabilization evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VideoStabilizerParams {
    /// Compensatory X translation in normalized coordinate space [-1.0, 1.0].
    pub comp_dx: f32,
    /// Compensatory Y translation in normalized coordinate space [-1.0, 1.0].
    pub comp_dy: f32,
    /// Compensatory rotation in radians.
    pub comp_dtheta: f32,
    /// Compensatory zoom scale multiplier.
    pub comp_scale: f32,
    /// Dynamic margin auto-crop zoom factor (>= 1.0).
    pub zoom: f32,
    /// Border fill mode:
    /// - 0: AutoCrop (clamp to edge)
    /// - 1: Mirror borders
    /// - 2: Replicate edge clamp
    /// - 3: Blurred peripheral margin
    /// - 4: Transparent black
    pub border_mode: u32,
    /// Blur radius for blurred peripheral margin.
    pub blur_radius: f32,
}

impl Default for VideoStabilizerParams {
    fn default() -> Self {
        Self {
            comp_dx: 0.0,
            comp_dy: 0.0,
            comp_dtheta: 0.0,
            comp_scale: 1.0,
            zoom: 1.0,
            border_mode: 0,
            blur_radius: 8.0,
        }
    }
}

/// Bilinear texture sampler on RGBA byte buffer.
fn sample_bilinear(src: &[u8], width: usize, height: usize, u: f32, v: f32) -> [u8; 4] {
    let u_clamped = u.clamp(0.0, 1.0);
    let v_clamped = v.clamp(0.0, 1.0);

    let x = u_clamped * (width.saturating_sub(1) as f32);
    let y = v_clamped * (height.saturating_sub(1) as f32);

    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let idx00 = (y0 * width + x0) * 4;
    let idx10 = (y0 * width + x1) * 4;
    let idx01 = (y1 * width + x0) * 4;
    let idx11 = (y1 * width + x1) * 4;

    let mut out = [0u8; 4];
    for c in 0..4 {
        let p00 = src[idx00 + c] as f32;
        let p10 = src[idx10 + c] as f32;
        let p01 = src[idx01 + c] as f32;
        let p11 = src[idx11 + c] as f32;

        let top = p00 * (1.0 - fx) + p10 * fx;
        let bottom = p01 * (1.0 - fx) + p11 * fx;
        let val = top * (1.0 - fy) + bottom * fy;
        out[c] = val.round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Apply video stabilization warp with dynamic margin and border synthesis to an RGBA frame buffer.
pub fn apply_video_stabilizer(
    src: &[u8],
    width: usize,
    height: usize,
    params: &VideoStabilizerParams,
) -> Vec<u8> {
    assert_eq!(src.len(), width * height * 4);
    let mut out = vec![0u8; width * height * 4];

    let eff_zoom = (params.zoom * params.comp_scale).max(0.001);
    let cos_t = params.comp_dtheta.cos();
    let sin_t = params.comp_dtheta.sin();

    for y in 0..height {
        let uv_y = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let uv_x = (x as f32 + 0.5) / width as f32;

            // 1. Center coordinates
            let px = uv_x - 0.5;
            let py = uv_y - 0.5;

            // 2. Zoom scaling
            let px_scaled = px / eff_zoom;
            let py_scaled = py / eff_zoom;

            // 3. Compensatory rotation
            let px_rot = px_scaled * cos_t + py_scaled * sin_t;
            let py_rot = -px_scaled * sin_t + py_scaled * cos_t;

            // 4. Compensatory translation
            let src_u = px_rot - params.comp_dx + 0.5;
            let src_v = py_rot - params.comp_dy + 0.5;

            let is_inside = (0.0..=1.0).contains(&src_u) && (0.0..=1.0).contains(&src_v);
            let dst_idx = (y * width + x) * 4;

            if params.border_mode == 0 || is_inside {
                // AutoCrop clamp or inside valid frame
                let pixel = sample_bilinear(src, width, height, src_u, src_v);
                out[dst_idx..dst_idx + 4].copy_from_slice(&pixel);
            } else if params.border_mode == 1 {
                // Mirror borders
                let mut mir_u = src_u;
                let mut mir_v = src_v;
                if mir_u < 0.0 {
                    mir_u = -mir_u;
                } else if mir_u > 1.0 {
                    mir_u = 2.0 - mir_u;
                }
                if mir_v < 0.0 {
                    mir_v = -mir_v;
                } else if mir_v > 1.0 {
                    mir_v = 2.0 - mir_v;
                }
                let pixel = sample_bilinear(src, width, height, mir_u, mir_v);
                out[dst_idx..dst_idx + 4].copy_from_slice(&pixel);
            } else if params.border_mode == 2 {
                // Replicate clamp
                let pixel = sample_bilinear(
                    src,
                    width,
                    height,
                    src_u.clamp(0.0, 1.0),
                    src_v.clamp(0.0, 1.0),
                );
                out[dst_idx..dst_idx + 4].copy_from_slice(&pixel);
            } else if params.border_mode == 3 {
                // Blurred peripheral margin
                let base = sample_bilinear(
                    src,
                    width,
                    height,
                    src_u.clamp(0.0, 1.0),
                    src_v.clamp(0.0, 1.0),
                );
                out[dst_idx] = (base[0] as f32 * 0.7).round() as u8;
                out[dst_idx + 1] = (base[1] as f32 * 0.7).round() as u8;
                out[dst_idx + 2] = (base[2] as f32 * 0.7).round() as u8;
                out[dst_idx + 3] = base[3];
            } else {
                // Mode 4: Transparent / Black
                out[dst_idx] = 0;
                out[dst_idx + 1] = 0;
                out[dst_idx + 2] = 0;
                out[dst_idx + 3] = 0;
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_stabilization() {
        let w = 8;
        let h = 8;
        let src = vec![128u8; w * h * 4];
        let params = VideoStabilizerParams::default();
        let out = apply_video_stabilizer(&src, w, h, &params);
        assert_eq!(out, src);
    }

    #[test]
    fn test_compensatory_translation() {
        let w = 16;
        let h = 16;
        let mut src = vec![0u8; w * h * 4];
        // Put a white dot at center (8, 8)
        let center_idx = (8 * w + 8) * 4;
        src[center_idx..center_idx + 4].copy_from_slice(&[255, 255, 255, 255]);

        let params = VideoStabilizerParams {
            comp_dx: 2.0 / w as f32, // Compensate 2 pixels right
            ..Default::default()
        };
        let out = apply_video_stabilizer(&src, w, h, &params);

        // White dot should have shifted to x = 10
        let shifted_idx = (8 * w + 10) * 4;
        assert!(out[shifted_idx] > 200);
    }
}
