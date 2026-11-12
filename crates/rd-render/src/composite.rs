//! Multi-layer compositing engine.
//!
//! Technical Proposal Section 3.4 & Gate 0:
//! - Multi-track video layer compositing (V1, V2, V3...).
//! - Keyframed opacity, spatial transforms (translation, scale), and blend modes.
//! - Direct pixel alpha blending and effect integration.

use rd_effects::ShapeMaskParams;

/// Parameters defining a video layer to be composited into a multi-track frame.
#[derive(Debug, Clone)]
pub struct CompositorLayer {
    /// Source RGBA pixel buffer.
    pub rgba: Vec<u8>,
    /// Source buffer width in pixels.
    pub width: u32,
    /// Source buffer height in pixels.
    pub height: u32,
    /// Effective layer opacity in [0.0, 1.0].
    pub opacity: f32,
    /// Horizontal offset in normalized coordinate space (-1.0 to 1.0, where 0.0 is center).
    pub pos_x: f32,
    /// Vertical offset in normalized coordinate space (-1.0 to 1.0, where 0.0 is center).
    pub pos_y: f32,
    /// Horizontal scale multiplier (1.0 = 100% scale).
    pub scale_x: f32,
    /// Vertical scale multiplier (1.0 = 100% scale).
    pub scale_y: f32,
    /// Blend mode: 0 = Normal (Over), 1 = Additive, 2 = Multiply, 3 = Screen.
    pub blend_mode: u32,
    /// Optional shape mask parameters.
    pub shape_mask: Option<ShapeMaskParams>,
}

impl CompositorLayer {
    /// Create a full-frame 1.0-opacity standard layer.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            rgba,
            width,
            height,
            opacity: 1.0,
            pos_x: 0.0,
            pos_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            blend_mode: 0,
            shape_mask: None,
        }
    }

    /// Create a layer from an existing RGBA buffer with custom transform and blend parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn with_transform(
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        opacity: f32,
        pos_x: f32,
        pos_y: f32,
        scale_x: f32,
        scale_y: f32,
        blend_mode: u32,
    ) -> Self {
        Self {
            rgba,
            width,
            height,
            opacity,
            pos_x,
            pos_y,
            scale_x,
            scale_y,
            blend_mode,
            shape_mask: None,
        }
    }
}

/// Composite a slice of `CompositorLayer`s from bottom to top onto `target_buffer`.
///
/// Layers are evaluated in order (index 0 is base/background, subsequent indices are overlays).
/// The function performs spatial transforms, keyframed opacity attenuation, shape mask evaluation,
/// and Porter-Duff alpha blending.
pub fn composite_layers_over(
    target_buffer: &mut [u8],
    target_w: u32,
    target_h: u32,
    layers: &[CompositorLayer],
) {
    let expected_len = (target_w * target_h * 4) as usize;
    if target_buffer.len() < expected_len || layers.is_empty() {
        return;
    }

    for layer in layers {
        composite_single_layer(target_buffer, target_w, target_h, layer);
    }
}

/// Composite a single layer over the target buffer.
fn composite_single_layer(
    target_buffer: &mut [u8],
    target_w: u32,
    target_h: u32,
    layer: &CompositorLayer,
) {
    if layer.opacity <= 0.001 || layer.width == 0 || layer.height == 0 || layer.rgba.is_empty() {
        return;
    }

    let is_identity_transform = (layer.scale_x - 1.0).abs() < 0.001
        && (layer.scale_y - 1.0).abs() < 0.001
        && layer.pos_x.abs() < 0.001
        && layer.pos_y.abs() < 0.001;

    let center_x = (target_w as f32 * 0.5) + (layer.pos_x * target_w as f32 * 0.5);
    let center_y = (target_h as f32 * 0.5) + (layer.pos_y * target_h as f32 * 0.5);
    let inv_scale_x = if layer.scale_x.abs() > 0.001 {
        1.0 / layer.scale_x
    } else {
        1.0
    };
    let inv_scale_y = if layer.scale_y.abs() > 0.001 {
        1.0 / layer.scale_y
    } else {
        1.0
    };

    let src_cx = layer.width as f32 * 0.5;
    let src_cy = layer.height as f32 * 0.5;

    for y in 0..target_h {
        let row_start = (y * target_w * 4) as usize;
        let norm_y = y as f32 / target_h as f32;

        for x in 0..target_w {
            let px_idx = row_start + (x * 4) as usize;
            let norm_x = x as f32 / target_w as f32;

            // Compute source coordinates
            let (sx, sy) =
                if is_identity_transform && layer.width == target_w && layer.height == target_h {
                    (x as f32, y as f32)
                } else {
                    let rel_x = (x as f32 - center_x) * inv_scale_x;
                    let rel_y = (y as f32 - center_y) * inv_scale_y;
                    (rel_x + src_cx, rel_y + src_cy)
                };

            if sx < 0.0 || sx >= layer.width as f32 || sy < 0.0 || sy >= layer.height as f32 {
                continue;
            }

            let src_x = (sx as u32).min(layer.width - 1);
            let src_y = (sy as u32).min(layer.height - 1);
            let src_idx = ((src_y * layer.width + src_x) * 4) as usize;

            if src_idx + 3 >= layer.rgba.len() {
                continue;
            }

            let sr = layer.rgba[src_idx] as f32;
            let sg = layer.rgba[src_idx + 1] as f32;
            let sb = layer.rgba[src_idx + 2] as f32;
            let mut sa = (layer.rgba[src_idx + 3] as f32 / 255.0) * layer.opacity;

            // Apply shape mask if present
            if let Some(ref mask) = layer.shape_mask {
                let mask_factor = evaluate_mask_pixel(norm_x, norm_y, mask);
                sa *= mask_factor;
            }

            if sa <= 0.001 {
                continue;
            }

            let dr = target_buffer[px_idx] as f32;
            let dg = target_buffer[px_idx + 1] as f32;
            let db = target_buffer[px_idx + 2] as f32;
            let da = target_buffer[px_idx + 3] as f32 / 255.0;

            let out_a = sa + da * (1.0 - sa);
            if out_a <= 0.0001 {
                continue;
            }

            let inv_out_a = 1.0 / out_a;

            match layer.blend_mode {
                1 => {
                    // Additive
                    target_buffer[px_idx] = (dr + sr * sa).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 1] = (dg + sg * sa).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 2] = (db + sb * sa).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
                }
                2 => {
                    // Multiply
                    let mult_r = (sr * dr) / 255.0;
                    let mult_g = (sg * dg) / 255.0;
                    let mult_b = (sb * db) / 255.0;
                    target_buffer[px_idx] = (mult_r * sa + dr * (1.0 - sa)).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 1] =
                        (mult_g * sa + dg * (1.0 - sa)).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 2] =
                        (mult_b * sa + db * (1.0 - sa)).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
                }
                3 => {
                    // Screen
                    let s_norm_r = sr / 255.0;
                    let d_norm_r = dr / 255.0;
                    let scr_r = (1.0 - (1.0 - s_norm_r) * (1.0 - d_norm_r)) * 255.0;

                    let s_norm_g = sg / 255.0;
                    let d_norm_g = dg / 255.0;
                    let scr_g = (1.0 - (1.0 - s_norm_g) * (1.0 - d_norm_g)) * 255.0;

                    let s_norm_b = sb / 255.0;
                    let d_norm_b = db / 255.0;
                    let scr_b = (1.0 - (1.0 - s_norm_b) * (1.0 - d_norm_b)) * 255.0;

                    target_buffer[px_idx] = (scr_r * sa + dr * (1.0 - sa)).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 1] =
                        (scr_g * sa + dg * (1.0 - sa)).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 2] =
                        (scr_b * sa + db * (1.0 - sa)).clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
                }
                _ => {
                    // Normal Porter-Duff Over
                    let out_r = (sr * sa + dr * da * (1.0 - sa)) * inv_out_a;
                    let out_g = (sg * sa + dg * da * (1.0 - sa)) * inv_out_a;
                    let out_b = (sb * sa + db * da * (1.0 - sa)) * inv_out_a;

                    target_buffer[px_idx] = out_r.clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 1] = out_g.clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 2] = out_b.clamp(0.0, 255.0) as u8;
                    target_buffer[px_idx + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// Compute shape mask coverage factor in [0.0, 1.0] for normalized coordinate (nx, ny).
fn evaluate_mask_pixel(nx: f32, ny: f32, params: &ShapeMaskParams) -> f32 {
    let dx = (nx - params.center_x).abs();
    let dy = (ny - params.center_y).abs();
    let feather = params.feather.max(0.001);

    let inside = match params.shape_type {
        0 => {
            // Rectangle
            let hx = params.size_x * 0.5;
            let hy = params.size_y * 0.5;
            if dx <= hx && dy <= hy {
                let edge_dist_x = hx - dx;
                let edge_dist_y = hy - dy;
                let edge_dist = edge_dist_x.min(edge_dist_y);
                (edge_dist / feather).clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        1 => {
            // Circle / Ellipse
            let rx = (params.size_x * 0.5).max(0.001);
            let ry = (params.size_y * 0.5).max(0.001);
            let norm_dist = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
            if norm_dist <= 1.0 {
                ((1.0 - norm_dist) * rx / feather).clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        _ => 1.0,
    };

    if params.invert { 1.0 - inside } else { inside }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_layer_compositing_order() {
        let w = 4u32;
        let h = 4u32;
        let mut target = vec![0u8; (w * h * 4) as usize];

        // Layer 1: Solid Red (255, 0, 0, 255)
        let red_layer = CompositorLayer::new([255, 0, 0, 255].repeat((w * h) as usize), w, h);

        // Layer 2: 50% Opacity Blue (0, 0, 255, 255)
        let mut blue_layer = CompositorLayer::new([0, 0, 255, 255].repeat((w * h) as usize), w, h);
        blue_layer.opacity = 0.5;

        composite_layers_over(&mut target, w, h, &[red_layer, blue_layer]);

        // Blending 50% Blue over 100% Red yields Purple (~128 Red, 0 Green, ~128 Blue, 255 Alpha)
        let first_pixel = &target[0..4];
        assert_eq!(first_pixel[3], 255);
        assert!((first_pixel[0] as i16 - 128).abs() <= 2);
        assert_eq!(first_pixel[1], 0);
        assert!((first_pixel[2] as i16 - 128).abs() <= 2);
    }

    #[test]
    fn test_multi_layer_pip_transform() {
        let w = 10u32;
        let h = 10u32;
        let mut target = vec![0u8; (w * h * 4) as usize];

        // Background: Solid Green
        let bg = CompositorLayer::new([0, 255, 0, 255].repeat((w * h) as usize), w, h);

        // Overlay: Small Solid White box (scale 0.5 centered)
        let mut pip = CompositorLayer::new([255, 255, 255, 255].repeat((w * h) as usize), w, h);
        pip.scale_x = 0.4;
        pip.scale_y = 0.4;

        composite_layers_over(&mut target, w, h, &[bg, pip]);

        // Center pixel (5, 5) should be covered by PIP (White)
        let center_idx = ((5 * w + 5) * 4) as usize;
        assert_eq!(&target[center_idx..center_idx + 4], &[255, 255, 255, 255]);

        // Corner pixel (0, 0) should remain Green (background)
        assert_eq!(&target[0..4], &[0, 255, 0, 255]);
    }
}
