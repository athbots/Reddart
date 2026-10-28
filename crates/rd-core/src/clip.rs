//! Clip: a segment of media placed on a track.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DurationUs;

/// Unique identifier for a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClipId(pub Uuid);

impl ClipId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self::new()
    }
}

/// 2D vector for position and scale.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// 2D spatial transform (position, scale, rotation).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2D {
    pub position: Vec2,
    pub scale: Vec2,
    pub rotation: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
        }
    }
}

/// The kind of source content a clip references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipKind {
    /// A video clip (may include embedded audio).
    Video(VideoClip),
    /// An audio-only clip.
    Audio(AudioClip),
    /// A text overlay.
    Text(TextClip),
    /// An image/sticker overlay.
    Image(ImageClip),
}

/// Video-specific clip data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoClip {
    /// Path to the source video file.
    pub source_path: String,
    /// Path to the generated proxy file, if available.
    pub proxy_path: Option<String>,
    /// Whether this video has an audio stream to mix.
    pub has_audio: bool,
}

/// Audio-specific clip data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    /// Path to the source audio file.
    pub source_path: String,
    /// Volume level (0.0 to 1.0+).
    pub volume: f32,
}

/// Text overlay clip data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextClip {
    /// The text content.
    pub text: String,
    /// Font family name.
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Text color as RGBA hex.
    pub color: [f32; 4],
    /// Position (normalized 0.0-1.0, relative to canvas).
    pub position: [f32; 2],
}

/// Image/sticker overlay clip data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageClip {
    /// Path to the image file.
    pub source_path: String,
    /// Position (normalized 0.0-1.0, relative to canvas).
    pub position: [f32; 2],
    /// Scale factor.
    pub scale: f32,
}

/// An effect applied to a clip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipEffect {
    /// Effect identifier (matches effect spec file name).
    pub effect_id: String,
    /// Effect parameters as key-value pairs.
    pub parameters: std::collections::HashMap<String, f32>,
    /// Whether this effect is currently enabled.
    pub enabled: bool,
}

/// A clip is a segment of media placed on a track.
///
/// The clip references a source (video, audio, text, image) and defines
/// which portion of the source to use (`source_in_us` to `source_out_us`),
/// where it sits on the timeline (`position_us`), and how fast it plays (`speed`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    /// Unique clip identifier.
    pub id: ClipId,
    /// The source content.
    pub kind: ClipKind,
    /// Position on the timeline in microseconds.
    pub position_us: crate::TimestampUs,
    /// Source in-point in microseconds (start of the used segment).
    pub source_in_us: crate::TimestampUs,
    /// Source out-point in microseconds (end of the used segment).
    pub source_out_us: crate::TimestampUs,
    /// Playback speed multiplier (1.0 = normal, 2.0 = double, 0.5 = half).
    pub speed: f64,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
    /// Spatial 2D transform (position, scale, rotation).
    pub transform: Transform2D,
    /// Effects applied to this clip, in order.
    pub effects: Vec<ClipEffect>,
}

impl Clip {
    /// Create a new video clip.
    pub fn new_video(
        source_path: impl Into<String>,
        position_us: crate::TimestampUs,
        source_in_us: crate::TimestampUs,
        source_out_us: crate::TimestampUs,
    ) -> Self {
        Self {
            id: ClipId::new(),
            kind: ClipKind::Video(VideoClip {
                source_path: source_path.into(),
                proxy_path: None,
                has_audio: true,
            }),
            position_us,
            source_in_us,
            source_out_us,
            speed: 1.0,
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
        }
    }
    /// Duration of this clip on the timeline, accounting for speed.
    pub fn duration_us(&self) -> DurationUs {
        let source_duration = (self.source_out_us - self.source_in_us).max(0);
        let effective_speed = self.speed.abs().max(0.0001);
        (source_duration as f64 / effective_speed) as DurationUs
    }

    /// The end position on the timeline.
    pub fn end_us(&self) -> crate::TimestampUs {
        self.position_us + self.duration_us()
    }

    /// Convert a timeline timestamp to a source timestamp for this clip.
    /// Returns None if the timestamp is outside this clip's range.
    pub fn timeline_to_source_us(
        &self,
        timeline_us: crate::TimestampUs,
    ) -> Option<crate::TimestampUs> {
        if timeline_us < self.position_us || timeline_us >= self.end_us() {
            return None;
        }
        let offset = timeline_us - self.position_us;
        let source_offset = (offset as f64 * self.speed) as crate::TimestampUs;
        Some(self.source_in_us + source_offset)
    }

    /// Trim the in-point (left edge). Adjusts position to keep content aligned.
    pub fn trim_in(&mut self, new_in_us: crate::TimestampUs) {
        let delta = new_in_us - self.source_in_us;
        self.source_in_us = new_in_us;
        self.position_us += (delta as f64 / self.speed) as crate::TimestampUs;
    }

    /// Trim the out-point (right edge).
    pub fn trim_out(&mut self, new_out_us: crate::TimestampUs) {
        self.source_out_us = new_out_us;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_video_clip() -> Clip {
        Clip {
            id: ClipId(Uuid::new_v4()),
            kind: ClipKind::Video(VideoClip {
                source_path: "test.mp4".into(),
                proxy_path: None,
                has_audio: true,
            }),
            position_us: 1_000_000,
            source_in_us: 0,
            source_out_us: 5_000_000,
            speed: 1.0,
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn test_clip_duration_normal_speed() {
        let clip = make_video_clip();
        assert_eq!(clip.duration_us(), 5_000_000);
    }

    #[test]
    fn test_clip_duration_double_speed() {
        let mut clip = make_video_clip();
        clip.speed = 2.0;
        assert_eq!(clip.duration_us(), 2_500_000);
    }

    #[test]
    fn test_clip_duration_half_speed() {
        let mut clip = make_video_clip();
        clip.speed = 0.5;
        assert_eq!(clip.duration_us(), 10_000_000);
    }

    #[test]
    fn test_timeline_to_source() {
        let clip = make_video_clip(); // starts at 1s, 5s long
        assert_eq!(clip.timeline_to_source_us(1_000_000), Some(0));
        assert_eq!(clip.timeline_to_source_us(3_500_000), Some(2_500_000));
        assert_eq!(clip.timeline_to_source_us(0), None); // before clip
        assert_eq!(clip.timeline_to_source_us(6_000_000), None); // after clip
    }

    #[test]
    fn test_timeline_to_source_with_speed() {
        let mut clip = make_video_clip();
        clip.speed = 2.0; // Clip is 2.5s on timeline, plays 5s of source
        assert_eq!(clip.timeline_to_source_us(2_000_000), Some(2_000_000));
    }
}
