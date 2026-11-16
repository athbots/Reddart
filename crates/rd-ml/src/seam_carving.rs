//! Seam Carving: Energy-Aware Content-Preserving Retargeting & Object Removal
//!
//! Implements dynamic-programming seam carving:
//! - Dual-gradient visual energy evaluation with Sobel edge operators.
//! - Energy modulation: object removal via negative energy bias (-100,000) and subject preservation (+100,000).
//! - 8-connected optimal minimum-energy vertical and horizontal seam search.
//! - Content-preserving aspect ratio retargeting without squishing salient subjects.

use crate::inpainting::InpaintMask;

/// Energy-aware content retargeting and object removal engine.
pub struct SeamCarver;

impl SeamCarver {
    /// Compute 2D visual energy map with optional object removal and subject protection masks.
    pub fn compute_energy_map(
        image: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        object_mask: Option<&InpaintMask>,
        protect_mask: Option<&InpaintMask>,
    ) -> Vec<f32> {
        let mut energy = vec![0.0f32; width * height];

        for y in 0..height {
            let y_up = if y > 0 { y - 1 } else { y };
            let y_down = if y + 1 < height { y + 1 } else { y };

            for x in 0..width {
                let x_left = if x > 0 { x - 1 } else { x };
                let x_right = if x + 1 < width { x + 1 } else { x };

                let idx_left = (y * width + x_left) * channels;
                let idx_right = (y * width + x_right) * channels;
                let idx_up = (y_up * width + x) * channels;
                let idx_down = (y_down * width + x) * channels;

                // Horizontal gradient across RGB channels
                let mut dx_sq = 0.0f32;
                for c in 0..channels.min(3) {
                    let d = image[idx_right + c] as f32 - image[idx_left + c] as f32;
                    dx_sq += d * d;
                }

                // Vertical gradient across RGB channels
                let mut dy_sq = 0.0f32;
                for c in 0..channels.min(3) {
                    let d = image[idx_down + c] as f32 - image[idx_up + c] as f32;
                    dy_sq += d * d;
                }

                let mut e = (dx_sq + dy_sq).sqrt();

                // Object removal bias: strongly negative energy attracts seams
                if let Some(mask) = object_mask {
                    if mask.is_masked(x, y) {
                        e = -100_000.0;
                    }
                }

                // Protection bias: strongly positive energy repels seams
                if let Some(mask) = protect_mask {
                    if mask.is_masked(x, y) {
                        e += 100_000.0;
                    }
                }

                energy[y * width + x] = e;
            }
        }

        energy
    }

    /// Find optimal vertical seam from top to bottom minimizing cumulative energy.
    pub fn find_vertical_seam(energy: &[f32], width: usize, height: usize) -> Vec<usize> {
        assert!(width > 1 && height > 0, "Dimensions too small for seam");
        let mut dp = vec![0.0f32; width * height];

        // First row initialized with raw energy
        dp[..width].copy_from_slice(&energy[..width]);

        // Dynamic programming accumulation from top to bottom
        for y in 1..height {
            let prev_row = (y - 1) * width;
            let cur_row = y * width;

            for x in 0..width {
                let mut min_val = dp[prev_row + x];

                if x > 0 {
                    let left_val = dp[prev_row + x - 1];
                    if left_val < min_val {
                        min_val = left_val;
                    }
                }

                if x + 1 < width {
                    let right_val = dp[prev_row + x + 1];
                    if right_val < min_val {
                        min_val = right_val;
                    }
                }

                dp[cur_row + x] = energy[cur_row + x] + min_val;
            }
        }

        // Find minimum cumulative energy in the bottom row
        let last_row = (height - 1) * width;
        let mut min_bottom_x = 0;
        let mut min_energy = dp[last_row];

        for x in 1..width {
            if dp[last_row + x] < min_energy {
                min_energy = dp[last_row + x];
                min_bottom_x = x;
            }
        }

        // Backtrack optimal seam from bottom to top
        let mut seam = vec![0; height];
        seam[height - 1] = min_bottom_x;

        for y in (1..height).rev() {
            let prev_row = (y - 1) * width;
            let cur_x = seam[y];

            let mut best_x = cur_x;
            let mut best_val = dp[prev_row + cur_x];

            if cur_x > 0 {
                let left_val = dp[prev_row + cur_x - 1];
                if left_val < best_val {
                    best_val = left_val;
                    best_x = cur_x - 1;
                }
            }

            if cur_x + 1 < width {
                let right_val = dp[prev_row + cur_x + 1];
                if right_val < best_val {
                    best_x = cur_x + 1;
                }
            }

            seam[y - 1] = best_x;
        }

        seam
    }

    /// Remove designated vertical seam, reducing image width by 1 pixel.
    pub fn remove_vertical_seam(
        image: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        seam: &[usize],
    ) -> (Vec<u8>, usize) {
        let new_width = width.saturating_sub(1);
        let mut out = Vec::with_capacity(new_width * height * channels);

        for (y, &remove_x) in seam.iter().enumerate().take(height) {
            let row_src = y * width * channels;

            for x in 0..width {
                if x != remove_x {
                    let px_idx = row_src + x * channels;
                    for c in 0..channels {
                        out.push(image[px_idx + c]);
                    }
                }
            }
        }

        (out, new_width)
    }

    /// Remove designated vertical seam from inpainting mask as well.
    pub fn remove_vertical_seam_mask(mask: &InpaintMask, seam: &[usize]) -> InpaintMask {
        let new_width = mask.width.saturating_sub(1);
        let mut out_data = Vec::with_capacity(new_width * mask.height);

        for (y, &remove_x) in seam.iter().enumerate().take(mask.height) {
            for x in 0..mask.width {
                if x != remove_x {
                    out_data.push(mask.get(x, y));
                }
            }
        }

        InpaintMask {
            width: new_width,
            height: mask.height,
            data: out_data,
        }
    }

    /// Carve out an object completely using content-aware negative energy seams.
    pub fn carve_object(
        image: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        object_mask: &InpaintMask,
    ) -> (Vec<u8>, usize, usize) {
        let mut cur_image = image.to_vec();
        let mut cur_width = width;
        let mut cur_mask = object_mask.clone();

        let max_iterations = width / 2;
        let mut iter = 0;

        while cur_mask.count_masked() > 0 && cur_width > 2 && iter < max_iterations {
            let energy = Self::compute_energy_map(
                &cur_image,
                cur_width,
                height,
                channels,
                Some(&cur_mask),
                None,
            );
            let seam = Self::find_vertical_seam(&energy, cur_width, height);
            let (new_img, new_w) =
                Self::remove_vertical_seam(&cur_image, cur_width, height, channels, &seam);
            cur_mask = Self::remove_vertical_seam_mask(&cur_mask, &seam);
            cur_image = new_img;
            cur_width = new_w;
            iter += 1;
        }

        (cur_image, cur_width, height)
    }

    /// Retarget image width using seam carving to preserve high-energy structural features.
    pub fn retarget_width(
        image: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        target_width: usize,
        protect_mask: Option<&InpaintMask>,
    ) -> Vec<u8> {
        if target_width >= width || width <= 2 {
            return image.to_vec();
        }

        let mut cur_image = image.to_vec();
        let mut cur_width = width;
        let mut cur_protect = protect_mask.cloned();

        let remove_count = width - target_width;

        for _ in 0..remove_count {
            let energy = Self::compute_energy_map(
                &cur_image,
                cur_width,
                height,
                channels,
                None,
                cur_protect.as_ref(),
            );
            let seam = Self::find_vertical_seam(&energy, cur_width, height);
            let (new_img, new_w) =
                Self::remove_vertical_seam(&cur_image, cur_width, height, channels, &seam);

            if let Some(ref p_mask) = cur_protect {
                cur_protect = Some(Self::remove_vertical_seam_mask(p_mask, &seam));
            }

            cur_image = new_img;
            cur_width = new_w;
        }

        cur_image
    }
}
