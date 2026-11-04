//! Phase W4.4: Hardening & Media Fuzz Testing Suite
//!
//! Verifies zero-crash resilience against extreme project geometries:
//! - 1000 overlapping clips
//! - Negative / out-of-bounds timestamps
//! - Non-standard speeds and edge cases
//! - Deep undo/redo cycles without leaks
//! - Malformed project JSON recovery

use rd_core::{Clip, CommandHistory, Project, TrackKind};

#[test]
fn test_fuzz_1000_clips_and_timeline_scrub() {
    let mut project = Project::new("Stress Project");
    let track_id = project
        .timeline
        .add_track(TrackKind::Video, "Video Stress Track");

    let track = project.timeline.track_mut(track_id).unwrap();
    for i in 0..1000 {
        let clip = Clip::new_video(format!("clip_{}.mp4", i), i * 100_000, 0, 500_000);
        track.add_clip(clip);
    }

    assert_eq!(project.timeline.total_clip_count(), 1000);

    // Scrub across 500 arbitrary points
    for step in 0..500 {
        let ts = step * 200_000;
        let active = project.timeline.active_tracks_at(ts);
        assert!(!active.is_empty());
    }

    // Out-of-bounds scrub tests
    assert_eq!(project.timeline.active_tracks_at(-100_000).len(), 0);
    assert_eq!(
        project.timeline.active_tracks_at(1_000_000_000_000).len(),
        0
    );
}

#[test]
fn test_fuzz_deep_undo_redo_cycles() {
    let mut project = Project::new("History Stress");
    let mut history = CommandHistory::new(500);

    let v1 = project.timeline.add_track(TrackKind::Video, "Track 1");

    // Push 200 commands
    for i in 0..200 {
        let clip = Clip::new_video(format!("clip_{}.mp4", i), i * 1_000_000, 0, 1_000_000);
        let cmd = rd_core::command::Command::AddClip { track_id: v1, clip };
        history.execute(cmd, &mut project).unwrap();
    }

    assert_eq!(project.timeline.total_clip_count(), 200);

    // Undo 150 times
    for _ in 0..150 {
        history.undo(&mut project).unwrap();
    }
    assert_eq!(project.timeline.total_clip_count(), 50);

    // Redo 100 times
    for _ in 0..100 {
        history.redo(&mut project).unwrap();
    }
    assert_eq!(project.timeline.total_clip_count(), 150);
}

#[test]
fn test_fuzz_speed_extremes_and_zero_division() {
    let mut clip = Clip::new_video("test.mp4", 0, 0, 10_000_000);

    // Ultra slow speed
    clip.speed = 0.0001;
    assert!(clip.duration_us() > 0);
    assert!(clip.duration_us() < i64::MAX);

    // High speed
    clip.speed = 100.0;
    assert_eq!(clip.duration_us(), 100_000);

    // Negative / zero safety clamp
    clip.speed = 0.0;
    assert!(clip.duration_us() > 0, "Zero speed must not divide by zero");

    clip.speed = -2.0;
    assert!(clip.duration_us() > 0, "Negative speed must be clamped");
}

#[test]
fn test_fuzz_malformed_json_recovery() {
    // Completely invalid JSON
    let bad_json = "{ name: not_valid_json, timeline: [ ";
    assert!(Project::from_json(bad_json).is_err());

    // Valid JSON but empty
    let empty_json = "{}";
    assert!(Project::from_json(empty_json).is_err());

    // Valid project round-trip
    let project = Project::new("Recovered Project");
    let json = project.to_json().unwrap();
    let recovered = Project::from_json(&json).unwrap();
    assert_eq!(recovered.name, "Recovered Project");
}
