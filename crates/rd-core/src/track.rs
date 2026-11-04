//! Track: a lane on the timeline holding clips.

use serde::{Deserialize, Serialize};

use crate::clip::{Clip, ClipId};
use crate::{DurationUs, TimestampUs};

/// Unique identifier for a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub u64);

/// The kind of content a track holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    /// Video (and optionally embedded audio).
    Video,
    /// Audio only.
    Audio,
    /// Text/title overlays.
    Text,
    /// Sticker/image overlays.
    Overlay,
}

/// A track is a lane on the timeline holding zero or more clips.
///
/// Clips within a track are non-overlapping and ordered by position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique track ID.
    pub id: TrackId,
    /// The kind of content this track holds.
    pub kind: TrackKind,
    /// Human-readable name.
    pub name: String,
    /// Whether the track is visible/audible during preview.
    pub enabled: bool,
    /// Whether the track is locked (prevents editing).
    pub locked: bool,
    /// Clips in this track, sorted by position_us.
    clips: Vec<Clip>,
}

impl Track {
    /// Create a new empty track.
    pub fn new(id: TrackId, kind: TrackKind, name: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            name: name.into(),
            enabled: true,
            locked: false,
            clips: Vec::new(),
        }
    }

    /// Add a clip to the track. Maintains sorted order by position.
    pub fn add_clip(&mut self, clip: Clip) {
        let pos = self
            .clips
            .binary_search_by_key(&clip.position_us, |c| c.position_us)
            .unwrap_or_else(|e| e);
        self.clips.insert(pos, clip);
    }

    /// Remove a clip by ID. Returns the removed clip if found.
    pub fn remove_clip(&mut self, id: ClipId) -> Option<Clip> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            Some(self.clips.remove(pos))
        } else {
            None
        }
    }

    /// Get a reference to a clip by ID.
    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    /// Get a mutable reference to a clip by ID.
    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    /// Get all clips in order.
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// Get mutable access to all clips.
    pub fn clips_mut(&mut self) -> &mut [Clip] {
        &mut self.clips
    }

    /// Total duration of this track (end of last clip).
    pub fn duration_us(&self) -> DurationUs {
        self.clips
            .last()
            .map(|c| c.position_us + c.duration_us())
            .unwrap_or(0)
    }

    /// Whether there is any clip covering the given timestamp.
    pub fn has_content_at(&self, time_us: TimestampUs) -> bool {
        self.clip_at(time_us).is_some()
    }

    /// Get the clip at a given timestamp, if any.
    pub fn clip_at(&self, time_us: TimestampUs) -> Option<&Clip> {
        self.clips
            .iter()
            .find(|c| time_us >= c.position_us && time_us < c.position_us + c.duration_us())
    }

    /// Find the nearest snap point to a given timestamp.
    /// Returns clip edges (start and end positions).
    pub fn snap_points(&self) -> Vec<TimestampUs> {
        let mut points = Vec::with_capacity(self.clips.len() * 2);
        for clip in &self.clips {
            points.push(clip.position_us);
            points.push(clip.position_us + clip.duration_us());
        }
        points
    }

    /// Split a clip at a given timestamp. Returns the new clip ID if split occurred.
    pub fn split_clip_at(&mut self, clip_id: ClipId, split_time_us: TimestampUs) -> Option<ClipId> {
        let clip_index = self.clips.iter().position(|c| c.id == clip_id)?;
        let clip = &self.clips[clip_index];

        // Validate split point is within the clip
        let clip_start = clip.position_us;
        let clip_end = clip_start + clip.duration_us();
        if split_time_us <= clip_start || split_time_us >= clip_end {
            return None;
        }

        // Create the second half
        let mut second_half = clip.clone();
        second_half.id = ClipId(uuid::Uuid::new_v4());
        second_half.position_us = split_time_us;

        // Calculate the offset into the source for the second half
        let offset_into_clip = split_time_us - clip_start;
        second_half.source_in_us += offset_into_clip;

        // Trim the first half
        self.clips[clip_index].source_out_us =
            self.clips[clip_index].source_in_us + offset_into_clip;

        // Insert the second half
        let new_id = second_half.id;
        self.clips.insert(clip_index + 1, second_half);

        Some(new_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::ClipKind;

    fn make_test_clip(position_us: TimestampUs, duration_us: DurationUs) -> Clip {
        Clip {
            id: ClipId(uuid::Uuid::new_v4()),
            kind: ClipKind::Video(crate::clip::VideoClip {
                source_path: "test.mp4".into(),
                proxy_path: None,
                has_audio: true,
            }),
            position_us,
            source_in_us: 0,
            source_out_us: duration_us,
            speed: 1.0,
            opacity: 1.0,
            transform: crate::clip::Transform2D::default(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn test_track_clip_ordering() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip_b = make_test_clip(2_000_000, 1_000_000); // 2s
        let clip_a = make_test_clip(0, 1_000_000); // 0s

        track.add_clip(clip_b);
        track.add_clip(clip_a);

        assert_eq!(track.clips()[0].position_us, 0);
        assert_eq!(track.clips()[1].position_us, 2_000_000);
    }

    #[test]
    fn test_track_duration() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        track.add_clip(make_test_clip(0, 1_000_000));
        track.add_clip(make_test_clip(2_000_000, 1_500_000));

        assert_eq!(track.duration_us(), 3_500_000);
    }

    #[test]
    fn test_split_clip() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip = make_test_clip(0, 2_000_000); // 2 second clip
        let clip_id = clip.id;
        track.add_clip(clip);

        // Split at 1 second
        let new_id = track.split_clip_at(clip_id, 1_000_000);
        assert!(new_id.is_some());
        assert_eq!(track.clips().len(), 2);

        // First half: 0-1s
        assert_eq!(track.clips()[0].position_us, 0);
        assert_eq!(track.clips()[0].duration_us(), 1_000_000);

        // Second half: 1-2s
        assert_eq!(track.clips()[1].position_us, 1_000_000);
        assert_eq!(track.clips()[1].duration_us(), 1_000_000);
    }
}
