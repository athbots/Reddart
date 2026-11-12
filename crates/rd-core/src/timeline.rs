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

    /// Create a compound clip from a selection of clips on a track.
    ///
    /// Extracts the specified clips, shifts their positions relative to the earliest clip's start,
    /// places them into an inner nested timeline sequence, and replaces them on the track with a
    /// single `ClipKind::Compound` clip.
    pub fn create_compound_clip(
        &mut self,
        track_id: TrackId,
        clip_ids: &[crate::clip::ClipId],
        name: impl Into<String>,
    ) -> Result<crate::clip::ClipId, &'static str> {
        if clip_ids.is_empty() {
            return Err("No clips specified for compound clip");
        }

        let track = self
            .track_mut(track_id)
            .ok_or("Track not found on timeline")?;
        let track_kind = track.kind;

        // Verify all clips exist on the track
        for &cid in clip_ids {
            if track.clip(cid).is_none() {
                return Err("Specified clip not found on target track");
            }
        }

        // Extract clips
        let mut extracted = Vec::with_capacity(clip_ids.len());
        for &cid in clip_ids {
            if let Some(c) = track.remove_clip(cid) {
                extracted.push(c);
            }
        }

        if extracted.is_empty() {
            return Err("Failed to extract clips");
        }

        // Find earliest start time
        let min_start_us = extracted.iter().map(|c| c.position_us).min().unwrap();

        // Build inner nested timeline
        let mut inner_timeline = Timeline::new();
        let inner_track_id = inner_timeline.add_track(track_kind, "Inner Track 1");
        let inner_track = inner_timeline.track_mut(inner_track_id).unwrap();

        for mut c in extracted {
            c.position_us -= min_start_us;
            inner_track.add_clip(c);
        }

        // Create the compound clip placed at min_start_us
        let compound = crate::clip::Clip::new_compound(name, inner_timeline, min_start_us);
        let compound_id = compound.id;

        // Insert compound clip back onto the parent track
        let track = self.track_mut(track_id).unwrap();
        track.add_clip(compound);

        Ok(compound_id)
    }

    /// Decompose / flatten a compound clip in place into its constituent clips.
    ///
    /// Replaces the compound clip on `track_id` with all the clips from its inner timeline,
    /// shifting their positions by the compound clip's position and speed.
    pub fn flatten_compound_clip(
        &mut self,
        track_id: TrackId,
        compound_clip_id: crate::clip::ClipId,
    ) -> Result<Vec<crate::clip::ClipId>, &'static str> {
        let track = self
            .track_mut(track_id)
            .ok_or("Track not found on timeline")?;

        let compound_clip = track
            .remove_clip(compound_clip_id)
            .ok_or("Compound clip not found on track")?;

        let compound_data = match compound_clip.kind {
            crate::clip::ClipKind::Compound(comp) => comp,
            _ => {
                // Restore clip if it's not a compound clip
                let track = self.track_mut(track_id).unwrap();
                track.add_clip(compound_clip);
                return Err("Target clip is not a compound clip");
            }
        };

        let mut unnested_ids = Vec::new();
        let base_pos_us = compound_clip.position_us;
        let speed = compound_clip.speed;

        let track = self.track_mut(track_id).unwrap();

        for inner_track in compound_data.timeline.tracks() {
            for inner_clip in inner_track.clips() {
                let mut restored_clip = inner_clip.clone();
                // Map inner timeline position through compound position and speed
                let scaled_offset = (restored_clip.position_us as f64 / speed) as TimestampUs;
                restored_clip.position_us = base_pos_us + scaled_offset;
                restored_clip.speed *= speed;
                unnested_ids.push(restored_clip.id);
                track.add_clip(restored_clip);
            }
        }

        Ok(unnested_ids)
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

    #[test]
    fn test_create_and_flatten_compound_clip() {
        let mut timeline = Timeline::new();
        let vid = timeline.add_track(TrackKind::Video, "V1");

        let clip1 = crate::Clip::new_video("clip1.mp4", 1_000_000, 0, 2_000_000);
        let clip2 = crate::Clip::new_video("clip2.mp4", 3_000_000, 0, 3_000_000);
        let id1 = clip1.id;
        let id2 = clip2.id;

        timeline.track_mut(vid).unwrap().add_clip(clip1);
        timeline.track_mut(vid).unwrap().add_clip(clip2);
        assert_eq!(timeline.total_clip_count(), 2);

        // Group clip1 & clip2 into a compound clip
        let compound_id = timeline
            .create_compound_clip(vid, &[id1, id2], "Scene Compound")
            .expect("Failed to create compound clip");

        assert_eq!(timeline.total_clip_count(), 1);
        let compound_clip = timeline.track(vid).unwrap().clip(compound_id).unwrap();
        assert!(compound_clip.is_compound());
        assert_eq!(compound_clip.position_us, 1_000_000);
        assert_eq!(compound_clip.duration_us(), 5_000_000);

        // Flatten back in place
        let restored_ids = timeline
            .flatten_compound_clip(vid, compound_id)
            .expect("Failed to flatten compound clip");

        assert_eq!(restored_ids.len(), 2);
        assert_eq!(timeline.total_clip_count(), 2);
        let clips = timeline.track(vid).unwrap().clips();
        assert_eq!(clips[0].id, id1);
        assert_eq!(clips[0].position_us, 1_000_000);
        assert_eq!(clips[1].id, id2);
        assert_eq!(clips[1].position_us, 3_000_000);
    }
}
