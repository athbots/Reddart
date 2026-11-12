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
    /// A nested sequence / compound clip.
    Compound(CompoundClip),
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

/// Nested sequence / compound clip data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompoundClip {
    /// Name of the compound sequence (e.g. "Scene 1 Compound").
    pub name: String,
    /// Inner timeline containing the nested tracks and clips.
    pub timeline: Box<crate::timeline::Timeline>,
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

fn default_volume() -> f32 {
    1.0
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
    /// Audio volume multiplier (0.0 to 2.0, default 1.0).
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Audio stereo pan (-1.0 left to +1.0 right, default 0.0 center).
    #[serde(default)]
    pub pan: f32,
    /// Keyframe animation tracks for parameters (e.g. "opacity", "pos_x", "pos_y", "scale_x", "scale_y", "rotation", "speed", "volume").
    #[serde(default)]
    pub keyframes: std::collections::HashMap<String, crate::KeyframeTrack>,
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
            volume: 1.0,
            pan: 0.0,
            keyframes: std::collections::HashMap::new(),
        }
    }

    /// Create a new audio clip.
    pub fn new_audio(
        source_path: impl Into<String>,
        position_us: crate::TimestampUs,
        duration_us: crate::DurationUs,
    ) -> Self {
        Self {
            id: ClipId::new(),
            kind: ClipKind::Audio(AudioClip {
                source_path: source_path.into(),
                volume: 1.0,
            }),
            position_us,
            source_in_us: 0,
            source_out_us: duration_us,
            speed: 1.0,
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            keyframes: std::collections::HashMap::new(),
        }
    }

    /// Create a new text overlay clip.
    pub fn new_text(
        text: impl Into<String>,
        position_us: crate::TimestampUs,
        duration_us: crate::DurationUs,
    ) -> Self {
        Self {
            id: ClipId::new(),
            kind: ClipKind::Text(TextClip {
                text: text.into(),
                font_family: "Inter".into(),
                font_size: 48.0,
                color: [1.0, 1.0, 1.0, 1.0],
                position: [0.5, 0.8],
            }),
            position_us,
            source_in_us: 0,
            source_out_us: duration_us,
            speed: 1.0,
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            keyframes: std::collections::HashMap::new(),
        }
    }

    /// Create a new compound clip nesting a timeline sequence.
    pub fn new_compound(
        name: impl Into<String>,
        inner_timeline: crate::timeline::Timeline,
        position_us: crate::TimestampUs,
    ) -> Self {
        let duration_us = inner_timeline.duration_us();
        Self {
            id: ClipId::new(),
            kind: ClipKind::Compound(CompoundClip {
                name: name.into(),
                timeline: Box::new(inner_timeline),
            }),
            position_us,
            source_in_us: 0,
            source_out_us: duration_us,
            speed: 1.0,
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            keyframes: std::collections::HashMap::new(),
        }
    }

    /// Return true if this clip is a nested sequence / compound clip.
    pub fn is_compound(&self) -> bool {
        matches!(self.kind, ClipKind::Compound(_))
    }

    /// Retrieve a reference to the nested timeline if this is a compound clip.
    pub fn nested_timeline(&self) -> Option<&crate::timeline::Timeline> {
        if let ClipKind::Compound(ref c) = self.kind {
            Some(&c.timeline)
        } else {
            None
        }
    }

    /// Retrieve a mutable reference to the nested timeline if this is a compound clip.
    pub fn nested_timeline_mut(&mut self) -> Option<&mut crate::timeline::Timeline> {
        if let ClipKind::Compound(ref mut c) = self.kind {
            Some(&mut c.timeline)
        } else {
            None
        }
    }

    /// Add or update a keyframe for a parameter.
    pub fn set_keyframe(
        &mut self,
        parameter: &str,
        time_us: crate::TimestampUs,
        value: f32,
        easing: crate::Easing,
    ) {
        let track = self
            .keyframes
            .entry(parameter.to_string())
            .or_insert_with(|| crate::KeyframeTrack::new(parameter));
        track.add_keyframe(crate::Keyframe {
            time_us,
            value,
            easing,
        });
    }

    /// Remove a keyframe by parameter and index.
    pub fn remove_keyframe(&mut self, parameter: &str, index: usize) -> Option<crate::Keyframe> {
        self.keyframes
            .get_mut(parameter)
            .and_then(|t| t.remove_keyframe(index))
    }

    /// Clear all keyframes for a parameter.
    pub fn clear_keyframes(&mut self, parameter: &str) {
        self.keyframes.remove(parameter);
    }

    /// Evaluate a parameter's value at a given timeline position.
    /// `timeline_us` is converted to relative clip offset time.
    pub fn evaluate_keyframe(
        &self,
        parameter: &str,
        timeline_us: crate::TimestampUs,
    ) -> Option<f32> {
        if timeline_us < self.position_us || timeline_us >= self.end_us() {
            return None;
        }
        let rel_us = timeline_us - self.position_us;
        self.keyframes
            .get(parameter)
            .and_then(|t| t.evaluate(rel_us))
    }

    /// Get effective opacity at a specific timeline timestamp, factoring in keyframes.
    pub fn effective_opacity(&self, timeline_us: crate::TimestampUs) -> f32 {
        self.evaluate_keyframe("opacity", timeline_us)
            .unwrap_or(self.opacity)
    }

    /// Get effective transform at a specific timeline timestamp, factoring in keyframes.
    pub fn effective_transform(&self, timeline_us: crate::TimestampUs) -> Transform2D {
        let mut t = self.transform;
        if let Some(x) = self.evaluate_keyframe("pos_x", timeline_us) {
            t.position.x = x;
        }
        if let Some(y) = self.evaluate_keyframe("pos_y", timeline_us) {
            t.position.y = y;
        }
        if let Some(sx) = self.evaluate_keyframe("scale_x", timeline_us) {
            t.scale.x = sx;
        }
        if let Some(sy) = self.evaluate_keyframe("scale_y", timeline_us) {
            t.scale.y = sy;
        }
        if let Some(rot) = self.evaluate_keyframe("rotation", timeline_us) {
            t.rotation = rot;
        }
        t
    }

    /// Get effective audio volume at a specific timeline timestamp, factoring in keyframes.
    pub fn effective_volume(&self, timeline_us: crate::TimestampUs) -> f32 {
        let base_vol = match &self.kind {
            ClipKind::Audio(a) => a.volume * self.volume,
            _ => self.volume,
        };
        self.evaluate_keyframe("volume", timeline_us)
            .unwrap_or(base_vol)
    }

    /// Get effective speed at a specific timeline timestamp.
    pub fn effective_speed(&self, timeline_us: crate::TimestampUs) -> f64 {
        if let Some(s) = self.evaluate_keyframe("speed", timeline_us) {
            s as f64
        } else {
            self.speed
        }
    }

    /// Returns true if this clip has any keyframes defined.
    pub fn has_keyframes(&self) -> bool {
        self.keyframes.values().any(|t| !t.is_empty())
    }

    /// Total count of keyframes across all animated parameters.
    pub fn total_keyframe_count(&self) -> usize {
        self.keyframes.values().map(|t| t.len()).sum()
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

    /// Returns the media source path if this clip references external media.
    pub fn source_path(&self) -> Option<&str> {
        match &self.kind {
            ClipKind::Video(v) => Some(&v.source_path),
            ClipKind::Audio(a) => Some(&a.source_path),
            ClipKind::Image(i) => Some(&i.source_path),
            ClipKind::Text(_) | ClipKind::Compound(_) => None,
        }
    }

    /// Sets the media source path if this clip references external media.
    pub fn set_source_path(&mut self, new_path: impl Into<String>) -> bool {
        let path = new_path.into();
        match &mut self.kind {
            ClipKind::Video(v) => {
                v.source_path = path;
                true
            }
            ClipKind::Audio(a) => {
                a.source_path = path;
                true
            }
            ClipKind::Image(i) => {
                i.source_path = path;
                true
            }
            ClipKind::Text(_) | ClipKind::Compound(_) => false,
        }
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
            volume: 1.0,
            pan: 0.0,
            keyframes: std::collections::HashMap::new(),
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

    #[test]
    fn test_clip_keyframe_animation() {
        let mut clip = make_video_clip();
        assert!(!clip.has_keyframes());
        assert_eq!(clip.total_keyframe_count(), 0);

        // Keyframe opacity from 0.0 at start to 1.0 at 2 seconds
        clip.set_keyframe("opacity", 0, 0.0, crate::Easing::Linear);
        clip.set_keyframe("opacity", 2_000_000, 1.0, crate::Easing::Linear);

        assert!(clip.has_keyframes());
        assert_eq!(clip.total_keyframe_count(), 2);

        // Clip starts at 1_000_000 us on timeline
        // At 1_000_000 (clip start): opacity is 0.0
        assert_eq!(clip.effective_opacity(1_000_000), 0.0);
        // At 2_000_000 (1s into clip): opacity is 0.5
        let op_mid = clip.effective_opacity(2_000_000);
        assert!((op_mid - 0.5).abs() < 0.01);
        // At 3_000_000 (2s into clip): opacity is 1.0
        assert_eq!(clip.effective_opacity(3_000_000), 1.0);
        // At 4_000_000 (3s into clip, past last keyframe): holds 1.0
        assert_eq!(clip.effective_opacity(4_000_000), 1.0);
    }

    #[test]
    fn test_clip_keyframe_transform_and_volume() {
        let mut clip = make_video_clip();
        clip.set_keyframe("pos_x", 0, 100.0, crate::Easing::Linear);
        clip.set_keyframe("pos_x", 1_000_000, 200.0, crate::Easing::Linear);
        clip.set_keyframe("volume", 0, 0.2, crate::Easing::Linear);
        clip.set_keyframe("volume", 1_000_000, 0.8, crate::Easing::Linear);

        let t_mid = clip.effective_transform(1_500_000);
        assert!((t_mid.position.x - 150.0).abs() < 0.01);

        let v_mid = clip.effective_volume(1_500_000);
        assert!((v_mid - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_compound_clip_duration_and_speed() {
        let mut inner_timeline = crate::timeline::Timeline::new();
        let track_id = inner_timeline.add_track(crate::track::TrackKind::Video, "V1");
        let track = inner_timeline.track_mut(track_id).unwrap();
        // Inner clip from 0 to 4s
        track.add_clip(Clip::new_video("inner1.mp4", 0, 0, 4_000_000));
        assert_eq!(inner_timeline.duration_us(), 4_000_000);

        let mut compound = Clip::new_compound("Intro Sequence", inner_timeline, 2_000_000);
        assert!(compound.is_compound());
        assert_eq!(compound.position_us, 2_000_000);
        assert_eq!(compound.duration_us(), 4_000_000);
        assert_eq!(compound.end_us(), 6_000_000);

        // Speed 2x on compound clip halves timeline duration to 2s
        compound.speed = 2.0;
        assert_eq!(compound.duration_us(), 2_000_000);
        assert_eq!(compound.end_us(), 4_000_000);
    }

    #[test]
    fn test_compound_clip_timeline_to_source() {
        let mut inner_timeline = crate::timeline::Timeline::new();
        let track_id = inner_timeline.add_track(crate::track::TrackKind::Video, "V1");
        let track = inner_timeline.track_mut(track_id).unwrap();
        track.add_clip(Clip::new_video("inner1.mp4", 0, 0, 6_000_000));

        let compound = Clip::new_compound("InnerSeq", inner_timeline, 3_000_000);
        // At parent timeline 3s (start of compound) -> inner source time 0s
        assert_eq!(compound.timeline_to_source_us(3_000_000), Some(0));
        // At parent timeline 4.5s -> inner source time 1.5s
        assert_eq!(compound.timeline_to_source_us(4_500_000), Some(1_500_000));
        // Outside compound clip boundary
        assert_eq!(compound.timeline_to_source_us(2_500_000), None);
        assert_eq!(compound.timeline_to_source_us(9_500_000), None);
    }
}
