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
        self.split_clip_at_with_id(clip_id, split_time_us, ClipId(uuid::Uuid::new_v4()))
    }

    /// Split a clip at a given timestamp using an explicit new clip ID.
    pub fn split_clip_at_with_id(
        &mut self,
        clip_id: ClipId,
        split_time_us: TimestampUs,
        new_id: ClipId,
    ) -> Option<ClipId> {
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
        second_half.id = new_id;
        second_half.position_us = split_time_us;

        // Calculate the offset into the source for the second half, accounting for speed
        let timeline_offset = split_time_us - clip_start;
        let source_offset = (timeline_offset as f64 * clip.speed).round() as i64;
        second_half.source_in_us = clip.source_in_us + source_offset;

        // Trim the first half
        self.clips[clip_index].source_out_us = clip.source_in_us + source_offset;

        // Partition keyframes between first and second half
        for track in self.clips[clip_index].keyframes.values_mut() {
            let kfs: Vec<crate::Keyframe> = track
                .keyframes()
                .iter()
                .filter(|k| k.time_us < timeline_offset)
                .cloned()
                .collect();
            *track = crate::KeyframeTrack::new(&track.parameter);
            for k in kfs {
                track.add_keyframe(k);
            }
        }
        for track in second_half.keyframes.values_mut() {
            let kfs: Vec<crate::Keyframe> = track
                .keyframes()
                .iter()
                .filter(|k| k.time_us >= timeline_offset)
                .map(|k| crate::Keyframe {
                    time_us: k.time_us - timeline_offset,
                    value: k.value,
                    easing: k.easing,
                })
                .collect();
            *track = crate::KeyframeTrack::new(&track.parameter);
            for k in kfs {
                track.add_keyframe(k);
            }
        }

        // Insert the second half
        let new_id = second_half.id;
        self.clips.insert(clip_index + 1, second_half);

        Some(new_id)
    }

    /// Ripple Trim In: adjusts the clip's head and ripples all downstream clips.
    ///
    /// Trimming in by `delta_us` (timeline microseconds):
    /// - `delta_us > 0`: cuts from head, shortening the clip. Downstream clips shift left (earlier) to close the gap.
    /// - `delta_us < 0`: extends head earlier in media, lengthening the clip. Downstream clips shift right (later).
    pub fn ripple_trim_in(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let pos = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or("Clip not found")?;

        let clip = &self.clips[pos];
        let speed = clip.speed.abs().max(0.0001);
        let source_delta = (delta_us as f64 * speed).round() as i64;
        let new_source_in = clip.source_in_us + source_delta;

        if new_source_in < 0 {
            return Err("Cannot trim in earlier than start of media");
        }
        if new_source_in >= clip.source_out_us {
            return Err("Invalid trim boundary: source in out of bounds");
        }

        let timeline_delta = (source_delta as f64 / speed).round() as i64;

        self.clips[pos].source_in_us = new_source_in;

        // Downstream clips shift to maintain contiguity without gaps:
        // Shortening head (delta_us > 0) pulls downstream clips left (-timeline_delta)
        // Lengthening head (delta_us < 0) pushes downstream clips right (+abs(timeline_delta))
        for downstream in &mut self.clips[pos + 1..] {
            downstream.position_us = (downstream.position_us - timeline_delta).max(0);
        }
        Ok(())
    }

    /// Ripple Trim Out: adjusts the clip's tail and ripples all downstream clips.
    ///
    /// Trimming out by `delta_us` (timeline microseconds):
    /// - `delta_us > 0`: extends tail later in media, lengthening the clip. Downstream clips push right.
    /// - `delta_us < 0`: cuts tail, shortening the clip. Downstream clips pull left.
    pub fn ripple_trim_out(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let pos = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or("Clip not found")?;

        let clip = &self.clips[pos];
        let speed = clip.speed.abs().max(0.0001);
        let source_delta = (delta_us as f64 * speed).round() as i64;
        let new_source_out = clip.source_out_us + source_delta;

        if new_source_out <= clip.source_in_us {
            return Err("Invalid trim boundary: source out cannot precede source in");
        }

        let timeline_delta = (source_delta as f64 / speed).round() as i64;

        self.clips[pos].source_out_us = new_source_out;

        // Downstream clips shift to stay attached to the adjusted tail
        for downstream in &mut self.clips[pos + 1..] {
            downstream.position_us = (downstream.position_us + timeline_delta).max(0);
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

        let out_speed = out_clip.speed.abs().max(0.0001);
        let in_speed = in_clip.speed.abs().max(0.0001);

        let out_source_delta = (delta_us as f64 * out_speed).round() as i64;
        let in_source_delta = (delta_us as f64 * in_speed).round() as i64;

        let new_out_source_out = out_clip.source_out_us + out_source_delta;
        let new_in_source_in = in_clip.source_in_us + in_source_delta;

        if new_out_source_out <= out_clip.source_in_us {
            return Err("Outgoing clip cannot be rolled to zero duration");
        }
        if new_in_source_in >= in_clip.source_out_us || new_in_source_in < 0 {
            return Err("Incoming clip cannot be rolled beyond source boundaries");
        }

        let timeline_delta = (out_source_delta as f64 / out_speed).round() as i64;
        let new_in_pos = (self.clips[in_idx].position_us + timeline_delta).max(0);

        self.clips[out_idx].source_out_us = new_out_source_out;
        self.clips[in_idx].source_in_us = new_in_source_in;
        self.clips[in_idx].position_us = new_in_pos;

        Ok(())
    }

    /// Slip Edit: shifts source in/out while keeping timeline position and duration constant.
    pub fn slip_edit(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        let clip = self.clip_mut(clip_id).ok_or("Clip not found")?;
        let speed = clip.speed.abs().max(0.0001);
        let source_delta = (delta_us as f64 * speed).round() as i64;

        let new_in = clip.source_in_us + source_delta;
        let new_out = clip.source_out_us + source_delta;
        if new_in < 0 {
            return Err("Cannot slip earlier than beginning of media");
        }
        clip.source_in_us = new_in;
        clip.source_out_us = new_out;
        Ok(())
    }

    /// Slide Edit: moves clip on timeline while trimming adjacent clips to preserve sequence duration.
    ///
    /// Requires an adjacent predecessor and successor clip.
    /// Positive delta slides clip to the right (extending predecessor, shrinking successor).
    /// Negative delta slides clip to the left (shrinking predecessor, extending successor).
    pub fn slide_edit(&mut self, clip_id: ClipId, delta_us: i64) -> Result<(), &'static str> {
        if self.locked {
            return Err("Track is locked");
        }
        if delta_us == 0 {
            return Ok(());
        }

        let pos = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or("Clip not found")?;

        if pos == 0 {
            return Err("Cannot slide clip: predecessor clip required");
        }
        if pos + 1 >= self.clips.len() {
            return Err("Cannot slide clip: successor clip required");
        }

        let prev = &self.clips[pos - 1];
        let target = &self.clips[pos];
        let next = &self.clips[pos + 1];

        let prev_speed = prev.speed.abs().max(0.0001);
        let next_speed = next.speed.abs().max(0.0001);

        let prev_source_delta = (delta_us as f64 * prev_speed).round() as i64;
        let next_source_delta = (delta_us as f64 * next_speed).round() as i64;

        let new_prev_source_out = prev.source_out_us + prev_source_delta;
        if new_prev_source_out <= prev.source_in_us {
            return Err("Predecessor clip cannot shrink below minimum duration");
        }

        let new_next_source_in = next.source_in_us + next_source_delta;
        if new_next_source_in >= next.source_out_us {
            return Err("Successor clip cannot shrink below minimum duration");
        }
        if new_next_source_in < 0 {
            return Err("Successor clip cannot be extended earlier than start of media");
        }

        let new_target_pos = target.position_us + delta_us;
        if new_target_pos < 0 {
            return Err("Cannot slide clip before start of timeline");
        }

        let new_next_pos = next.position_us + delta_us;
        if new_next_pos < 0 {
            return Err("Successor clip position cannot precede timeline start");
        }

        // Apply mutations
        self.clips[pos - 1].source_out_us = new_prev_source_out;
        self.clips[pos].position_us = new_target_pos;
        self.clips[pos + 1].source_in_us = new_next_source_in;
        self.clips[pos + 1].position_us = new_next_pos;

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

    /// Ripple Insert: inserts a clip at index and shifts downstream clips forward by its duration.
    pub fn ripple_insert(&mut self, clip: Clip, at_index: usize) {
        let duration = clip.duration_us();
        let idx = at_index.min(self.clips.len());
        for c in &mut self.clips[idx..] {
            c.position_us += duration;
        }
        self.clips.insert(idx, clip);
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
            retime_mode: crate::clip::RetimeMode::default(),
            opacity: 1.0,
            transform: crate::clip::Transform2D::default(),
            effects: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            pan_surround: 0.0,
            lfe_send: 0.0,
            keyframes: std::collections::HashMap::new(),
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

        // Ripple trim in on clip A by +500ms (shortening head by 500ms)
        assert!(track.ripple_trim_in(id_a, 500_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_in_us, 500_000);
        assert_eq!(track.clip(id_a).unwrap().position_us, 0); // Position at track start remains 0
        assert_eq!(track.clip(id_a).unwrap().duration_us(), 1_500_000); // 2.0s - 0.5s = 1.5s
        assert_eq!(track.clip(id_b).unwrap().position_us, 1_500_000); // Downstream pulled backward to close gap!

        // Ripple trim in on clip A by -200ms (extending head earlier by 200ms)
        assert!(track.ripple_trim_in(id_a, -200_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_in_us, 300_000);
        assert_eq!(track.clip(id_a).unwrap().duration_us(), 1_700_000); // 1.5s + 0.2s = 1.7s
        assert_eq!(track.clip(id_b).unwrap().position_us, 1_700_000); // Downstream pushed forward to accommodate extension!

        // Ripple trim out on clip A by -200ms (shortening tail)
        assert!(track.ripple_trim_out(id_a, -200_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_out_us, 1_800_000);
        assert_eq!(track.clip(id_a).unwrap().duration_us(), 1_500_000);
        assert_eq!(track.clip(id_b).unwrap().position_us, 1_500_000); // Downstream pulled backward to close gap!

        // Ripple trim out on clip A by +300ms (extending tail)
        assert!(track.ripple_trim_out(id_a, 300_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_out_us, 2_100_000);
        assert_eq!(track.clip(id_a).unwrap().duration_us(), 1_800_000);
        assert_eq!(track.clip(id_b).unwrap().position_us, 1_800_000); // Downstream pushed forward!
    }

    #[test]
    fn test_ripple_trim_with_non_unity_speed() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        // Clip A at 2.0x speed: 2s source = 1s timeline
        let mut clip_a = make_test_clip(0, 2_000_000);
        clip_a.speed = 2.0;
        assert_eq!(clip_a.duration_us(), 1_000_000);
        let id_a = clip_a.id;

        // Clip B at 0.5x speed: 1s source = 2s timeline
        let mut clip_b = make_test_clip(1_000_000, 1_000_000);
        clip_b.speed = 0.5;
        assert_eq!(clip_b.duration_us(), 2_000_000);
        let id_b = clip_b.id;

        track.add_clip(clip_a);
        track.add_clip(clip_b);

        // Ripple trim in on clip A by 250ms timeline time:
        // Source delta = 250ms * 2.0x = 500ms
        assert!(track.ripple_trim_in(id_a, 250_000).is_ok());
        assert_eq!(track.clip(id_a).unwrap().source_in_us, 500_000);
        assert_eq!(track.clip(id_a).unwrap().duration_us(), 750_000);
        // Downstream clip B must be pulled left by 250ms timeline time
        assert_eq!(track.clip(id_b).unwrap().position_us, 750_000);
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
        // Clip C has 1s of head media available (source 1s..3s)
        let mut clip_c = make_test_clip(4_000_000, 2_000_000);
        clip_c.source_in_us = 1_000_000;
        clip_c.source_out_us = 3_000_000;

        let id_b = clip_b.id;
        track.add_clip(clip_a);
        track.add_clip(clip_b);
        track.add_clip(clip_c);

        let initial_dur = track.duration_us();

        // Slide middle clip B by +400ms (positive delta: moves right)
        assert!(track.slide_edit(id_b, 400_000).is_ok());
        assert_eq!(track.clips()[0].duration_us(), 2_400_000); // Clip A lengthened
        assert_eq!(track.clips()[1].position_us, 2_400_000); // Clip B shifted
        assert_eq!(track.clips()[1].duration_us(), 2_000_000); // Clip B duration untouched!
        assert_eq!(track.clips()[2].position_us, 4_400_000); // Clip C shifted
        assert_eq!(track.clips()[2].duration_us(), 1_600_000); // Clip C shortened
        assert_eq!(track.duration_us(), initial_dur); // Total duration invariant

        // Slide middle clip B by -600ms (negative delta: moves left)
        assert!(track.slide_edit(id_b, -600_000).is_ok());
        assert_eq!(track.clips()[0].duration_us(), 1_800_000); // Clip A shortened
        assert_eq!(track.clips()[1].position_us, 1_800_000); // Clip B shifted left
        assert_eq!(track.clips()[1].duration_us(), 2_000_000); // Clip B duration untouched!
        assert_eq!(track.clips()[2].position_us, 3_800_000); // Clip C shifted left
        assert_eq!(track.clips()[2].duration_us(), 2_200_000); // Clip C lengthened
        assert_eq!(track.duration_us(), initial_dur); // Total duration invariant

        // Edge case: slide first clip (no predecessor) must fail
        assert!(track.slide_edit(track.clips()[0].id, 100_000).is_err());
        // Edge case: slide last clip (no successor) must fail
        assert!(track.slide_edit(track.clips()[2].id, 100_000).is_err());
        // Edge case: excessive delta that would shrink predecessor below 0 must fail
        assert!(track.slide_edit(id_b, -2_000_000).is_err());
    }

    #[test]
    fn test_slide_edit_with_non_unity_speeds() {
        let mut track = Track::new(TrackId(1), TrackKind::Video, "V1");
        // Clip A: 2.0x speed, 4s source -> 2s timeline
        let mut clip_a = make_test_clip(0, 4_000_000);
        clip_a.speed = 2.0;
        // Clip B: 1.0x speed, 2s source -> 2s timeline
        let clip_b = make_test_clip(2_000_000, 2_000_000);
        let id_b = clip_b.id;
        // Clip C: 0.5x speed, 1s source -> 2s timeline
        let mut clip_c = make_test_clip(4_000_000, 1_000_000);
        clip_c.speed = 0.5;

        track.add_clip(clip_a);
        track.add_clip(clip_b);
        track.add_clip(clip_c);

        let initial_dur = track.duration_us();
        assert_eq!(initial_dur, 6_000_000);

        // Slide B by +500ms
        assert!(track.slide_edit(id_b, 500_000).is_ok());
        // Predecessor A at 2.0x speed: +500ms timeline requires +1000ms source
        assert_eq!(track.clips()[0].source_out_us, 5_000_000);
        assert_eq!(track.clips()[0].duration_us(), 2_500_000);
        assert_eq!(track.clips()[1].position_us, 2_500_000);
        // Successor C at 0.5x speed: -500ms timeline requires +250ms source in
        assert_eq!(track.clips()[2].source_in_us, 250_000);
        assert_eq!(track.clips()[2].duration_us(), 1_500_000);
        // Total duration invariant strictly preserved
        assert_eq!(track.duration_us(), initial_dur);
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
