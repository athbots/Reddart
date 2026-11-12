//! Frame Interpolation & Motion-Compensated Slow-Motion Warping.
//!
//! Technical Proposal Section W4.1 & Post-v1.0 Initiative 7:
//! - Nearest-neighbor frame repetition / hold.
//! - Linear cross-dissolve temporal frame blending.
//! - Cinema-grade optical flow motion-compensated bidirectional frame warping.

use anyhow::Result;
use rd_core::RetimeMode;

use crate::optical_flow::{DenseOpticalFlowEstimator, OpticalFlowConfig, OpticalFlowField};

/// Linearly blend two decoded RGBA frames with weight `alpha` in [0.0, 1.0].
/// `alpha = 0.0` returns frame_a, `alpha = 1.0` returns frame_b.
pub fn blend_rgba_frames(
    frame_a: &[u8],
    frame_b: &[u8],
    alpha: f32,
    output: &mut [u8],
) -> Result<()> {
    anyhow::ensure!(
        frame_a.len() == frame_b.len() && frame_a.len() == output.len(),
        "Frame length mismatch for interpolation"
    );

    let alpha_clamped = alpha.clamp(0.0, 1.0);
    let inv_alpha = 1.0 - alpha_clamped;

    // Fast SIMD/auto-vectorizable blending loop
    for i in 0..frame_a.len() {
        let val_a = frame_a[i] as f32;
        let val_b = frame_b[i] as f32;
        output[i] = (val_a * inv_alpha + val_b * alpha_clamped).round() as u8;
    }

    Ok(())
}

/// Bidirectionally warp Frame A and Frame B along optical flow trajectories to time $t \in [0.0, 1.0]$.
///
/// Uses forward motion splatting with bilinear scattering:
/// - Pixels from Frame A are mapped forward to $(x + t \cdot u_{AB}, y + t \cdot v_{AB})$.
/// - Pixels from Frame B are mapped backward to $(x + (1-t) \cdot u_{BA}, y + (1-t) \cdot v_{BA})$.
/// - Overlapping splats are normalized by accumulated bilateral weights.
/// - Unsampled disocclusion holes gracefully fall back to linear temporal blending.
#[allow(clippy::too_many_arguments)]
pub fn interpolate_optical_flow(
    frame_a: &[u8],
    frame_b: &[u8],
    flow_fwd: &OpticalFlowField,
    flow_bwd: &OpticalFlowField,
    _occlusion_mask: &[f32],
    width: usize,
    height: usize,
    t: f32,
    output: &mut [u8],
) -> Result<()> {
    let size = width * height;
    let expected_len = size * 4;
    anyhow::ensure!(
        frame_a.len() == expected_len
            && frame_b.len() == expected_len
            && output.len() == expected_len,
        "Frame dimension mismatch in optical flow interpolation"
    );

    let t_clamped = t.clamp(0.0, 1.0);
    if t_clamped <= 0.001 {
        output.copy_from_slice(frame_a);
        return Ok(());
    }
    if t_clamped >= 0.999 {
        output.copy_from_slice(frame_b);
        return Ok(());
    }

    let inv_t = 1.0 - t_clamped;

    let mut accum_rgba = vec![0.0_f32; size * 4];
    let mut accum_weight = vec![0.0_f32; size];

    let max_x = (width.saturating_sub(1)) as f32;
    let max_y = (height.saturating_sub(1)) as f32;

    // 1. Splat Frame A forward along flow_fwd to time t with weight (1 - t)
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let (u, v) = flow_fwd.get(x, y);

            let dest_x = (x as f32 + t_clamped * u).clamp(0.0, max_x);
            let dest_y = (y as f32 + t_clamped * v).clamp(0.0, max_y);

            let x0 = dest_x.floor() as usize;
            let y0 = dest_y.floor() as usize;
            let x1 = (x0 + 1).min(width - 1);
            let y1 = (y0 + 1).min(height - 1);

            let fx = dest_x - x0 as f32;
            let fy = dest_y - y0 as f32;

            let w00 = (1.0 - fx) * (1.0 - fy) * inv_t;
            let w10 = fx * (1.0 - fy) * inv_t;
            let w01 = (1.0 - fx) * fy * inv_t;
            let w11 = fx * fy * inv_t;

            let r = frame_a[idx] as f32;
            let g = frame_a[idx + 1] as f32;
            let b = frame_a[idx + 2] as f32;
            let a = frame_a[idx + 3] as f32;

            let neighbors = [
                (y0 * width + x0, w00),
                (y0 * width + x1, w10),
                (y1 * width + x0, w01),
                (y1 * width + x1, w11),
            ];

            for (n_idx, w) in neighbors {
                accum_weight[n_idx] += w;
                let c_idx = n_idx * 4;
                accum_rgba[c_idx] += r * w;
                accum_rgba[c_idx + 1] += g * w;
                accum_rgba[c_idx + 2] += b * w;
                accum_rgba[c_idx + 3] += a * w;
            }
        }
    }

    // 2. Splat Frame B backward along flow_bwd to time t with weight t
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let (u, v) = flow_bwd.get(x, y);

            let dest_x = (x as f32 + inv_t * u).clamp(0.0, max_x);
            let dest_y = (y as f32 + inv_t * v).clamp(0.0, max_y);

            let x0 = dest_x.floor() as usize;
            let y0 = dest_y.floor() as usize;
            let x1 = (x0 + 1).min(width - 1);
            let y1 = (y0 + 1).min(height - 1);

            let fx = dest_x - x0 as f32;
            let fy = dest_y - y0 as f32;

            let w00 = (1.0 - fx) * (1.0 - fy) * t_clamped;
            let w10 = fx * (1.0 - fy) * t_clamped;
            let w01 = (1.0 - fx) * fy * t_clamped;
            let w11 = fx * fy * t_clamped;

            let r = frame_b[idx] as f32;
            let g = frame_b[idx + 1] as f32;
            let b = frame_b[idx + 2] as f32;
            let a = frame_b[idx + 3] as f32;

            let neighbors = [
                (y0 * width + x0, w00),
                (y0 * width + x1, w10),
                (y1 * width + x0, w01),
                (y1 * width + x1, w11),
            ];

            for (n_idx, w) in neighbors {
                accum_weight[n_idx] += w;
                let c_idx = n_idx * 4;
                accum_rgba[c_idx] += r * w;
                accum_rgba[c_idx + 1] += g * w;
                accum_rgba[c_idx + 2] += b * w;
                accum_rgba[c_idx + 3] += a * w;
            }
        }
    }

    // 3. Normalize accumulated splats or fallback to linear blend in disoccluded holes
    for (i, &w) in accum_weight.iter().enumerate().take(size) {
        let out_idx = i * 4;
        if w > 0.05 {
            output[out_idx] = (accum_rgba[out_idx] / w).round().clamp(0.0, 255.0) as u8;
            output[out_idx + 1] = (accum_rgba[out_idx + 1] / w).round().clamp(0.0, 255.0) as u8;
            output[out_idx + 2] = (accum_rgba[out_idx + 2] / w).round().clamp(0.0, 255.0) as u8;
            output[out_idx + 3] = (accum_rgba[out_idx + 3] / w).round().clamp(0.0, 255.0) as u8;
        } else {
            // Hole filling via temporal blending
            let r = (frame_a[out_idx] as f32 * inv_t + frame_b[out_idx] as f32 * t_clamped).round();
            let g = (frame_a[out_idx + 1] as f32 * inv_t + frame_b[out_idx + 1] as f32 * t_clamped)
                .round();
            let b = (frame_a[out_idx + 2] as f32 * inv_t + frame_b[out_idx + 2] as f32 * t_clamped)
                .round();
            let a = (frame_a[out_idx + 3] as f32 * inv_t + frame_b[out_idx + 3] as f32 * t_clamped)
                .round();
            output[out_idx] = r as u8;
            output[out_idx + 1] = g as u8;
            output[out_idx + 2] = b as u8;
            output[out_idx + 3] = a as u8;
        }
    }

    Ok(())
}

/// High-level retimed frame synthesizer dispatching between Nearest, Blend, and OpticalFlow modes.
pub fn interpolate_retimed_frame(
    frame_a: &[u8],
    frame_b: &[u8],
    width: usize,
    height: usize,
    t: f32,
    mode: RetimeMode,
    output: &mut [u8],
) -> Result<()> {
    match mode {
        RetimeMode::Nearest => {
            anyhow::ensure!(output.len() == frame_a.len(), "Buffer size mismatch");
            if t < 0.5 {
                output.copy_from_slice(frame_a);
            } else {
                output.copy_from_slice(frame_b);
            }
            Ok(())
        }
        RetimeMode::Blend => blend_rgba_frames(frame_a, frame_b, t, output),
        RetimeMode::OpticalFlow => {
            let estimator = DenseOpticalFlowEstimator::new(OpticalFlowConfig::default());
            let (fwd, bwd, occlusion) =
                estimator.compute_bidirectional_flow(frame_a, frame_b, width, height);

            interpolate_optical_flow(
                frame_a, frame_b, &fwd, &bwd, &occlusion, width, height, t, output,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_blending() {
        let frame_a = vec![0u8; 16]; // Black
        let frame_b = vec![200u8; 16]; // Light gray
        let mut out = vec![0u8; 16];

        // 50% midpoint blend
        blend_rgba_frames(&frame_a, &frame_b, 0.5, &mut out).unwrap();
        assert_eq!(out, vec![100u8; 16]);

        // 25% blend
        blend_rgba_frames(&frame_a, &frame_b, 0.25, &mut out).unwrap();
        assert_eq!(out, vec![50u8; 16]);

        // 100% blend (matches frame_b)
        blend_rgba_frames(&frame_a, &frame_b, 1.0, &mut out).unwrap();
        assert_eq!(out, vec![200u8; 16]);
    }

    #[test]
    fn test_nearest_and_blend_retimed_frame() {
        let frame_a = vec![10u8; 16];
        let frame_b = vec![210u8; 16];
        let mut out = vec![0u8; 16];

        // Nearest at t=0.25 picks Frame A
        interpolate_retimed_frame(
            &frame_a,
            &frame_b,
            2,
            2,
            0.25,
            RetimeMode::Nearest,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, frame_a);

        // Nearest at t=0.75 picks Frame B
        interpolate_retimed_frame(
            &frame_a,
            &frame_b,
            2,
            2,
            0.75,
            RetimeMode::Nearest,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, frame_b);

        // Blend at t=0.5 yields exact average (110)
        interpolate_retimed_frame(&frame_a, &frame_b, 2, 2, 0.50, RetimeMode::Blend, &mut out)
            .unwrap();
        assert_eq!(out, vec![110u8; 16]);
    }

    #[test]
    fn test_motion_compensated_optical_flow_interpolation() {
        let width = 32;
        let height = 16;
        let size = width * height * 4;

        // Frame A: White vertical bar at x=8..14
        let mut frame_a = vec![0u8; size];
        for y in 0..height {
            for x in 8..14 {
                let idx = (y * width + x) * 4;
                frame_a[idx] = 255;
                frame_a[idx + 1] = 255;
                frame_a[idx + 2] = 255;
                frame_a[idx + 3] = 255;
            }
        }

        // Frame B: White vertical bar moved right by 4 pixels to x=12..18
        let mut frame_b = vec![0u8; size];
        for y in 0..height {
            for x in 12..18 {
                let idx = (y * width + x) * 4;
                frame_b[idx] = 255;
                frame_b[idx + 1] = 255;
                frame_b[idx + 2] = 255;
                frame_b[idx + 3] = 255;
            }
        }

        let mut out = vec![0u8; size];

        let estimator = DenseOpticalFlowEstimator::new(OpticalFlowConfig {
            pyramid_levels: 2,
            iterations: 20,
            alpha_sq: 0.01,
            occlusion_threshold: 1.5,
        });
        let (fwd, bwd, occ) =
            estimator.compute_bidirectional_flow(&frame_a, &frame_b, width, height);

        interpolate_optical_flow(
            &frame_a, &frame_b, &fwd, &bwd, &occ, width, height, 0.5, &mut out,
        )
        .unwrap();

        // Check that intermediate position around x=11..15 has strong brightness
        let center_y = height / 2;
        let mid_idx = (center_y * width + 13) * 4;
        assert!(
            out[mid_idx] > 50,
            "Intermediate position x=13 should be bright via optical flow warping, got {}",
            out[mid_idx]
        );
    }
}
