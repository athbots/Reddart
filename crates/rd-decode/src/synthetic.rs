//! Synthetic and test pattern video decoder.
//!
//! Generates procedural frames (color bars, animated gradient, or moving bounce)
//! for testing, proxy fallback, and zero-hardware previewing.

use crate::{DecodedFrame, DecoderCapabilities, VideoDecoder};
use anyhow::Result;
use rd_core::{DurationUs, TimestampUs};

/// Pattern style rendered by the synthetic decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternStyle {
    /// SMPTE color bars.
    ColorBars,
    /// Animated horizontal gradient.
    AnimatedGradient,
    /// Moving bouncing square with timecode.
    BouncingBox,
}

/// Synthetic procedural video decoder.
pub struct SyntheticDecoder {
    width: u32,
    height: u32,
    duration_us: DurationUs,
    fps: f64,
    current_time_us: TimestampUs,
    frame_index: i64,
    style: PatternStyle,
}

impl SyntheticDecoder {
    /// Create a new synthetic decoder with custom dimensions, duration, and pattern style.
    pub fn new(
        width: u32,
        height: u32,
        duration_us: DurationUs,
        fps: f64,
        style: PatternStyle,
    ) -> Self {
        Self {
            width: width.max(16),
            height: height.max(16),
            duration_us,
            fps: fps.max(1.0),
            current_time_us: 0,
            frame_index: 0,
            style,
        }
    }

    /// Helper to render color bars into RGBA buffer.
    fn render_color_bars(&self, buf: &mut [u8]) {
        let colors: [[u8; 4]; 7] = [
            [255, 255, 255, 255], // White
            [255, 255, 0, 255],   // Yellow
            [0, 255, 255, 255],   // Cyan
            [0, 255, 0, 255],     // Green
            [255, 0, 255, 255],   // Magenta
            [255, 0, 0, 255],     // Red
            [0, 0, 255, 255],     // Blue
        ];

        let w = self.width as usize;
        let h = self.height as usize;
        let col_w = (w / colors.len()).max(1);

        for y in 0..h {
            for x in 0..w {
                let bar_idx = (x / col_w).min(colors.len() - 1);
                let c = colors[bar_idx];
                let idx = (y * w + x) * 4;
                buf[idx..idx + 4].copy_from_slice(&c);
            }
        }
    }

    /// Helper to render animated bouncing box into RGBA buffer.
    fn render_bouncing_box(&self, buf: &mut [u8], time_us: TimestampUs) {
        let w = self.width as usize;
        let h = self.height as usize;

        // Background: dark slate gray
        for chunk in buf.as_chunks_mut::<4>().0 {
            chunk.copy_from_slice(&[24, 28, 36, 255]);
        }

        let time_sec = time_us as f32 / 1_000_000.0;
        let box_size = (w.min(h) / 5).max(10);
        let max_x = (w - box_size) as f32;
        let max_y = (h - box_size) as f32;

        let bx = ((time_sec * 200.0) % (max_x * 2.0) - max_x).abs() as usize;
        let by = ((time_sec * 140.0) % (max_y * 2.0) - max_y).abs() as usize;

        for y in by..(by + box_size).min(h) {
            for x in bx..(bx + box_size).min(w) {
                let idx = (y * w + x) * 4;
                // Vibrant orange box
                buf[idx..idx + 4].copy_from_slice(&[255, 110, 40, 255]);
            }
        }
    }

    /// Helper to render animated gradient into RGBA buffer.
    fn render_gradient(&self, buf: &mut [u8], time_us: TimestampUs) {
        let w = self.width as usize;
        let h = self.height as usize;
        let offset = ((time_us as f64 / 1_000_000.0) * 50.0) as usize;

        for y in 0..h {
            for x in 0..w {
                let r = ((x + offset) % 256) as u8;
                let g = (y * 255 / h) as u8;
                let b = ((x * 255 / w) + 128) as u8;
                let idx = (y * w + x) * 4;
                buf[idx..idx + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
}

impl VideoDecoder for SyntheticDecoder {
    fn seek(&mut self, timestamp_us: TimestampUs) -> Result<()> {
        self.current_time_us = timestamp_us.min(self.duration_us);
        self.frame_index = ((self.current_time_us as f64 / 1_000_000.0) * self.fps) as i64;
        Ok(())
    }

    fn decode_next(&mut self) -> Result<Option<DecodedFrame>> {
        if self.current_time_us >= self.duration_us {
            return Ok(None);
        }

        let mut rgba = vec![0u8; (self.width * self.height * 4) as usize];
        match self.style {
            PatternStyle::ColorBars => self.render_color_bars(&mut rgba),
            PatternStyle::AnimatedGradient => self.render_gradient(&mut rgba, self.current_time_us),
            PatternStyle::BouncingBox => self.render_bouncing_box(&mut rgba, self.current_time_us),
        }

        let frame = DecodedFrame {
            frame_index: self.frame_index,
            pts_us: self.current_time_us,
            width: self.width,
            height: self.height,
            rgba_data: Some(rgba),
            is_keyframe: true, // All-intra procedural frames
        };

        let frame_interval_us = (1_000_000.0 / self.fps) as i64;
        self.current_time_us += frame_interval_us;
        self.frame_index += 1;

        Ok(Some(frame))
    }

    fn duration_us(&self) -> DurationUs {
        self.duration_us
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_rate(&self) -> f64 {
        self.fps
    }

    fn capabilities(&self) -> DecoderCapabilities {
        DecoderCapabilities {
            max_width: 3840,
            max_height: 2160,
            max_instances: 16,
            supports_zero_copy: false,
            codec_name: "procedural_synthetic".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_decoder_seek_and_decode() {
        let mut dec = SyntheticDecoder::new(320, 180, 5_000_000, 30.0, PatternStyle::BouncingBox);
        assert_eq!(dec.duration_us(), 5_000_000);
        assert_eq!(dec.resolution(), (320, 180));

        let frame1 = dec.decode_next().unwrap().expect("frame 0");
        assert_eq!(frame1.frame_index, 0);
        assert_eq!(frame1.pts_us, 0);
        assert_eq!(frame1.width, 320);
        assert_eq!(frame1.height, 180);
        assert_eq!(frame1.rgba_data.as_ref().unwrap().len(), 320 * 180 * 4);

        // Seek to 2 seconds
        dec.seek(2_000_000).unwrap();
        let frame2 = dec.decode_next().unwrap().expect("frame at 2s");
        assert_eq!(frame2.pts_us, 2_000_000);
        assert_eq!(frame2.frame_index, 60); // 2s * 30fps
    }
}
