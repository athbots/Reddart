//! Timeline: container for tracks in a project.

use serde::{Deserialize, Serialize};

use crate::track::{Track, TrackId, TrackKind};
use crate::{DurationUs, TimestampUs};

/// Professional timeline marker for notes, beats, or cue points.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineMarker {
    pub id: u64,
    pub time_us: TimestampUs,
    pub name: String,
    pub color_rgb: [u8; 3],
}

/// The timeline holds all tracks and manages their ordering.
///
/// Tracks are ordered from bottom (index 0) to top. During compositing,
/// higher-index tracks render on top of lower-index tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    /// All tracks, ordered from bottom to top.
    tracks: Vec<Track>,
    /// Next track ID counter.
    #[serde(default)]
    next_track_id: u64,
    /// Playhead position in microseconds.
    #[serde(default)]
    pub playhead_us: TimestampUs,
    /// In-point for sequence work area / export range.
    #[serde(default)]
    pub mark_in_us: Option<TimestampUs>,
    /// Out-point for sequence work area / export range.
    #[serde(default)]
    pub mark_out_us: Option<TimestampUs>,
    /// Production markers on the timeline.
    #[serde(default)]
    pub markers: Vec<TimelineMarker>,
    /// Next marker ID counter.
    #[serde(default)]
    next_marker_id: u64,
}

impl Timeline {
    /// Create a new empty timeline.
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            next_track_id: 1,
            playhead_us: 0,
            mark_in_us: None,
            mark_out_us: None,
            markers: Vec::new(),
            next_marker_id: 1,
        }
    }

    /// Set sequence In-point (I shortcut).
    pub fn set_mark_in(&mut self, time_us: TimestampUs) {
        self.mark_in_us = Some(time_us);
    }

    /// Set sequence Out-point (O shortcut).
    pub fn set_mark_out(&mut self, time_us: TimestampUs) {
        self.mark_out_us = Some(time_us);
    }

    /// Clear In and Out points (Alt+X / X shortcut).
    pub fn clear_in_out(&mut self) {
        self.mark_in_us = None;
        self.mark_out_us = None;
    }

    /// Add a marker to the timeline at the specified timestamp.
    pub fn add_marker(
        &mut self,
        time_us: TimestampUs,
        name: impl Into<String>,
        color_rgb: [u8; 3],
    ) -> u64 {
        let id = self.next_marker_id;
        self.next_marker_id += 1;
        self.markers.push(TimelineMarker {
            id,
            time_us,
            name: name.into(),
            color_rgb,
        });
        self.markers.sort_by_key(|m| m.time_us);
        id
    }

    /// Remove marker by ID.
    pub fn remove_marker(&mut self, id: u64) -> bool {
        if let Some(pos) = self.markers.iter().position(|m| m.id == id) {
            self.markers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Add a new track at the top and return its ID.
    pub fn add_track(&mut self, kind: TrackKind, name: impl Into<String>) -> TrackId {
        let id = TrackId(self.next_track_id);
        self.next_track_id += 1;
        self.tracks.push(Track::new(id, kind, name));
        id
    }

    /// Insert a track at a specific index.
    pub fn insert_track(
        &mut self,
        index: usize,
        kind: TrackKind,
        name: impl Into<String>,
    ) -> TrackId {
        let id = TrackId(self.next_track_id);
        self.next_track_id += 1;
        let index = index.min(self.tracks.len());
        self.tracks.insert(index, Track::new(id, kind, name));
        id
    }

    /// Remove a track by ID. Returns the removed track if found.
    pub fn remove_track(&mut self, id: TrackId) -> Option<Track> {
        if let Some(pos) = self.tracks.iter().position(|t| t.id == id) {
            Some(self.tracks.remove(pos))
        } else {
            None
        }
    }

    /// Get a reference to a track by ID.
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// Get a mutable reference to a track by ID.
    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    /// Get all tracks (bottom to top ordering).
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Get mutable access to all tracks.
    pub fn tracks_mut(&mut self) -> &mut [Track] {
        &mut self.tracks
    }

    /// Move a track from one index to another.
    pub fn move_track(&mut self, from: usize, to: usize) {
        if from < self.tracks.len() && to < self.tracks.len() && from != to {
            let track = self.tracks.remove(from);
            self.tracks.insert(to, track);
        }
    }

    /// Number of tracks.
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// Total number of clips across all tracks.
    pub fn total_clip_count(&self) -> usize {
        self.tracks.iter().map(|t| t.clips().len()).sum()
    }

    /// Total duration of the timeline (the end of the last clip on any track).
    pub fn duration_us(&self) -> DurationUs {
        self.tracks
            .iter()
            .map(|t| t.duration_us())
            .max()
            .unwrap_or(0)
    }

    /// Get all tracks that are visible at a given timestamp.
    pub fn active_tracks_at(&self, time_us: TimestampUs) -> Vec<&Track> {
        self.tracks
            .iter()
            .filter(|t| t.has_content_at(time_us))
            .collect()
    }

    /// Return all snap points (clip start and end boundaries) across all tracks, sorted.
    pub fn snap_points(&self) -> Vec<TimestampUs> {
        let mut points = vec![0];
        for track in &self.tracks {
            for clip in track.clips() {
                points.push(clip.position_us);
                points.push(clip.end_us());
            }
        }
        points.sort_unstable();
        points.dedup();
        points
    }

    /// Find the nearest snap point to `target_us` within `threshold_us`.
    pub fn snap_to(&self, target_us: TimestampUs, threshold_us: i64) -> TimestampUs {
        let mut nearest = target_us;
        let mut min_diff = i64::MAX;
        for &pt in &self.snap_points() {
            let diff = (pt - target_us).abs();
            if diff <= threshold_us && diff < min_diff {
                min_diff = diff;
                nearest = pt;
            }
        }
        nearest
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_remove_tracks() {
        let mut timeline = Timeline::new();
        let id1 = timeline.add_track(TrackKind::Video, "Video 1");
        let id2 = timeline.add_track(TrackKind::Audio, "Audio 1");

        assert_eq!(timeline.track_count(), 2);
        assert!(timeline.track(id1).is_some());
        assert!(timeline.track(id2).is_some());

        timeline.remove_track(id1);
        assert_eq!(timeline.track_count(), 1);
        assert!(timeline.track(id1).is_none());
        assert!(timeline.track(id2).is_some());
    }

    #[test]
    fn test_move_track() {
        let mut timeline = Timeline::new();
        let id1 = timeline.add_track(TrackKind::Video, "V1");
        let id2 = timeline.add_track(TrackKind::Video, "V2");
        let id3 = timeline.add_track(TrackKind::Video, "V3");

        // Move V1 (index 0) to index 2 (top)
        timeline.move_track(0, 2);
        assert_eq!(timeline.tracks()[0].id, id2);
        assert_eq!(timeline.tracks()[1].id, id3);
        assert_eq!(timeline.tracks()[2].id, id1);
    }

    #[test]
    fn test_snap_points_and_snap_to() {
        let mut timeline = Timeline::new();
        let vid = timeline.add_track(TrackKind::Video, "V1");
        timeline
            .track_mut(vid)
            .unwrap()
            .add_clip(crate::Clip::new_video("test.mp4", 1_000_000, 0, 4_000_000));

        let snaps = timeline.snap_points();
        assert_eq!(snaps, vec![0, 1_000_000, 5_000_000]);

        // Near 1_000_000 (at 1_040_000 with 50_000 threshold -> snaps to 1_000_000)
        assert_eq!(timeline.snap_to(1_040_000, 50_000), 1_000_000);
        // Too far (at 1_200_000 with 50_000 threshold -> does not snap)
        assert_eq!(timeline.snap_to(1_200_000, 50_000), 1_200_000);
    }
}
