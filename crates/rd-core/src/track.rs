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

    /// Ripple Trim In: adjusts the clip's head and ripples all downstream clips.
    pub fn ripple_trim_in(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let pos = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or("Clip not found")?;

        let clip = &mut self.clips[pos];
        let new_source_in = clip.source_in_us + delta_us;
        if new_source_in < 0 || new_source_in >= clip.source_out_us {
            return Err("Invalid trim boundary: source in out of bounds");
        }
        let new_pos = (clip.position_us + delta_us).max(0);
        clip.source_in_us = new_source_in;
        clip.position_us = new_pos;

        // Shift downstream clips
        for downstream in &mut self.clips[pos + 1..] {
            downstream.position_us = (downstream.position_us + delta_us).max(0);
        }
        Ok(())
    }

    /// Ripple Trim Out: adjusts the clip's tail and ripples all downstream clips.
    pub fn ripple_trim_out(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let pos = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or("Clip not found")?;

        let clip = &mut self.clips[pos];
        let new_source_out = clip.source_out_us + delta_us;
        if new_source_out <= clip.source_in_us {
            return Err("Invalid trim boundary: source out cannot precede source in");
        }
        clip.source_out_us = new_source_out;

        // Shift downstream clips
        for downstream in &mut self.clips[pos + 1..] {
            downstream.position_us = (downstream.position_us + delta_us).max(0);
        }
        Ok(())
    }

    /// Roll Edit: dual-sided edit between two adjacent clips preserving total sequence duration.
    pub fn roll_edit(
        &mut self,
        outgoing_id: ClipId,
        incoming_id: ClipId,
        delta_us: i64,
    ) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let out_idx = self
            .clips
            .iter()
            .position(|c| c.id == outgoing_id)
            .ok_or("Outgoing clip not found")?;
        let in_idx = self
            .clips
            .iter()
            .position(|c| c.id == incoming_id)
            .ok_or("Incoming clip not found")?;

        if out_idx + 1 != in_idx {
            return Err("Clips must be immediately adjacent for roll edit");
        }

        let out_clip = &self.clips[out_idx];
        let in_clip = &self.clips[in_idx];

        let new_out_source_out = out_clip.source_out_us + delta_us;
        let new_in_source_in = in_clip.source_in_us + delta_us;

        if new_out_source_out <= out_clip.source_in_us {
            return Err("Outgoing clip cannot be rolled to zero duration");
        }
        if new_in_source_in >= in_clip.source_out_us || new_in_source_in < 0 {
            return Err("Incoming clip cannot be rolled beyond source boundaries");
        }

        self.clips[out_idx].source_out_us = new_out_source_out;
        self.clips[in_idx].source_in_us = new_in_source_in;
        self.clips[in_idx].position_us = (self.clips[in_idx].position_us + delta_us).max(0);

        Ok(())
    }

    /// Slip Edit: shifts source in/out while keeping timeline position and duration constant.
    pub fn slip_edit(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let clip = self.clip_mut(clip_id).ok_or("Clip not found")?;
        let new_in = clip.source_in_us + delta_us;
        let new_out = clip.source_out_us + delta_us;
        if new_in < 0 {
            return Err("Cannot slip earlier than beginning of media");
        }
        clip.source_in_us = new_in;
        clip.source_out_us = new_out;
        Ok(())
    }

    /// Slide Edit: moves clip on timeline while trimming adjacent clips to preserve sequence duration.
    pub fn slide_edit(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let pos = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or("Clip not found")?;

        if pos > 0 {
            let prev = &self.clips[pos - 1];
            let new_prev_out = prev.source_out_us + delta_us;
            if new_prev_out <= prev.source_in_us {
                return Err("Predecessor clip cannot shrink below minimum duration");
            }
        }

        if pos + 1 < self.clips.len() {
            let next = &self.clips[pos + 1];
            let new_next_in = next.source_in_us + delta_us;
            if new_next_in >= next.source_out_us || new_next_in < 0 {
                return Err("Successor clip cannot shrink below minimum duration");
            }
        }

        if pos > 0 {
            self.clips[pos - 1].source_out_us += delta_us;
        }

        self.clips[pos].position_us = (self.clips[pos].position_us + delta_us).max(0);

        if pos + 1 < self.clips.len() {
            self.clips[pos + 1].source_in_us += delta_us;
            self.clips[pos + 1].position_us = (self.clips[pos + 1].position_us + delta_us).max(0);
        }

        Ok(())
    }

    /// Ripple Delete: removes a clip and shifts all downstream clips left to close the gap.
    pub fn ripple_delete(&mut self, clip_id: ClipId) -> Option<(Clip, usize)> {
        if self.locked {
            return None;
        }
        let pos = self.clips.iter().position(|c| c.id == clip_id)?;
        let removed = self.clips.remove(pos);
        let duration = removed.duration_us();

        for c in &mut self.clips[pos..] {
            c.position_us = c.position_us.saturating_sub(duration);
        }

        Some((removed, pos))
    }

    /// Duplicate Clip: clones clip and places it directly adjacent.
    pub fn duplicate_clip(&mut self, clip_id: ClipId) -> Option<ClipId> {
        if self.locked {
            return None;
        }
        let clip = self.clip(clip_id)?.clone();
        let new_id = ClipId(uuid::Uuid::new_v4());
        let mut dup = clip;
        dup.id = new_id;
        dup.position_us += dup.duration_us();
        self.add_clip(dup);
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

    #[test]
    fn test_ripple_trim_invariants() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip_a = make_test_clip(0, 2_000_000);
        let clip_b = make_test_clip(2_000_000, 3_000_000);
        let id_a = clip_a.id;
        let id_b = clip_b.id;
        track.add_clip(clip_a);
        track.add_clip(clip_b);

        // Ripple trim in on clip A by +500ms
        assert!(track.ripple_trim_in(id_a, 500_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_in_us, 500_000);
        assert_eq!(track.clip(id_a).unwrap().position_us, 500_000);
        assert_eq!(track.clip(id_b).unwrap().position_us, 2_500_000); // Downstream rippled forward

        // Ripple trim out on clip A by -200ms
        assert!(track.ripple_trim_out(id_a, -200_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_out_us, 1_800_000);
        assert_eq!(track.clip(id_b).unwrap().position_us, 2_300_000); // Downstream rippled backward
    }

    #[test]
    fn test_roll_edit_invariants() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip_a = make_test_clip(0, 2_000_000);
        let clip_b = make_test_clip(2_000_000, 2_000_000);
        let id_a = clip_a.id;
        let id_b = clip_b.id;
        track.add_clip(clip_a);
        track.add_clip(clip_b);

        let initial_total_duration = track.duration_us();

        // Roll cut point by +500ms
        assert!(track.roll_edit(id_a, id_b, 500_000).is_ok());
        // Clip A expanded by 500ms
        assert_eq!(track.clip(id_a).unwrap().duration_us(), 2_500_000);
        // Clip B contracted by 500ms and starts at 2.5s
        assert_eq!(track.clip(id_b).unwrap().position_us, 2_500_000);
        assert_eq!(track.clip(id_b).unwrap().duration_us(), 1_500_000);
        // Sequence total duration invariant preserved!
        assert_eq!(track.duration_us(), initial_total_duration);
    }

    #[test]
    fn test_slip_edit_invariants() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip = make_test_clip(1_000_000, 2_000_000);
        let cid = clip.id;
        track.add_clip(clip);

        let orig_pos = track.clip(cid).unwrap().position_us;
        let orig_dur = track.clip(cid).unwrap().duration_us();

        // Slip source forward by 300ms
        assert!(track.slip_edit(cid, 300_000).is_ok());
        let updated = track.clip(cid).unwrap();
        assert_eq!(updated.source_in_us, 300_000);
        assert_eq!(updated.source_out_us, 2_300_000);
        // Timeline position and duration invariant strictly preserved
        assert_eq!(updated.position_us, orig_pos);
        assert_eq!(updated.duration_us(), orig_dur);
    }

    #[test]
    fn test_slide_edit_invariants() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip_a = make_test_clip(0, 2_000_000);
        let clip_b = make_test_clip(2_000_000, 2_000_000);
        let clip_c = make_test_clip(4_000_000, 2_000_000);
        let id_b = clip_b.id;
        track.add_clip(clip_a);
        track.add_clip(clip_b);
        track.add_clip(clip_c);

        let initial_dur = track.duration_us();

        // Slide middle clip B by +400ms
        assert!(track.slide_edit(id_b, 400_000).is_ok());
        assert_eq!(track.clips()[0].duration_us(), 2_400_000); // Clip A lengthened
        assert_eq!(track.clips()[1].position_us, 2_400_000); // Clip B shifted
        assert_eq!(track.clips()[1].duration_us(), 2_000_000); // Clip B duration untouched!
        assert_eq!(track.clips()[2].position_us, 4_400_000); // Clip C shifted
        assert_eq!(track.clips()[2].duration_us(), 1_600_000); // Clip C shortened
        assert_eq!(track.duration_us(), initial_dur); // Total duration invariant
    }

    #[test]
    fn test_ripple_delete_and_duplicate() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        let clip_a = make_test_clip(0, 1_000_000);
        let clip_b = make_test_clip(1_000_000, 2_000_000);
        let clip_c = make_test_clip(3_000_000, 1_000_000);
        let id_b = clip_b.id;
        track.add_clip(clip_a);
        track.add_clip(clip_b);
        track.add_clip(clip_c);

        // Ripple delete clip B (2s)
        let res = track.ripple_delete(id_b);
        assert!(res.is_some());
        assert_eq!(track.clips().len(), 2);
        assert_eq!(track.clips()[1].position_us, 1_000_000); // Clip C pulled back by 2s!

        // Duplicate clip A
        let dup_id = track.duplicate_clip(track.clips()[0].id);
        assert!(dup_id.is_some());
        assert_eq!(track.clips().len(), 3);
    }
}
