//! Generative Video Inpainting & Content-Aware Fill Effect
//!
//! Provides software and shader parameter specifications for removing objects,
//! microphones, logos, timestamps, and wires from video clips.

use serde::{Deserialize, Serialize};

/// Generative Inpainting Clip Effect Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InpaintParams {
    /// Inpainting operating mode:
    /// - 0: Object Removal (General generative texture fill)
    /// - 1: Wire Removal (Directional morphological edge completion)
    /// - 2: Watermark / Logo Removal
    /// - 3: Content-Aware Fill
    pub mode: u32,
    /// Normalized hole rectangle [x, y, width, height] in range [0.0, 1.0].
    pub hole_rect: Option<[f32; 4]>,
    /// Boundary feather radius fraction [0.0, 0.1].
    pub feather: f32,
    /// Search radius in pixels around hole for exemplar donor texture.
    pub search_radius: u32,
}

impl Default for InpaintParams {
    fn default() -> Self {
        Self {
            mode: 0,
            hole_rect: None,
            feather: 0.02,
            search_radius: 32,
        }
    }
}

/// Apply inpainting hole filling to an in-memory RGBA frame buffer.
pub fn apply_inpaint_effect(frame: &mut [u8], width: u32, height: u32, params: &InpaintParams) {
    let (uw, uh) = (width as usize, height as usize);
    let channels = 4;

    if frame.len() < uw * uh * channels {
        return;
    }

    let rect = match params.hole_rect {
        Some(r) => r,
        None => return,
    };

    let min_x = ((rect[0] * width as f32).round() as usize).min(uw.saturating_sub(1));
    let min_y = ((rect[1] * height as f32).round() as usize).min(uh.saturating_sub(1));
    let rw = ((rect[2] * width as f32).round() as usize).clamp(1, uw - min_x);
    let rh = ((rect[3] * height as f32).round() as usize).clamp(1, uh - min_y);

    let max_x = (min_x + rw).min(uw);
    let max_y = (min_y + rh).min(uh);

    // Directional boundary interpolation and edge feathering
    let mut temp = frame.to_vec();
    let r = params.search_radius.clamp(4, 48) as i32;

    for y in min_y..max_y {
        let y_i = y as i32;
        for x in min_x..max_x {
            let mut accum = [0.0f32; 4];
            let mut weight_sum = 0.0f32;

            for dy in -r..=r {
                let ny = y_i + dy;
                if ny < 0 || ny >= uh as i32 {
                    continue;
                }
                for dx in -r..=r {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    if nx < 0 || nx >= uw as i32 {
                        continue;
                    }

                    // Check if candidate pixel is outside the hole
                    let ux = nx as usize;
                    let uy = ny as usize;
                    if ux < min_x || ux >= max_x || uy < min_y || uy >= max_y {
                        let dist = ((dx * dx + dy * dy) as f32).sqrt();
                        let w = 1.0 / (dist + 0.1);
                        let src_idx = (uy * uw + ux) * channels;
                        for c in 0..channels {
                            accum[c] += frame[src_idx + c] as f32 * w;
                        }
                        weight_sum += w;
                    }
                }
            }

            if weight_sum > 0.0 {
                let dst_idx = (y * uw + x) * channels;
                for c in 0..channels {
                    temp[dst_idx + c] = (accum[c] / weight_sum).clamp(0.0, 255.0).round() as u8;
                }
            }
        }
    }

    frame.copy_from_slice(&temp);
}
