//! Spatial Visual Saliency & Subject Detection
//!
//! Evaluates multi-cue spatial visual saliency maps combining structural gradient energy,
//! chrominance/skin-locus distinctness, and kinematic center-bias to locate the primary
//! subject of interest in video frames without heavy external neural runtimes.

use crate::tracking::BoundingBox;

/// 2D Normalized Saliency Map with values in [0.0, 1.0].
#[derive(Debug, Clone)]
pub struct SaliencyMap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl SaliencyMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; width * height],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.data[y * self.width + x]
        } else {
            0.0
        }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, val: f32) {
        if x < self.width && y < self.height {
            self.data[y * self.width + x] = val;
        }
    }
}

/// Visual Saliency Detector configuration.
#[derive(Debug, Clone)]
pub struct SaliencyConfig {
    /// Weight of structural luminance gradient energy [0.0, 1.0].
    pub gradient_weight: f32,
    /// Weight of chromatic / skin locus distinctness [0.0, 1.0].
    pub color_weight: f32,
    /// Center bias influence [0.0, 1.0].
    pub center_bias_weight: f32,
    /// Downsample factor for high-throughput frame analysis (e.g. 4 for 4x speedup).
    pub downsample_factor: usize,
}

impl Default for SaliencyConfig {
    fn default() -> Self {
        Self {
            gradient_weight: 0.5,
            color_weight: 0.5,
            center_bias_weight: 0.35,
            downsample_factor: 2,
        }
    }
}

/// Visual Saliency Detector for video frames.
#[derive(Debug, Clone)]
pub struct SaliencyDetector {
    pub config: SaliencyConfig,
}

impl SaliencyDetector {
    pub fn new(config: SaliencyConfig) -> Self {
        Self { config }
    }

    /// Compute spatial visual saliency map from an RGB or RGBA frame buffer.
    pub fn compute_saliency_map(
        &self,
        frame: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> SaliencyMap {
        assert!(channels >= 3, "Frame must have at least 3 channels (RGB)");
        let step = self.config.downsample_factor.max(1);
        let sw = (width / step).max(1);
        let sh = (height / step).max(1);

        let mut lum = vec![0.0_f32; sw * sh];
        let mut skin_cue = vec![0.0_f32; sw * sh];

        // 1. Extract luminance and skin chromaticity cues
        for sy in 0..sh {
            let fy = (sy * step).min(height - 1);
            for sx in 0..sw {
                let fx = (sx * step).min(width - 1);
                let idx = (fy * width + fx) * channels;
                let r = frame[idx] as f32 / 255.0;
                let g = frame[idx + 1] as f32 / 255.0;
                let b = frame[idx + 2] as f32 / 255.0;

                // Luminance
                let y = 0.299 * r + 0.587 * g + 0.114 * b;
                lum[sy * sw + sx] = y;

                // Normalized YCbCr
                let cb = -0.168736 * r - 0.331264 * g + 0.5 * b + 0.5;
                let cr = 0.5 * r - 0.418688 * g - 0.081312 * b + 0.5;

                // Skin locus model (Cb in [0.32, 0.49], Cr in [0.52, 0.75])
                let is_skin = (0.32..=0.49).contains(&cb) && (0.52..=0.75).contains(&cr);
                let chroma_dist = ((cb - 0.5).powi(2) + (cr - 0.5).powi(2)).sqrt();
                if is_skin && chroma_dist > 0.035 {
                    skin_cue[sy * sw + sx] = 1.0;
                }
            }
        }

        // 2. Compute luminance gradients (Sobel / central differences)
        let mut grad_mag = vec![0.0_f32; sw * sh];
        let mut max_grad = 0.001_f32;

        for sy in 1..sh.saturating_sub(1) {
            for sx in 1..sw.saturating_sub(1) {
                let dx = lum[sy * sw + (sx + 1)] - lum[sy * sw + (sx - 1)];
                let dy = lum[(sy + 1) * sw + sx] - lum[(sy - 1) * sw + sx];
                let mag = (dx * dx + dy * dy).sqrt();
                grad_mag[sy * sw + sx] = mag;
                if mag > max_grad {
                    max_grad = mag;
                }
            }
        }

        // 3. Synthesize saliency map with center bias
        let mut saliency = SaliencyMap::new(sw, sh);
        let cx = sw as f32 * 0.5;
        let cy = sh as f32 * 0.5;
        let sigma_x = sw as f32 * 0.4;
        let sigma_y = sh as f32 * 0.4;

        for sy in 0..sh {
            let dy = (sy as f32 - cy) / sigma_y;
            for sx in 0..sw {
                let dx = (sx as f32 - cx) / sigma_x;
                let center_bias = (-0.5 * (dx * dx + dy * dy)).exp();

                let g_norm = grad_mag[sy * sw + sx] / max_grad;
                let c_norm = skin_cue[sy * sw + sx];

                let combined =
                    self.config.gradient_weight * g_norm + self.config.color_weight * c_norm;

                let bias_factor = 1.0 - self.config.center_bias_weight
                    + self.config.center_bias_weight * center_bias;

                let score = (combined * bias_factor).clamp(0.0, 1.0);
                saliency.set(sx, sy, score);
            }
        }

        saliency
    }

    /// Detect the bounding box of the most salient subject in the frame.
    pub fn detect_salient_subject(
        &self,
        frame: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> BoundingBox {
        let sm = self.compute_saliency_map(frame, width, height, channels);
        let sw = sm.width;
        let sh = sm.height;

        // Calculate center of mass and weighted spatial spread
        let mut total_weight = 0.0_f32;
        let mut weighted_x = 0.0_f32;
        let mut weighted_y = 0.0_f32;

        // Apply a high-saliency threshold to isolate foreground subjects from background noise
        let threshold = 0.25_f32;

        for sy in 0..sh {
            for sx in 0..sw {
                let score = sm.get(sx, sy);
                if score >= threshold {
                    let w = (score - threshold).powi(2);
                    total_weight += w;
                    weighted_x += sx as f32 * w;
                    weighted_y += sy as f32 * w;
                }
            }
        }

        // If no salient region found, fallback to centered safe framing
        if total_weight < 1e-4 {
            return BoundingBox {
                x: 0.35,
                y: 0.25,
                width: 0.30,
                height: 0.50,
            };
        }

        let mean_x = weighted_x / total_weight;
        let mean_y = weighted_y / total_weight;

        // Calculate standard deviation spread around center of mass
        let mut var_x = 0.0_f32;
        let mut var_y = 0.0_f32;

        for sy in 0..sh {
            for sx in 0..sw {
                let score = sm.get(sx, sy);
                if score >= threshold {
                    let w = (score - threshold).powi(2);
                    var_x += (sx as f32 - mean_x).powi(2) * w;
                    var_y += (sy as f32 - mean_y).powi(2) * w;
                }
            }
        }

        let std_x = (var_x / total_weight).sqrt();
        let std_y = (var_y / total_weight).sqrt();

        // Convert back to normalized coordinates [0.0, 1.0]
        let norm_cx = mean_x / sw as f32;
        let norm_cy = mean_y / sh as f32;

        // Bounding box spans ~2.5 standard deviations (capturing full head/torso)
        let norm_w = ((std_x * 2.8) / sw as f32).clamp(0.20, 0.70);
        let norm_h = ((std_y * 3.2) / sh as f32).clamp(0.30, 0.85);

        let norm_x = (norm_cx - norm_w * 0.5).clamp(0.0, 1.0 - norm_w);
        let norm_y = (norm_cy - norm_h * 0.5).clamp(0.0, 1.0 - norm_h);

        BoundingBox {
            x: norm_x,
            y: norm_y,
            width: norm_w,
            height: norm_h,
        }
    }

    /// Detect multiple salient subject clusters (e.g. podcast co-hosts or group discussions).
    pub fn detect_multiple_salient_subjects(
        &self,
        frame: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        max_subjects: usize,
    ) -> Vec<BoundingBox> {
        let sm = self.compute_saliency_map(frame, width, height, channels);
        let sw = sm.width;
        let sh = sm.height;

        let threshold = 0.40_f32;
        let mut peaks: Vec<(usize, usize, f32)> = Vec::new();

        // 1. Identify local maxima peaks in saliency map
        let r = 3i32;
        for sy in (r as usize)..(sh - r as usize) {
            for sx in (r as usize)..(sw - r as usize) {
                let val = sm.get(sx, sy);
                if val < threshold {
                    continue;
                }

                let mut is_max = true;
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = (sx as i32 + dx) as usize;
                        let ny = (sy as i32 + dy) as usize;
                        if sm.get(nx, ny) > val {
                            is_max = false;
                            break;
                        }
                    }
                    if !is_max {
                        break;
                    }
                }

                if is_max {
                    peaks.push((sx, sy, val));
                }
            }
        }

        // Sort peaks descending by saliency intensity
        peaks.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        let mut boxes = Vec::new();
        let min_separation_sq = (sw as f32 * 0.20).powi(2);

        for (px, py, _val) in peaks {
            if boxes.len() >= max_subjects {
                break;
            }

            let norm_px = px as f32 / sw as f32;
            let norm_py = py as f32 / sh as f32;

            // Enforce spatial separation so peaks belong to distinct subjects
            let mut separate = true;
            for b in &boxes {
                let (bx, by) = (b as &BoundingBox).center();
                let dist_sq = (norm_px - bx).powi(2) + (norm_py - by).powi(2);
                if dist_sq < (min_separation_sq / (sw as f32).powi(2)) {
                    separate = false;
                    break;
                }
            }

            if separate {
                let w = 0.28_f32;
                let h = 0.42_f32;
                boxes.push(BoundingBox {
                    x: (norm_px - w * 0.5).clamp(0.0, 1.0 - w),
                    y: (norm_py - h * 0.5).clamp(0.0, 1.0 - h),
                    width: w,
                    height: h,
                });
            }
        }

        if boxes.is_empty() {
            vec![self.detect_salient_subject(frame, width, height, channels)]
        } else {
            boxes
        }
    }

    /// Compute a unified bounding box that encloses all detected subjects with headroom.
    pub fn compute_multi_subject_framing_box(
        &self,
        subjects: &[BoundingBox],
        target_aspect_ratio: f32,
    ) -> BoundingBox {
        if subjects.is_empty() {
            return BoundingBox {
                x: 0.35,
                y: 0.25,
                width: 0.30,
                height: 0.50,
            };
        }

        let mut min_x = 1.0f32;
        let mut max_x = 0.0f32;
        let mut min_y = 1.0f32;
        let mut max_y = 0.0f32;

        for s in subjects {
            min_x = min_x.min(s.x);
            max_x = max_x.max(s.x + s.width);
            min_y = min_y.min(s.y);
            max_y = max_y.max(s.y + s.height);
        }

        // Add 10% breathing room / margins
        let margin_x = (max_x - min_x) * 0.12;
        let margin_y = (max_y - min_y) * 0.12;

        let w = (max_x - min_x + margin_x * 2.0).clamp(0.25, 1.0);
        let h = (max_y - min_y + margin_y * 2.0).clamp(0.30, 1.0);
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;

        // Shape to target aspect ratio
        let (final_w, final_h) = if w / h > target_aspect_ratio {
            (w, (w / target_aspect_ratio).clamp(0.2, 1.0))
        } else {
            ((h * target_aspect_ratio).clamp(0.2, 1.0), h)
        };

        BoundingBox {
            x: (cx - final_w * 0.5).clamp(0.0, 1.0 - final_w),
            y: (cy - final_h * 0.5).clamp(0.0, 1.0 - final_h),
            width: final_w,
            height: final_h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saliency_empty_frame_returns_safe_box() {
        let detector = SaliencyDetector::new(SaliencyConfig::default());
        let frame = vec![128u8; 100 * 100 * 3];
        let bbox = detector.detect_salient_subject(&frame, 100, 100, 3);
        assert!((bbox.x - 0.35).abs() < 1e-4);
        assert!((bbox.y - 0.25).abs() < 1e-4);
    }

    #[test]
    fn test_saliency_detects_high_contrast_skin_subject() {
        let detector = SaliencyDetector::new(SaliencyConfig::default());
        let width = 200;
        let height = 200;
        let mut frame = vec![40u8; width * height * 3]; // dark background

        // Place a high-contrast skin-toned subject cluster on the right side (x: 140..180, y: 50..150)
        for y in 50..150 {
            for x in 140..180 {
                let idx = (y * width + x) * 3;
                frame[idx] = 230; // R
                frame[idx + 1] = 160; // G
                frame[idx + 2] = 120; // B (skin tone)
            }
        }

        let bbox = detector.detect_salient_subject(&frame, width, height, 3);
        let (cx, cy) = bbox.center();
        // Subject center is around x=160/200 = 0.80, y=100/200 = 0.50
        assert!(
            cx > 0.65,
            "Expected detected center x to be right-biased (>0.65), got {}",
            cx
        );
        assert!(
            (cy - 0.50).abs() < 0.20,
            "Expected detected center y to be centered, got {}",
            cy
        );
    }
}
