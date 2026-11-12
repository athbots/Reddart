//! Software AI Auto Reframe evaluator matching `effects/auto_reframe/shader.wgsl`
//! for CPU preview fallback, headless export, and multi-aspect ratio rendering.

use serde::{Deserialize, Serialize};

/// Parameters for software auto-reframe evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoReframeParams {
    /// Target aspect ratio format:
    /// - 0: 9:16 Vertical (TikTok, Instagram Reels, YouTube Shorts)
    /// - 1: 1:1 Square (Instagram Feed)
    /// - 2: 4:5 Portrait
    /// - 3: 21:9 Cinematic Ultrawide
    pub target_aspect: u32,
    /// Normalized horizontal pan offset [-0.5, 0.5].
    pub pan_x: f32,
    /// Normalized vertical pan offset [-0.5, 0.5].
    pub pan_y: f32,
    /// Dynamic crop zoom scale [0.5, 3.0].
    pub zoom: f32,
    /// Fill mode:
    /// - 0: Crop to fill
    /// - 1: Blurred background pillarbox / letterbox
    /// - 2: Solid black bars
    pub fill_mode: u32,
    /// Background blur radius for pillarbox / letterbox mode.
    pub background_blur: f32,
    /// Background dimming multiplier [0.0, 1.0].
    pub background_dim: f32,
}

impl Default for AutoReframeParams {
    fn default() -> Self {
        Self {
            target_aspect: 0, // 9:16
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            fill_mode: 0,
            background_blur: 8.0,
            background_dim: 0.45,
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

/// Apply auto-reframe crop and background fill to generate an output RGBA buffer.
pub fn apply_auto_reframe(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    params: &AutoReframeParams,
) -> Vec<u8> {
    assert_eq!(src.len(), src_width * src_height * 4);
    let mut out = vec![0u8; dst_width * dst_height * 4];

    let dst_aspect = match params.target_aspect {
        1 => 1.0_f32,
        2 => 4.0 / 5.0,
        3 => 21.0 / 9.0,
        _ => 9.0 / 16.0,
    };
    let src_aspect = src_width as f32 / src_height as f32;
    let aspect_ratio = dst_aspect / src_aspect;

    let center_x = 0.5 + params.pan_x;
    let center_y = 0.5 + params.pan_y;
    let scale = 1.0 / params.zoom.max(0.001);

    for dy in 0..dst_height {
        let uv_y = (dy as f32 + 0.5) / dst_height as f32;
        for dx in 0..dst_width {
            let uv_x = (dx as f32 + 0.5) / dst_width as f32;

            // Viewport coordinate transform
            let mut crop_u = (uv_x - 0.5) * scale;
            let mut crop_v = (uv_y - 0.5) * scale;

            if aspect_ratio < 1.0 {
                crop_u *= aspect_ratio;
            } else {
                crop_v /= aspect_ratio;
            }

            crop_u += center_x;
            crop_v += center_y;

            let is_inside = (0.0..=1.0).contains(&crop_u) && (0.0..=1.0).contains(&crop_v);
            let dst_idx = (dy * dst_width + dx) * 4;

            if params.fill_mode == 0 || is_inside {
                let pixel = sample_bilinear(src, src_width, src_height, crop_u, crop_v);
                out[dst_idx..dst_idx + 4].copy_from_slice(&pixel);
            } else if params.fill_mode == 2 {
                // Solid black bars
                out[dst_idx] = 0;
                out[dst_idx + 1] = 0;
                out[dst_idx + 2] = 0;
                out[dst_idx + 3] = 255;
            } else {
                // Blurred background pillarbox / letterbox (fill_mode == 1)
                let base = sample_bilinear(src, src_width, src_height, uv_x, uv_y);
                let dim = params.background_dim.clamp(0.0, 1.0);
                out[dst_idx] = (base[0] as f32 * dim).round() as u8;
                out[dst_idx + 1] = (base[1] as f32 * dim).round() as u8;
                out[dst_idx + 2] = (base[2] as f32 * dim).round() as u8;
                out[dst_idx + 3] = base[3];
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_reframe_crop_centering() {
        let sw = 160;
        let sh = 90;
        let dw = 90;
        let dh = 160;

        // Source has red on the left (x < 80) and blue on the right (x >= 80)
        let mut src = vec![0u8; sw * sh * 4];
        for y in 0..sh {
            for x in 0..sw {
                let idx = (y * sw + x) * 4;
                if x < 80 {
                    src[idx] = 255; // Red
                } else {
                    src[idx + 2] = 255; // Blue
                }
                src[idx + 3] = 255;
            }
        }

        // 1. Center framing (pan_x = 0.0)
        let params_center = AutoReframeParams {
            target_aspect: 0, // 9:16
            pan_x: 0.0,
            ..Default::default()
        };
        let out_center = apply_auto_reframe(&src, sw, sh, dw, dh, &params_center);
        assert_eq!(out_center.len(), dw * dh * 4);

        // 2. Pan left to frame the red subject (pan_x = -0.25)
        let params_left = AutoReframeParams {
            target_aspect: 0,
            pan_x: -0.25,
            ..Default::default()
        };
        let out_left = apply_auto_reframe(&src, sw, sh, dw, dh, &params_left);
        // Center pixel of out_left should be strongly red
        let mid_idx = (dh / 2 * dw + dw / 2) * 4;
        assert!(
            out_left[mid_idx] > 200,
            "Center pixel when panned left must be red"
        );

        // 3. Pan right to frame the blue subject (pan_x = 0.25)
        let params_right = AutoReframeParams {
            target_aspect: 0,
            pan_x: 0.25,
            ..Default::default()
        };
        let out_right = apply_auto_reframe(&src, sw, sh, dw, dh, &params_right);
        assert!(
            out_right[mid_idx + 2] > 200,
            "Center pixel when panned right must be blue"
        );
    }

    #[test]
    fn test_auto_reframe_black_bars_and_dimmed_bg() {
        let sw = 100;
        let sh = 100;
        let dw = 100;
        let dh = 100;

        let src = vec![200u8; sw * sh * 4];

        let params_black = AutoReframeParams {
            fill_mode: 2, // Black bars
            zoom: 0.5,    // Zoom out so borders appear
            ..Default::default()
        };
        let out_black = apply_auto_reframe(&src, sw, sh, dw, dh, &params_black);
        // Corner pixel should be black
        assert_eq!(out_black[0], 0);
        assert_eq!(out_black[1], 0);
        assert_eq!(out_black[2], 0);

        let params_dimmed = AutoReframeParams {
            fill_mode: 1, // Dimmed BG
            zoom: 0.5,
            background_dim: 0.50,
            ..Default::default()
        };
        let out_dimmed = apply_auto_reframe(&src, sw, sh, dw, dh, &params_dimmed);
        // Corner pixel should be dimmed (~100)
        assert!((out_dimmed[0] as i32 - 100).abs() < 5);
    }
}
