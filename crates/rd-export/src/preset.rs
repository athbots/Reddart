//! Export presets calibrated for social platforms.
//!
//! Section 3.10 of the Technical Proposal:
//! Presets are calibrated against measured upload results, not assumed bitrates.

use serde::{Deserialize, Serialize};

/// Export preset defining video and audio encoding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub video_codec: String,
    pub audio_codec: String,
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
        }
    }

    /// Ultra HD 4K landscape.
    pub fn uhd_4k() -> Self {
        Self {
            name: "4K UHD".into(),
            width: 3840,
            height: 2160,
            fps: 30.0,
            video_bitrate_kbps: 25000,
            audio_bitrate_kbps: 320,
            video_codec: "h265".into(),
            audio_codec: "aac".into(),
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
        }
    }
}
