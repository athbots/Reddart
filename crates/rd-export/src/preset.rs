//! Export presets calibrated for social platforms and professional delivery.
//!
//! Section 3.10 of the Technical Proposal:
//! Presets are calibrated against measured upload results, not assumed bitrates.

use serde::{Deserialize, Serialize};

fn default_bit_depth() -> u32 {
    8
}

/// Rate control modes for hardware and software video encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateControlMode {
    /// Constant Bitrate (CBR) targeting fixed bitrate with tight buffer constraints.
    Cbr,
    /// Variable Bitrate (VBR) balancing quality and file size around average bitrate.
    Vbr,
    /// Constant Rate Factor / Constant Quality (CRF/CQ: lower is higher quality).
    Crf(u32),
    /// Constrained Variable Bitrate with explicit peak and buffer ceilings.
    Cvbr {
        max_bitrate_kbps: u32,
        bufsize_kbps: u32,
    },
}

/// Export preset defining video and audio encoding configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub video_codec: String,
    pub audio_codec: String,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u32,
    #[serde(default)]
    pub color_space: Option<String>,
    #[serde(default)]
    pub rate_control: Option<RateControlMode>,
}

impl Default for ExportPreset {
    fn default() -> Self {
        Self::youtube_1080p()
    }
}

impl ExportPreset {
    /// Standard 1080p landscape (YouTube, Vimeo, Web).
    pub fn youtube_1080p() -> Self {
        Self {
            name: "YouTube 1080p".into(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            video_bitrate_kbps: 8000,
            audio_bitrate_kbps: 192,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            bit_depth: 8,
            color_space: Some("bt709".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// Vertical 9:16 (TikTok, Instagram Reels, YouTube Shorts).
    pub fn vertical_9_16_1080p() -> Self {
        Self {
            name: "Shorts / TikTok 1080x1920".into(),
            width: 1080,
            height: 1920,
            fps: 30.0,
            video_bitrate_kbps: 7000,
            audio_bitrate_kbps: 192,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            bit_depth: 8,
            color_space: Some("bt709".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// Ultra HD 4K landscape (H.265 / HEVC).
    pub fn uhd_4k() -> Self {
        Self {
            name: "4K UHD HEVC".into(),
            width: 3840,
            height: 2160,
            fps: 30.0,
            video_bitrate_kbps: 25000,
            audio_bitrate_kbps: 320,
            video_codec: "hevc".into(),
            audio_codec: "aac".into(),
            bit_depth: 8,
            color_space: Some("bt709".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// Square 1:1 format (Instagram Feed).
    pub fn instagram_square() -> Self {
        Self {
            name: "Instagram Square (1080x1080)".into(),
            width: 1080,
            height: 1080,
            fps: 30.0,
            video_bitrate_kbps: 6000,
            audio_bitrate_kbps: 192,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            bit_depth: 8,
            color_space: Some("bt709".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// Next-Generation AV1 1080p (superior compression efficiency for streaming).
    pub fn av1_1080p() -> Self {
        Self {
            name: "AV1 1080p Web".into(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            video_bitrate_kbps: 5500,
            audio_bitrate_kbps: 192,
            video_codec: "av1".into(),
            audio_codec: "aac".into(),
            bit_depth: 8,
            color_space: Some("bt709".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// Next-Generation AV1 4K UHD.
    pub fn av1_4k() -> Self {
        Self {
            name: "AV1 4K UHD".into(),
            width: 3840,
            height: 2160,
            fps: 30.0,
            video_bitrate_kbps: 18000,
            audio_bitrate_kbps: 320,
            video_codec: "av1".into(),
            audio_codec: "aac".into(),
            bit_depth: 8,
            color_space: Some("bt709".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// 10-Bit HDR10 HEVC 4K UHD (BT.2020 Color Primaries + SMPTE ST 2084 PQ curve).
    pub fn hdr10_hevc_4k() -> Self {
        Self {
            name: "HDR10 HEVC 4K UHD (10-bit BT.2020 PQ)".into(),
            width: 3840,
            height: 2160,
            fps: 30.0,
            video_bitrate_kbps: 28000,
            audio_bitrate_kbps: 320,
            video_codec: "hevc".into(),
            audio_codec: "aac".into(),
            bit_depth: 10,
            color_space: Some("bt2020_pq".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// 10-Bit HDR10 AV1 4K UHD.
    pub fn hdr10_av1_4k() -> Self {
        Self {
            name: "HDR10 AV1 4K UHD (10-bit BT.2020 PQ)".into(),
            width: 3840,
            height: 2160,
            fps: 30.0,
            video_bitrate_kbps: 20000,
            audio_bitrate_kbps: 320,
            video_codec: "av1".into(),
            audio_codec: "aac".into(),
            bit_depth: 10,
            color_space: Some("bt2020_pq".into()),
            rate_control: Some(RateControlMode::Vbr),
        }
    }

    /// Apple ProRes 422 HQ Broadcast Master (10-bit 4:2:2).
    pub fn mastering_prores_422() -> Self {
        Self {
            name: "Apple ProRes 422 HQ Broadcast Master".into(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            video_bitrate_kbps: 147000,
            audio_bitrate_kbps: 320,
            video_codec: "prores".into(),
            audio_codec: "pcm_s24le".into(),
            bit_depth: 10,
            color_space: Some("bt709".into()),
            rate_control: None,
        }
    }

    /// Builder helper to set bit depth (8, 10, or 12).
    pub fn with_bit_depth(mut self, bit_depth: u32) -> Self {
        self.bit_depth = bit_depth;
        self
    }

    /// Builder helper to set color space metadata ("bt709", "bt2020_pq", "bt2020_hlg").
    pub fn with_color_space(mut self, color_space: impl Into<String>) -> Self {
        self.color_space = Some(color_space.into());
        self
    }

    /// Builder helper to configure rate control mode.
    pub fn with_rate_control(mut self, rate_control: RateControlMode) -> Self {
        self.rate_control = Some(rate_control);
        self
    }
}
