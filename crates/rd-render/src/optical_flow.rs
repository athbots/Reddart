//! Dense Optical Flow Estimation for Motion-Compensated Retiming & Slow Motion.
//!
//! Technical Proposal Section W4.1 & Post-v1.0 Initiative 7:
//! - Multi-scale hierarchical pyramidal optical flow solver (Coarse-to-fine Horn-Schunck with Sobel gradients).
//! - Forward flow field (Frame A -> Frame B) and Backward flow field (Frame B -> Frame A).
//! - Bidirectional forward-backward consistency checking for occlusion and boundary discontinuity detection.

use serde::{Deserialize, Serialize};

/// Dense 2D motion vector field representing per-pixel translation vectors $(\Delta x, \Delta y)$.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalFlowField {
    pub width: usize,
    pub height: usize,
    /// Horizontal pixel displacements $u = \Delta x$.
    pub u: Vec<f32>,
    /// Vertical pixel displacements $v = \Delta y$.
    pub v: Vec<f32>,
}

impl OpticalFlowField {
    /// Create a zero-initialized motion field of dimensions `width x height`.
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            u: vec![0.0; size],
            v: vec![0.0; size],
        }
    }

    /// Retrieve displacement vector at integer coordinates $(x, y)$.
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> (f32, f32) {
        let cx = x.min(self.width.saturating_sub(1));
        let cy = y.min(self.height.saturating_sub(1));
        let idx = cy * self.width + cx;
        (self.u[idx], self.v[idx])
    }

    /// Set displacement vector at integer coordinates $(x, y)$.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, u: f32, v: f32) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.u[idx] = u;
            self.v[idx] = v;
        }
    }

    /// Sub-pixel bilinear sampling of displacement vectors.
    pub fn sample_bilinear(&self, x: f32, y: f32) -> (f32, f32) {
        let max_x = (self.width.saturating_sub(1)) as f32;
        let max_y = (self.height.saturating_sub(1)) as f32;

        let cx = x.clamp(0.0, max_x);
        let cy = y.clamp(0.0, max_y);

        let x0 = cx.floor() as usize;
        let y0 = cy.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let fx = cx - x0 as f32;
        let fy = cy - y0 as f32;

        let (u00, v00) = self.get(x0, y0);
        let (u10, v10) = self.get(x1, y0);
        let (u01, v01) = self.get(x0, y1);
        let (u11, v11) = self.get(x1, y1);

        let u_top = u00 * (1.0 - fx) + u10 * fx;
        let u_bot = u01 * (1.0 - fx) + u11 * fx;
        let u_interp = u_top * (1.0 - fy) + u_bot * fy;

        let v_top = v00 * (1.0 - fx) + v10 * fx;
        let v_bot = v01 * (1.0 - fx) + v11 * fx;
        let v_interp = v_top * (1.0 - fy) + v_bot * fy;

        (u_interp, v_interp)
    }

    /// Compute motion vector magnitude $\|\vec{v}\| = \sqrt{u^2 + v^2}$ at $(x, y)$.
    pub fn magnitude(&self, x: usize, y: usize) -> f32 {
        let (u, v) = self.get(x, y);
        (u * u + v * v).sqrt()
    }

    /// Calculate average displacement magnitude across the entire frame.
    pub fn mean_magnitude(&self) -> f32 {
        if self.u.is_empty() {
            return 0.0;
        }
        let sum: f32 = self
            .u
            .iter()
            .zip(self.v.iter())
            .map(|(&u, &v)| (u * u + v * v).sqrt())
            .sum();
        sum / self.u.len() as f32
    }

    /// Upsample motion field by a factor of 2x using bilinear interpolation with displacement doubling.
    pub fn upsample_2x(&self, dst_w: usize, dst_h: usize) -> Self {
        let mut up = Self::new(dst_w, dst_h);
        let scale_x = self.width as f32 / dst_w as f32;
        let scale_y = self.height as f32 / dst_h as f32;

        let up_scale_x = dst_w as f32 / self.width as f32;
        let up_scale_y = dst_h as f32 / self.height as f32;

        for dy in 0..dst_h {
            let sy = (dy as f32 + 0.5) * scale_y - 0.5;
            for dx in 0..dst_w {
                let sx = (dx as f32 + 0.5) * scale_x - 0.5;
                let (u, v) = self.sample_bilinear(sx, sy);
                up.set(dx, dy, u * up_scale_x, v * up_scale_y);
            }
        }
        up
    }
}

/// Configuration parameters for the multi-scale dense optical flow estimator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalFlowConfig {
    /// Number of image pyramid levels (default: 3).
    pub pyramid_levels: usize,
    /// Number of relaxation iterations per pyramid level (default: 10).
    pub iterations: usize,
    /// Smoothness regularization weight $\alpha^2$ (default: 0.02).
    pub alpha_sq: f32,
    /// Forward-backward consistency threshold in pixels for occlusion detection (default: 1.5).
    pub occlusion_threshold: f32,
}

impl Default for OpticalFlowConfig {
    fn default() -> Self {
        Self {
            pyramid_levels: 3,
            iterations: 10,
            alpha_sq: 0.02,
            occlusion_threshold: 1.5,
        }
    }
}

/// Cinema-grade Dense Optical Flow Estimator.
pub struct DenseOpticalFlowEstimator {
    pub config: OpticalFlowConfig,
}

impl DenseOpticalFlowEstimator {
    pub fn new(config: OpticalFlowConfig) -> Self {
        Self { config }
    }

    /// Estimate forward and backward flow fields between Frame A and Frame B.
    ///
    /// Returns `(flow_forward, flow_backward, occlusion_mask)` where:
    /// - `flow_forward` maps Frame A -> Frame B.
    /// - `flow_backward` maps Frame B -> Frame A.
    /// - `occlusion_mask` is a `Vec<f32>` in `[0.0, 1.0]` (1.0 = occluded/disoccluded, 0.0 = reliable match).
    pub fn compute_bidirectional_flow(
        &self,
        frame_a: &[u8],
        frame_b: &[u8],
        width: usize,
        height: usize,
    ) -> (OpticalFlowField, OpticalFlowField, Vec<f32>) {
        let lum_a = rgba_to_luminance(frame_a, width, height);
        let lum_b = rgba_to_luminance(frame_b, width, height);

        let flow_forward = self.compute_flow_pyramid(&lum_a, &lum_b, width, height);
        let flow_backward = self.compute_flow_pyramid(&lum_b, &lum_a, width, height);

        let occlusion_mask =
            self.compute_occlusion_mask(&flow_forward, &flow_backward, width, height);

        (flow_forward, flow_backward, occlusion_mask)
    }

    /// Hierarchical coarse-to-fine optical flow calculation using image pyramids.
    fn compute_flow_pyramid(
        &self,
        lum_a: &[f32],
        lum_b: &[f32],
        width: usize,
        height: usize,
    ) -> OpticalFlowField {
        let levels = self.config.pyramid_levels.max(1);

        // Build pyramids from finest (0) to coarsest (levels - 1)
        let mut pyr_a = Vec::with_capacity(levels);
        let mut pyr_b = Vec::with_capacity(levels);
        let mut dims = Vec::with_capacity(levels);

        pyr_a.push(lum_a.to_vec());
        pyr_b.push(lum_b.to_vec());
        dims.push((width, height));

        for l in 1..levels {
            let (prev_w, prev_h) = dims[l - 1];
            let next_w = (prev_w / 2).max(1);
            let next_h = (prev_h / 2).max(1);
            let down_a = downsample_2x(&pyr_a[l - 1], prev_w, prev_h, next_w, next_h);
            let down_b = downsample_2x(&pyr_b[l - 1], prev_w, prev_h, next_w, next_h);
            pyr_a.push(down_a);
            pyr_b.push(down_b);
            dims.push((next_w, next_h));
        }

        // Initialize coarsest flow field
        let (coarsest_w, coarsest_h) = dims[levels - 1];
        let mut current_flow = OpticalFlowField::new(coarsest_w, coarsest_h);

        // Coarse-to-fine relaxation
        for level in (0..levels).rev() {
            let (lvl_w, lvl_h) = dims[level];
            if level < levels - 1 {
                // Upsample previous coarser level
                current_flow = current_flow.upsample_2x(lvl_w, lvl_h);
            }

            self.solve_level(
                &pyr_a[level],
                &pyr_b[level],
                lvl_w,
                lvl_h,
                &mut current_flow,
            );
        }

        current_flow
    }

    /// Solve optical flow relaxation at a single scale level with warped residuals.
    fn solve_level(
        &self,
        img_a: &[f32],
        img_b: &[f32],
        width: usize,
        height: usize,
        flow: &mut OpticalFlowField,
    ) {
        let size = width * height;
        if size == 0 {
            return;
        }

        let u_init = flow.u.clone();
        let v_init = flow.v.clone();

        let mut du = vec![0.0_f32; size];
        let mut dv = vec![0.0_f32; size];

        // 1. Compute spatial gradients on Frame A
        let mut grad_x = vec![0.0_f32; size];
        let mut grad_y = vec![0.0_f32; size];

        for y in 1..height.saturating_sub(1) {
            for x in 1..width.saturating_sub(1) {
                let idx = y * width + x;
                let p00 = img_a[(y - 1) * width + (x - 1)];
                let p20 = img_a[(y - 1) * width + (x + 1)];
                let p01 = img_a[y * width + (x - 1)];
                let p21 = img_a[y * width + (x + 1)];
                let p02 = img_a[(y + 1) * width + (x - 1)];
                let p22 = img_a[(y + 1) * width + (x + 1)];

                let p10 = img_a[(y - 1) * width + x];
                let p12 = img_a[(y + 1) * width + x];

                grad_x[idx] = ((p20 + 2.0 * p21 + p22) - (p00 + 2.0 * p01 + p02)) / 8.0;
                grad_y[idx] = ((p02 + 2.0 * p12 + p22) - (p00 + 2.0 * p10 + p20)) / 8.0;
            }
        }

        // 2. Compute warped temporal difference It = Warped_B(x + u_init, y + v_init) - A(x, y)
        let mut it = vec![0.0_f32; size];
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let wx = x as f32 + u_init[idx];
                let wy = y as f32 + v_init[idx];
                let b_sample = sample_luminance_bilinear(img_b, width, height, wx, wy);
                it[idx] = b_sample - img_a[idx];
            }
        }

        let alpha_sq = self.config.alpha_sq;

        // 3. Iterative Horn-Schunck relaxation on residual flow
        for _ in 0..self.config.iterations {
            let du_old = du.clone();
            let dv_old = dv.clone();

            for y in 1..height.saturating_sub(1) {
                for x in 1..width.saturating_sub(1) {
                    let idx = y * width + x;

                    let du_avg = (du_old[idx - 1]
                        + du_old[idx + 1]
                        + du_old[idx - width]
                        + du_old[idx + width])
                        * 0.25;
                    let dv_avg = (dv_old[idx - 1]
                        + dv_old[idx + 1]
                        + dv_old[idx - width]
                        + dv_old[idx + width])
                        * 0.25;

                    let ix = grad_x[idx];
                    let iy = grad_y[idx];

                    let denom = alpha_sq + ix * ix + iy * iy;
                    let delta = (ix * du_avg + iy * dv_avg + it[idx]) / denom;

                    du[idx] = du_avg - ix * delta;
                    dv[idx] = dv_avg - iy * delta;
                }
            }
        }

        // Add residual to total flow
        for i in 0..size {
            flow.u[i] = u_init[i] + du[i];
            flow.v[i] = v_init[i] + dv[i];
        }
    }

    /// Forward-backward consistency check for motion occlusion detection.
    fn compute_occlusion_mask(
        &self,
        forward: &OpticalFlowField,
        backward: &OpticalFlowField,
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        let mut mask = vec![0.0_f32; width * height];
        let thresh = self.config.occlusion_threshold.max(0.1);

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let (u_fwd, v_fwd) = forward.get(x, y);

                // Target coordinate in Frame B
                let bx = x as f32 + u_fwd;
                let by = y as f32 + v_fwd;

                // Sample backward flow at destination
                let (u_bwd, v_bwd) = backward.sample_bilinear(bx, by);

                // Consistency roundtrip error
                let err_x = u_fwd + u_bwd;
                let err_y = v_fwd + v_bwd;
                let err = (err_x * err_x + err_y * err_y).sqrt();

                if err > thresh {
                    let confidence = ((err - thresh) / thresh).clamp(0.0, 1.0);
                    mask[idx] = confidence;
                }
            }
        }

        mask
    }
}

/// Convert RGBA byte slice to normalized single-channel luminance [0.0, 1.0].
fn rgba_to_luminance(rgba: &[u8], width: usize, height: usize) -> Vec<f32> {
    let mut lum = vec![0.0_f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            if idx + 2 < rgba.len() {
                let r = rgba[idx] as f32;
                let g = rgba[idx + 1] as f32;
                let b = rgba[idx + 2] as f32;
                lum[y * width + x] = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
            }
        }
    }
    lum
}

/// Bilinear sampling of luminance buffer with edge clamping.
fn sample_luminance_bilinear(img: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    let max_x = (width.saturating_sub(1)) as f32;
    let max_y = (height.saturating_sub(1)) as f32;

    let cx = x.clamp(0.0, max_x);
    let cy = y.clamp(0.0, max_y);

    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);

    let fx = cx - x0 as f32;
    let fy = cy - y0 as f32;

    let p00 = img[y0 * width + x0];
    let p10 = img[y0 * width + x1];
    let p01 = img[y1 * width + x0];
    let p11 = img[y1 * width + x1];

    let top = p00 * (1.0 - fx) + p10 * fx;
    let bot = p01 * (1.0 - fx) + p11 * fx;
    top * (1.0 - fy) + bot * fy
}

/// Downscale single-channel luminance image by 2x using box filtering.
fn downsample_2x(src: &[f32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<f32> {
    let mut dst = vec![0.0_f32; dst_w * dst_h];
    for dy in 0..dst_h {
        let sy0 = (dy * 2).min(src_h - 1);
        let sy1 = (dy * 2 + 1).min(src_h - 1);
        for dx in 0..dst_w {
            let sx0 = (dx * 2).min(src_w - 1);
            let sx1 = (dx * 2 + 1).min(src_w - 1);

            let val = (src[sy0 * src_w + sx0]
                + src[sy0 * src_w + sx1]
                + src[sy1 * src_w + sx0]
                + src[sy1 * src_w + sx1])
                * 0.25;
            dst[dy * dst_w + dx] = val;
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optical_flow_field_sampling_and_upsample() {
        let mut flow = OpticalFlowField::new(4, 4);
        flow.set(1, 1, 4.0, 2.0);
        assert_eq!(flow.get(1, 1), (4.0, 2.0));

        let sampled = flow.sample_bilinear(1.0, 1.0);
        assert_eq!(sampled, (4.0, 2.0));

        let up = flow.upsample_2x(8, 8);
        assert_eq!(up.width, 8);
        assert_eq!(up.height, 8);
        assert!(up.mean_magnitude() > 0.0);
    }

    #[test]
    fn test_dense_optical_flow_on_horizontal_motion() {
        let width = 64;
        let height = 32;

        // Create Frame A with a bright block at x=10..20
        let mut frame_a = vec![30u8; width * height * 4];
        for y in 8..24 {
            for x in 10..20 {
                let idx = (y * width + x) * 4;
                frame_a[idx] = 220;
                frame_a[idx + 1] = 220;
                frame_a[idx + 2] = 220;
                frame_a[idx + 3] = 255;
            }
        }

        // Create Frame B with the bright block shifted right by 4 pixels to x=14..24
        let mut frame_b = vec![30u8; width * height * 4];
        for y in 8..24 {
            for x in 14..24 {
                let idx = (y * width + x) * 4;
                frame_b[idx] = 220;
                frame_b[idx + 1] = 220;
                frame_b[idx + 2] = 220;
                frame_b[idx + 3] = 255;
            }
        }

        let estimator = DenseOpticalFlowEstimator::new(OpticalFlowConfig {
            pyramid_levels: 3,
            iterations: 15,
            alpha_sq: 0.015,
            occlusion_threshold: 1.5,
        });

        let (fwd, bwd, occlusion) =
            estimator.compute_bidirectional_flow(&frame_a, &frame_b, width, height);

        assert_eq!(fwd.width, width);
        assert_eq!(bwd.width, width);
        assert_eq!(occlusion.len(), width * height);

        // Forward flow inside the moving block should have positive horizontal velocity (moving right)
        let block_center_u = fwd.get(15, 16).0;
        assert!(
            block_center_u > 0.5,
            "Expected rightward flow > 0.5 px, got {}",
            block_center_u
        );

        // Backward flow inside the moved block should have negative horizontal velocity (moving left)
        let bwd_center_u = bwd.get(19, 16).0;
        assert!(
            bwd_center_u < -0.5,
            "Expected leftward backward flow < -0.5 px, got {}",
            bwd_center_u
        );
    }
}
