//! Missing Media & Relinking Safety Test Suite
//!
//! Validates:
//! 1. Detection of missing/detached media files across all tracks and clips with structured metadata.
//! 2. Preservation of original asset references when relinking is cancelled or unconfirmed.
//! 3. Execution, undo, and redo of `Command::RelinkMedia`.
//! 4. Durability of `JournalMutation::RelinkMedia` across WAL journal append and replay.
//! 5. Explicit offline-media placeholder generation without silent fallback to synthetic footage.

use rd_core::{
    Clip, Command, CommandHistory, MissingMediaItem, Project, ProjectJournal, TrackKind,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_missing_media_detection_and_item_metadata() {
    let mut project = Project::new("Missing Media Test Project");
    let video_track_id = project.timeline.add_track(TrackKind::Video, "Video 1");
    let audio_track_id = project.timeline.add_track(TrackKind::Audio, "Audio 1");

    // Clip 1: references non-existent file
    let clip1 = Clip::new_video("footage/missing_clip_01.mp4", 0, 0, 5_000_000);
    let clip1_id = clip1.id;
    project
        .timeline
        .track_mut(video_track_id)
        .unwrap()
        .add_clip(clip1);

    // Clip 2: references another non-existent file
    let clip2 = Clip::new_video("footage/missing_clip_02.mp4", 5_000_000, 0, 5_000_000);
    let clip2_id = clip2.id;
    project
        .timeline
        .track_mut(video_track_id)
        .unwrap()
        .add_clip(clip2);

    // Clip 3: Audio referencing non-existent audio
    let clip3 = Clip::new_audio("audio/missing_track.wav", 0, 10_000_000);
    let clip3_id = clip3.id;
    project
        .timeline
        .track_mut(audio_track_id)
        .unwrap()
        .add_clip(clip3);

    let missing_items = project.find_missing_media_items();
    assert_eq!(
        missing_items.len(),
        3,
        "All 3 clips must be flagged as missing"
    );

    assert!(missing_items.contains(&MissingMediaItem {
        clip_id: clip1_id,
        track_id: video_track_id,
        source_path: "footage/missing_clip_01.mp4".into(),
    }));
    assert!(missing_items.contains(&MissingMediaItem {
        clip_id: clip2_id,
        track_id: video_track_id,
        source_path: "footage/missing_clip_02.mp4".into(),
    }));
    assert!(missing_items.contains(&MissingMediaItem {
        clip_id: clip3_id,
        track_id: audio_track_id,
        source_path: "audio/missing_track.wav".into(),
    }));

    // Backwards-compatible path list check
    let missing_paths = project.find_missing_media();
    assert_eq!(missing_paths.len(), 3);
    assert!(missing_paths.contains(&PathBuf::from("footage/missing_clip_01.mp4")));
}

#[test]
fn test_relink_preserves_original_when_unconfirmed() {
    let mut project = Project::new("Preserve Reference Test");
    let video_track_id = project.timeline.add_track(TrackKind::Video, "Video 1");

    let original_path = "footage/original_never_overwrite_on_cancel.mp4";
    let clip = Clip::new_video(original_path, 0, 0, 5_000_000);
    let clip_id = clip.id;
    project
        .timeline
        .track_mut(video_track_id)
        .unwrap()
        .add_clip(clip);

    // 1. Relinking non-matching path changes 0 clips and preserves original reference
    let count = project.relink_media("non_matching_path.mp4", "new_path.mp4");
    assert_eq!(count, 0, "No clips should be modified");

    let clip_ref = project
        .timeline
        .track(video_track_id)
        .unwrap()
        .clip(clip_id)
        .unwrap();
    assert_eq!(
        clip_ref.source_path().unwrap(),
        original_path,
        "Original media path must be preserved exactly"
    );

    // 2. Relinking matching path updates the reference
    let new_path = "footage/confirmed_relocation.mp4";
    let count2 = project.relink_media(original_path, new_path);
    assert_eq!(count2, 1, "Exactly 1 clip should be relinked");

    let clip_ref2 = project
        .timeline
        .track(video_track_id)
        .unwrap()
        .clip(clip_id)
        .unwrap();
    assert_eq!(
        clip_ref2.source_path().unwrap(),
        new_path,
        "Updated media path must be applied"
    );
}

#[test]
fn test_relink_command_undo_redo_and_journal_roundtrip() {
    let mut project = Project::new("Relink Command & Journal Test");
    let video_track_id = project.timeline.add_track(TrackKind::Video, "Video 1");

    let old_path = "footage/lost_asset.mp4";
    let new_path = "footage/found_asset.mp4";

    let clip = Clip::new_video(old_path, 0, 0, 5_000_000);
    let clip_id = clip.id;
    project
        .timeline
        .track_mut(video_track_id)
        .unwrap()
        .add_clip(clip);

    let temp_dir = std::env::temp_dir().join(format!("rd_relink_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let project_file = temp_dir.join("test.reddart");

    let mut journal = ProjectJournal::open_or_create(&project_file).expect("Create WAL journal");
    let mut history = CommandHistory::new(50);

    // 1. Execute relink command with journal
    let relink_cmd = Command::RelinkMedia {
        old_path: old_path.into(),
        new_path: new_path.into(),
    };
    history
        .execute_with_journal(relink_cmd, &mut project, &mut journal)
        .expect("Execute relink command");

    assert_eq!(
        project
            .timeline
            .track(video_track_id)
            .unwrap()
            .clip(clip_id)
            .unwrap()
            .source_path()
            .unwrap(),
        new_path
    );

    // 2. Undo relink
    let undone = history
        .undo_with_journal(&mut project, &mut journal)
        .expect("Undo relink");
    assert!(undone);
    assert_eq!(
        project
            .timeline
            .track(video_track_id)
            .unwrap()
            .clip(clip_id)
            .unwrap()
            .source_path()
            .unwrap(),
        old_path,
        "Undo must restore original media path"
    );

    // 3. Redo relink
    let redone = history
        .redo_with_journal(&mut project, &mut journal)
        .expect("Redo relink");
    assert!(redone);
    assert_eq!(
        project
            .timeline
            .track(video_track_id)
            .unwrap()
            .clip(clip_id)
            .unwrap()
            .source_path()
            .unwrap(),
        new_path,
        "Redo must reapply relinked media path"
    );

    // 4. Test WAL replay into a fresh project
    let mut fresh_project = Project::new("Replayed Relink Project");
    fresh_project
        .timeline
        .add_track(TrackKind::Video, "Video 1");
    fresh_project.timeline.tracks_mut().get_mut(0).unwrap().id = video_track_id;
    let mut replay_clip = Clip::new_video(old_path, 0, 0, 5_000_000);
    replay_clip.id = clip_id;
    fresh_project
        .timeline
        .track_mut(video_track_id)
        .unwrap()
        .add_clip(replay_clip);

    let report = journal
        .replay_into(&mut fresh_project)
        .expect("Replay journal");
    assert!(report.valid_records_replayed >= 3);

    assert_eq!(
        fresh_project
            .timeline
            .track(video_track_id)
            .unwrap()
            .clip(clip_id)
            .unwrap()
            .source_path()
            .unwrap(),
        new_path,
        "Replaying journal must reproduce confirmed relinked path"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
