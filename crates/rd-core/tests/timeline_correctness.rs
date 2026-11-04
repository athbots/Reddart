//! RD-008: Timeline Edit Correctness Suite
//!
//! Property-style and boundary tests for all core editing operations:
//! - Trim In / Trim Out invariants
//! - Split clip at arbitrary timestamps
//! - Ripple delete simulation (shift downstream clips)
//! - Invariant: Undo all commands always returns to exact initial project snapshot
//! - Track reordering and cross-track consistency

use rd_core::{
    Clip, Project, TrackKind,
    command::{Command, CommandHistory},
};

#[test]
fn test_trim_in_and_out_invariants() {
    let mut project = Project::new("Trim Invariant");
    let tid = project.timeline.add_track(TrackKind::Video, "Video 1");

    // Clip starting at 2s, source 0s to 10s, speed 1.0 -> duration 10s
    let clip = Clip::new_video("test.mp4", 2_000_000, 0, 10_000_000);
    let cid = clip.id;
    project.timeline.track_mut(tid).unwrap().add_clip(clip);

    let initial_json = project.to_json().unwrap();
    let mut history = CommandHistory::new(100);

    // 1. Trim In: Move in-point from 0s to 3s (clip moves from 2s to 5s, duration becomes 7s)
    let trim_in_cmd = Command::TrimIn {
        track_id: tid,
        clip_id: cid,
        old_source_in_us: 0,
        new_source_in_us: 3_000_000,
        old_position_us: 2_000_000,
        new_position_us: 5_000_000,
    };
    history.execute(trim_in_cmd, &mut project).unwrap();

    let clip = project.timeline.track(tid).unwrap().clip(cid).unwrap();
    assert_eq!(clip.source_in_us, 3_000_000);
    assert_eq!(clip.position_us, 5_000_000);
    assert_eq!(clip.duration_us(), 7_000_000);

    // 2. Trim Out: Move out-point from 10s to 8s (duration becomes 5s)
    let trim_out_cmd = Command::TrimOut {
        track_id: tid,
        clip_id: cid,
        old_source_out_us: 10_000_000,
        new_source_out_us: 8_000_000,
    };
    history.execute(trim_out_cmd, &mut project).unwrap();

    let clip = project.timeline.track(tid).unwrap().clip(cid).unwrap();
    assert_eq!(clip.source_out_us, 8_000_000);
    assert_eq!(clip.duration_us(), 5_000_000);

    // 3. Undo Trim Out
    history.undo(&mut project).unwrap();
    let clip = project.timeline.track(tid).unwrap().clip(cid).unwrap();
    assert_eq!(clip.source_out_us, 10_000_000);

    // 4. Undo Trim In
    history.undo(&mut project).unwrap();
    let restored_json = project.to_json().unwrap();

    assert_eq!(
        initial_json, restored_json,
        "Undo must restore exact initial state"
    );
}

#[test]
fn test_split_clip_boundary_conditions() {
    let mut project = Project::new("Split Test");
    let tid = project.timeline.add_track(TrackKind::Video, "Video 1");
    let track = project.timeline.track_mut(tid).unwrap();

    // 4-second clip placed at 1s (covers 1s to 5s)
    let clip = Clip::new_video("test.mp4", 1_000_000, 0, 4_000_000);
    let cid = clip.id;
    track.add_clip(clip);

    // Attempt invalid split BEFORE clip start (< 1s)
    assert!(track.split_clip_at(cid, 500_000).is_none());

    // Attempt invalid split EXACTLY at clip start (= 1s)
    assert!(track.split_clip_at(cid, 1_000_000).is_none());

    // Attempt invalid split AFTER clip end (> 5s)
    assert!(track.split_clip_at(cid, 6_000_000).is_none());

    // Attempt invalid split EXACTLY at clip end (= 5s)
    assert!(track.split_clip_at(cid, 5_000_000).is_none());

    // Valid split at 2.5s (1.5s into clip)
    let new_cid = track
        .split_clip_at(cid, 2_500_000)
        .expect("Split should succeed");
    assert_eq!(track.clips().len(), 2);

    let first = track.clip(cid).unwrap();
    assert_eq!(first.position_us, 1_000_000);
    assert_eq!(first.duration_us(), 1_500_000);
    assert_eq!(first.source_in_us, 0);
    assert_eq!(first.source_out_us, 1_500_000);

    let second = track.clip(new_cid).unwrap();
    assert_eq!(second.position_us, 2_500_000);
    assert_eq!(second.duration_us(), 2_500_000);
    assert_eq!(second.source_in_us, 1_500_000);
    assert_eq!(second.source_out_us, 4_000_000);
}

#[test]
fn test_ripple_delete_compound_operation() {
    let mut project = Project::new("Ripple Delete");
    let tid = project.timeline.add_track(TrackKind::Video, "Video 1");

    // Place 3 clips sequentially:
    // Clip A: 0s..2s (dur 2s)
    // Clip B: 2s..5s (dur 3s)
    // Clip C: 5s..8s (dur 3s)
    let clip_a = Clip::new_video("A.mp4", 0, 0, 2_000_000);
    let clip_b = Clip::new_video("B.mp4", 2_000_000, 0, 3_000_000);
    let clip_c = Clip::new_video("C.mp4", 5_000_000, 0, 3_000_000);

    let cid_b = clip_b.id;
    let cid_c = clip_c.id;

    let track = project.timeline.track_mut(tid).unwrap();
    track.add_clip(clip_a);
    track.add_clip(clip_b.clone());
    track.add_clip(clip_c.clone());

    let initial_json = project.to_json().unwrap();
    let mut history = CommandHistory::new(50);

    // Ripple delete Clip B:
    // Remove Clip B (dur 3s), then Move Clip C from 5s to 2s
    let ripple_delete_cmd = Command::Compound {
        label: "Ripple Delete Clip B".into(),
        commands: vec![
            Command::RemoveClip {
                track_id: tid,
                clip: clip_b,
            },
            Command::MoveClip {
                track_id: tid,
                clip_id: cid_c,
                old_position_us: 5_000_000,
                new_position_us: 2_000_000,
            },
        ],
    };

    history.execute(ripple_delete_cmd, &mut project).unwrap();

    let track = project.timeline.track(tid).unwrap();
    assert_eq!(track.clips().len(), 2);
    assert!(track.clip(cid_b).is_none());
    let c = track.clip(cid_c).unwrap();
    assert_eq!(c.position_us, 2_000_000, "Clip C should ripple back to 2s");
    assert_eq!(project.timeline.duration_us(), 5_000_000);

    // Undo Ripple Delete
    history.undo(&mut project).unwrap();
    assert_eq!(project.timeline.track(tid).unwrap().clips().len(), 3);
    let c = project.timeline.track(tid).unwrap().clip(cid_c).unwrap();
    assert_eq!(c.position_us, 5_000_000, "Clip C restored to 5s");

    let restored_json = project.to_json().unwrap();
    assert_eq!(
        initial_json, restored_json,
        "Full undo returns to original state"
    );
}

#[test]
fn test_track_reorder_and_mute_solo() {
    let mut project = Project::new("Track Management");
    let v1 = project.timeline.add_track(TrackKind::Video, "V1");
    let v2 = project.timeline.add_track(TrackKind::Video, "V2");
    let a1 = project.timeline.add_track(TrackKind::Audio, "A1");

    assert_eq!(project.timeline.tracks()[0].id, v1);
    assert_eq!(project.timeline.tracks()[1].id, v2);
    assert_eq!(project.timeline.tracks()[2].id, a1);

    // Move audio track to the top
    project.timeline.move_track(2, 0);
    assert_eq!(project.timeline.tracks()[0].id, a1);
    assert_eq!(project.timeline.tracks()[1].id, v1);
    assert_eq!(project.timeline.tracks()[2].id, v2);

    // Mute/Solo track controls
    project.timeline.track_mut(a1).unwrap().enabled = false;
    assert!(!project.timeline.track(a1).unwrap().enabled);
}
