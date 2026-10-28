//! Timeline: container for tracks in a project.

use serde::{Deserialize, Serialize};

use crate::track::{Track, TrackId, TrackKind};
use crate::{DurationUs, TimestampUs};

/// The timeline holds all tracks and manages their ordering.
///
/// Tracks are ordered from bottom (index 0) to top. During compositing,
/// higher-index tracks render on top of lower-index tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    /// All tracks, ordered from bottom to top.
    tracks: Vec<Track>,
    /// Next track ID counter.
    next_track_id: u64,
    /// Playhead position in microseconds.
    #[serde(default)]
    pub playhead_us: TimestampUs,
}

impl Timeline {
    /// Create a new empty timeline.
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            next_track_id: 1,
            playhead_us: 0,
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
}
