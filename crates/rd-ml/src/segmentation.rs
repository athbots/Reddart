//! Portrait Background Removal & Segmentation.
//!
//! Section W3.3 of the Technical Proposal:
//! Lightweight on-device segmentation model (MODNet / MediaPipe class).
//! Evaluates video frames and produces a high-resolution alpha matte to isolate subjects.

use serde::{Deserialize, Serialize};

/// Output mask format for segmented video frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentationMask {
    pub width: u32,
    pub height: u32,
    /// 8-bit grayscale alpha values (0 = background, 255 = foreground subject).
    pub alpha_bytes: Vec<u8>,
}

impl SegmentationMask {
    pub fn new(width: u32, height: u32, fill: u8) -> Self {
        Self {
            width,
            height,
            alpha_bytes: vec![fill; (width * height) as usize],
        }
    }

    /// Read alpha at coordinates (x, y).
    pub fn get_alpha(&self, x: u32, y: u32) -> u8 {
        if x < self.width && y < self.height {
            self.alpha_bytes[(y * self.width + x) as usize]
        } else {
            0
        }
    }

    /// Feather mask edges with a 1D box blur pass to avoid harsh boundaries.
    pub fn feather_edges(&mut self, radius: usize) {
        if radius == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        let mut blurred = self.alpha_bytes.clone();
        let w = self.width as usize;
        let h = self.height as usize;

        for y in 0..h {
            for x in 0..w {
                let x_min = x.saturating_sub(radius);
                let x_max = (x + radius).min(w - 1);
                let count = x_max - x_min + 1;
                let sum: u32 = (x_min..=x_max).map(|ix| self.alpha_bytes[y * w + ix] as u32).sum();
                blurred[y * w + x] = (sum / count as u32) as u8;
            }
        }

        self.alpha_bytes = blurred;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segmentation_mask_feathering() {
        let mut mask = SegmentationMask::new(10, 10, 0);
        // Put foreground in the center 4x4
        for y in 3..7 {
            for x in 3..7 {
                mask.alpha_bytes[y * 10 + x] = 255;
            }
        }

        assert_eq!(mask.get_alpha(5, 5), 255);
        assert_eq!(mask.get_alpha(1, 1), 0);

        mask.feather_edges(1);
        // Edge adjacent to foreground should now be softened (> 0 and < 255)
        let edge_val = mask.get_alpha(2, 5);
        assert!(edge_val > 0 && edge_val < 255);
    }
}

