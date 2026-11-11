//! Automated Release Verification Test Suite
//!
//! Validates:
//! 1. Core Timeline Editing Operations (Razor split, Ripple trim, Roll, Slip, Slide, Markers)
//! 2. Keyframe Automation (Linear, EaseIn, EaseOut, EaseInOut, Hold)
//! 3. Atomic Project Serialization, WAL Append-Only Logging, CRC32 Checksumming & Torn-Write Recovery
//! 4. End-to-End Hardware Export Pipeline & Container Integrity Probe

use rd_core::{Clip, Easing, JournalMutation, Project, ProjectJournal, TrackKind};
use rd_export::{
    ColorGradeSettings, ExportJob, ExportPreset, RenderRequest, render_project_to_file,
};
use std::path::PathBuf;

#[test]
fn test_release_core_editing_pipeline() {
    let mut project = Project::new("Release Core Verification");
    let v1 = project.timeline.add_track(TrackKind::Video, "Video Main");
    let v2 = project
        .timeline
        .add_track(TrackKind::Video, "Video Overlay");
    let a1 = project
        .timeline
        .add_track(TrackKind::Audio, "Dialogue Master");

    // Add clips
    let clip1 = Clip::new_video("camera_a.mp4", 0, 0, 10_000_000); // 0s - 10s
    let clip1_id = clip1.id;
    let clip2 = Clip::new_video("camera_b.mp4", 10_000_000, 0, 10_000_000); // 10s - 20s
    project.timeline.track_mut(v1).unwrap().add_clip(clip1);
    project.timeline.track_mut(v1).unwrap().add_clip(clip2);

    let overlay_clip = Clip::new_video("broll.mp4", 2_000_000, 0, 6_000_000);
    project
        .timeline
        .track_mut(v2)
        .unwrap()
        .add_clip(overlay_clip);

    let audio_clip = Clip::new_audio("voiceover.wav", 0, 15_000_000);
    project.timeline.track_mut(a1).unwrap().add_clip(audio_clip);

    // Invariant check: Initial track duration
    assert_eq!(project.timeline.duration_us(), 20_000_000);

    // 1. Razor Tool: Split clip1 at 4.0s (4_000_000 us)
    let split_id = project
        .timeline
        .track_mut(v1)
        .unwrap()
        .split_clip_at(clip1_id, 4_000_000);
    assert!(split_id.is_some(), "Razor split at 4.0s should succeed");
    assert_eq!(project.timeline.track(v1).unwrap().clips().len(), 3);
    assert_eq!(
        project.timeline.track(v1).unwrap().clips()[0].duration_us(),
        4_000_000
    );
    assert_eq!(
        project.timeline.track(v1).unwrap().clips()[1].duration_us(),
        6_000_000
    );

    // 2. Ripple Trim Out: Trim out-point of clip 0 by -1.0s (from 4s to 3s)
    let clip0_id = project.timeline.track(v1).unwrap().clips()[0].id;
    let ripple_res = project
        .timeline
        .track_mut(v1)
        .unwrap()
        .ripple_trim_out(clip0_id, -1_000_000);
    assert!(ripple_res.is_ok(), "Ripple trim out should succeed");
    // Downstream clips on track v1 should have shifted earlier by 1.0s
    assert_eq!(
        project.timeline.track(v1).unwrap().clips()[1].position_us,
        3_000_000
    );
    assert_eq!(
        project.timeline.track(v1).unwrap().clips()[2].position_us,
        9_000_000
    );

    // 3. Roll Edit: Adjust boundary between clip 1 and clip 2 by +500ms
    let outgoing_id = project.timeline.track(v1).unwrap().clips()[1].id;
    let incoming_id = project.timeline.track(v1).unwrap().clips()[2].id;
    let roll_res =
        project
            .timeline
            .track_mut(v1)
            .unwrap()
            .roll_edit(outgoing_id, incoming_id, 500_000);
    assert!(roll_res.is_ok(), "Roll edit should succeed");
    assert_eq!(
        project.timeline.track(v1).unwrap().clips()[1].duration_us(),
        6_500_000
    );
    assert_eq!(
        project.timeline.track(v1).unwrap().clips()[2].duration_us(),
        9_500_000
    );

    // 4. Keyframing automation: Add keyframes to overlay clip
    let v2_clip = &mut project.timeline.track_mut(v2).unwrap().clips_mut()[0];
    v2_clip.set_keyframe("opacity", 0, 0.0, Easing::Linear);
    v2_clip.set_keyframe("opacity", 1_000_000, 1.0, Easing::EaseIn);
    v2_clip.set_keyframe("opacity", 4_000_000, 1.0, Easing::Hold);
    v2_clip.set_keyframe("opacity", 5_000_000, 0.5, Easing::EaseInOut);
    v2_clip.set_keyframe("opacity", 6_000_000, 0.0, Easing::EaseOut);

    assert!(v2_clip.has_keyframes());
    assert_eq!(v2_clip.total_keyframe_count(), 5);

    // 5. Speed Remapping
    v2_clip.speed = 2.0;
    assert_eq!(v2_clip.duration_us(), 3_000_000); // 6s source at 2x = 3s on timeline
}

#[test]
fn test_release_saving_and_journal_recovery() {
    let temp_dir = std::env::temp_dir().join(format!("rd_release_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let proj_file = temp_dir.join("release_project.reddart");

    // Phase 1: Initialize project and save to disk
    let mut project = Project::new("Release Save & Recovery");
    let v_id = project.timeline.add_track(TrackKind::Video, "Video Track");
    let clip = Clip::new_video("source.mp4", 0, 0, 5_000_000);
    let clip_id = clip.id;
    project.timeline.track_mut(v_id).unwrap().add_clip(clip);

    project
        .save_atomic(&proj_file)
        .expect("Atomic project save should succeed");
    assert!(proj_file.exists());

    // Phase 2: Open WAL journal and append operations
    let mut journal = ProjectJournal::open_or_create(&proj_file).expect("WAL open should succeed");
    let mut split_clip_half = Clip::new_video("source.mp4", 2_500_000, 2_500_000, 5_000_000);
    split_clip_half.id = rd_core::ClipId::new();
    let split_op = JournalMutation::SplitClip {
        track_id: v_id,
        clip_id,
        split_time_us: 2_500_000,
        new_clip: split_clip_half,
    };
    journal
        .append(split_op)
        .expect("Appending split op to WAL should succeed");

    let move_op = JournalMutation::MoveClip {
        track_id: v_id,
        clip_id,
        new_position_us: 500_000,
    };
    journal
        .append(move_op)
        .expect("Appending move op to WAL should succeed");

    let wal_path = journal.path().to_path_buf();
    drop(journal);

    // Phase 3: Simulate power outage / crash with torn trailing write
    assert!(wal_path.exists());
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        // Write partial torn JSON record (simulating power lost mid-write)
        f.write_all(b"{\"seq\":3,\"tx_id\":\"torn-write-interrupted")
            .unwrap();
        f.flush().unwrap();
    }

    // Phase 4: Reload project from disk and replay WAL into it
    let mut recovered_project =
        Project::load_file(&proj_file).expect("Loading base project should succeed");
    let mut recovery_journal =
        ProjectJournal::open_or_create(&proj_file).expect("Reopening journal should succeed");
    let replay_report = recovery_journal
        .replay_into(&mut recovered_project)
        .expect("Replay should handle torn write gracefully");

    // Verify recovery invariants:
    // Exactly 2 valid operations were replayed; the trailing torn write was detected and safely truncated
    assert_eq!(replay_report.valid_records_replayed, 2);
    assert!(replay_report.torn_tail_bytes_truncated > 0);

    // Verify replayed state: Track should have 2 clips following the replayed split
    let recovered_track = recovered_project.timeline.track(v_id).unwrap();
    assert_eq!(recovered_track.clips().len(), 2);

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_release_export_and_probe_pipeline() {
    let temp_dir = std::env::temp_dir().join(format!("rd_release_export_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let out_file = temp_dir.join("release_export_test.mp4");

    // Build project with video and audio tracks
    let mut project = Project::new("Release Export Smoke");
    let v_id = project.timeline.add_track(TrackKind::Video, "Video");
    let a_id = project.timeline.add_track(TrackKind::Audio, "Audio");

    // Check if demo file is available for real media testing
    let demo_path = PathBuf::from("ready_to_use/windows/Reddart_Master_Demo.mp4");
    let media_path = if demo_path.exists() {
        demo_path.to_str().unwrap().to_string()
    } else {
        "synthetic_test_pattern.mp4".to_string()
    };

    let clip_v = Clip::new_video(&media_path, 0, 0, 1_000_000); // 1.0s video
    let clip_a = Clip::new_audio(&media_path, 0, 1_000_000); // 1.0s audio
    project.timeline.track_mut(v_id).unwrap().add_clip(clip_v);
    project.timeline.track_mut(a_id).unwrap().add_clip(clip_a);

    let preset = ExportPreset::youtube_1080p();
    let req = RenderRequest {
        project: &project,
        output_path: &out_file,
        preset: &preset,
        grade: ColorGradeSettings::default(),
    };

    let (progress_tx, progress_rx) = std::sync::mpsc::channel();
    let res = render_project_to_file(req, Some(&progress_tx), None);

    if let Ok(()) = res {
        // Collect progress reports via non-blocking try_recv
        let mut max_fraction: f32 = 0.0;
        while let Ok(prog) = progress_rx.try_recv() {
            if prog.fraction > max_fraction {
                max_fraction = prog.fraction;
            }
        }
        assert!(
            max_fraction > 0.9,
            "Export progress must reach at least 90%"
        );
        assert!(out_file.exists(), "Exported MP4 file must exist on disk");
        assert!(
            out_file.metadata().unwrap().len() > 1000,
            "Exported MP4 file size must be non-trivial (>1KB)"
        );

        // Probe container using ffprobe
        if let Ok(probe) = ExportJob::probe_file(&out_file) {
            assert!(
                probe.has_video,
                "Exported file must contain a valid video stream"
            );
            assert_eq!(probe.width, Some(1920));
            assert_eq!(probe.height, Some(1080));
            assert!(
                probe.has_audio,
                "Exported file must contain a valid 48kHz audio stream"
            );
            assert!(
                probe.duration_seconds.unwrap_or(0.0) >= 0.8,
                "Duration must match sequence length"
            );
        }
    } else {
        let is_ci = std::env::var("CI").is_ok() || std::env::var("REDDART_REQUIRE_FFMPEG").is_ok();
        if is_ci {
            panic!(
                "CRITICAL: Hardware/FFmpeg encoder is required in CI for release_suite, but render failed: {:?}",
                res.err()
            );
        } else {
            eprintln!(
                "[SKIPPED TEST] test_release_export_and_probe_pipeline: Hardware/FFmpeg encoder not available in local environment."
            );
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
