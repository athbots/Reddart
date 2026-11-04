//! Frame Interpolation & Slow-Motion Blending.
//!
//! Section W4.1 of the Technical Proposal:
//! Temporal frame blending and motion interpolation for silky smooth slow-motion
//! playback (e.g. 0.25x or 0.5x) on desktop and mobile GPUs.

use anyhow::Result;

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
}
