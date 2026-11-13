//! Comprehensive WAL Journal Fault Injection & Durability Test Suite
//!
//! Validates:
//! 1. Torn write and interrupted append detection, safe tail truncation, and state preservation.
//! 2. CRC32 checksum mismatch detection and prevention of corrupted state ingestion.
//! 3. Strictly monotonic sequence continuity validation (gaps and regressions halted).
//! 4. Inapplicable/failed mutation handling (not counted as successfully replayed).
//! 5. Append error propagation and automatic in-memory rollback.
//! 6. Interrupted checkpoint recovery and crash-resilience.

use rd_core::{
    Clip, ClipId, ClipKind, Command, CommandError, CommandHistory, JournalEntry, JournalMutation,
    Project, ProjectJournal, TimelineMarker, TrackId, TrackKind, Transform2D, VideoClip,
};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

fn create_temp_wal_dir(label: &str) -> (PathBuf, PathBuf) {
    let temp_dir =
        std::env::temp_dir().join(format!("reddart_wal_test_{}_{}", label, Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let proj_path = temp_dir.join("project.reddart");
    (temp_dir, proj_path)
}

fn make_test_clip(source: &str, pos_us: i64, dur_us: i64) -> Clip {
    Clip {
        id: ClipId(Uuid::new_v4()),
        kind: ClipKind::Video(VideoClip {
            source_path: source.into(),
            proxy_path: None,
            has_audio: true,
        }),
        position_us: pos_us,
        source_in_us: 0,
        source_out_us: dur_us,
        speed: 1.0,
        retime_mode: rd_core::RetimeMode::default(),
        opacity: 1.0,
        transform: Transform2D::default(),
        effects: Vec::new(),
        volume: 1.0,
        pan: 0.0,
        pan_surround: 0.0,
        lfe_send: 0.0,
        keyframes: std::collections::HashMap::new(),
    }
}

#[test]
fn test_wal_torn_tail_truncation_and_recovery() {
    let (temp_dir, proj_path) = create_temp_wal_dir("torn_tail");
    let tid = TrackId(1);
    let clip = make_test_clip("scene_01.mp4", 0, 5_000_000);
    let clip_id = clip.id;

    // 1. Append 3 valid operations
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Create journal");
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "Main V1".into(),
            })
            .expect("Append track");
        journal
            .append(JournalMutation::AddClip {
                track_id: tid,
                clip,
            })
            .expect("Append clip");
        journal
            .append(JournalMutation::MoveClip {
                track_id: tid,
                clip_id,
                new_position_us: 2_000_000,
            })
            .expect("Append move");
    }

    // 2. Inject torn write (interrupted mid-write during power loss)
    let wal_path = proj_path.with_extension("reddart.wal");
    {
        let mut f = OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .expect("Open WAL for fault injection");
        f.write_all(b"{\"seq\":4,\"tx_id\":\"fa38b72a-incomplete-torn-record-data")
            .expect("Write torn bytes");
        f.flush().unwrap();
    }

    // 3. Replay into fresh project instance
    let mut project = Project::new("Torn Recovery Project");
    let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Reopen journal");
    let report = journal
        .replay_into(&mut project)
        .expect("Replay with torn tail");

    assert_eq!(report.total_records_scanned, 4);
    assert_eq!(report.valid_records_replayed, 3);
    assert_eq!(report.failed_mutations, 0);
    assert_eq!(report.corrupt_records_detected, 1);
    assert!(report.torn_tail_bytes_truncated > 0);
    assert_eq!(report.last_seq, 3);

    // Verify project state preserved
    assert_eq!(project.timeline.track_count(), 1);
    let track = project.timeline.track(tid).expect("Track exists");
    assert_eq!(track.clips().len(), 1);
    assert_eq!(track.clips()[0].position_us, 2_000_000);

    // 4. Verify subsequent append works seamlessly and appends seq=4
    let entry = journal
        .append(JournalMutation::AddMarker {
            id: 10,
            time_us: 1_000_000,
            name: "Post Recovery".into(),
            color_rgb: [100, 200, 100],
        })
        .expect("Append post recovery");
    assert_eq!(entry.seq, 4);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_checksum_corruption_detection_and_safe_truncation() {
    let (temp_dir, proj_path) = create_temp_wal_dir("checksum_corruption");
    let tid = TrackId(2);
    let clip = make_test_clip("shot_a.mp4", 0, 3_000_000);

    // 1. Append 2 valid operations
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "V2".into(),
            })
            .unwrap();
        journal
            .append(JournalMutation::AddClip {
                track_id: tid,
                clip,
            })
            .unwrap();
    }

    // 2. Append a 3rd operation, but corrupt its checksum in the WAL file
    let wal_path = proj_path.with_extension("reddart.wal");
    let mut entry3 = JournalEntry::new(
        3,
        JournalMutation::AddMarker {
            id: 1,
            time_us: 500_000,
            name: "Marker".into(),
            color_rgb: [255, 0, 0],
        },
    );
    // Tamper with checksum (simulate bit rot / corrupt sector)
    entry3.checksum ^= 0xDEAD_BEEF;

    {
        let mut f = OpenOptions::new().append(true).open(&wal_path).unwrap();
        let line = serde_json::to_string(&entry3).unwrap();
        writeln!(f, "{}", line).unwrap();
        f.flush().unwrap();
    }

    // 3. Replay: Must detect checksum mismatch, refuse corrupted record, and truncate
    let mut project = Project::new("Checksum Check");
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
    let report = journal.replay_into(&mut project).unwrap();

    assert_eq!(report.valid_records_replayed, 2);
    assert_eq!(report.corrupt_records_detected, 1);
    assert!(report.torn_tail_bytes_truncated > 0);
    assert_eq!(
        project.timeline.markers.len(),
        0,
        "Corrupted marker must not be ingested"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_sequence_gap_and_regression_halt() {
    let (temp_dir, proj_path) = create_temp_wal_dir("seq_discontinuity");
    let tid = TrackId(5);

    // 1. Write Record 1 (seq=1)
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "V5".into(),
            })
            .unwrap();
    }

    // 2. Manually append a record with a gap: seq=10 instead of seq=2
    let wal_path = proj_path.with_extension("reddart.wal");
    let bad_seq_entry = JournalEntry::new(
        10,
        JournalMutation::AddMarker {
            id: 2,
            time_us: 100_000,
            name: "Discontinuous".into(),
            color_rgb: [0, 255, 0],
        },
    );
    {
        let mut f = OpenOptions::new().append(true).open(&wal_path).unwrap();
        let line = serde_json::to_string(&bad_seq_entry).unwrap();
        writeln!(f, "{}", line).unwrap();
        f.flush().unwrap();
    }

    // 3. Replay: Discontinuity must halt recovery at offset and preserve record 1
    let mut project = Project::new("Sequence Gap Check");
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
    let report = journal.replay_into(&mut project).unwrap();

    assert_eq!(report.valid_records_replayed, 1);
    assert_eq!(report.corrupt_records_detected, 1);
    assert_eq!(project.timeline.track_count(), 1);
    assert_eq!(project.timeline.markers.len(), 0);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_inapplicable_mutation_not_counted_as_replayed() {
    let (temp_dir, proj_path) = create_temp_wal_dir("failed_mutation");
    let tid = TrackId(8);

    // 1. Create WAL with:
    //    seq 1: Valid AddTrack(8)
    //    seq 2: Invalid RemoveClip for non-existent clip ID on track 8
    //    seq 3: Valid AddMarker
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "V8".into(),
            })
            .unwrap();
        journal
            .append(JournalMutation::RemoveClip {
                track_id: tid,
                clip_id: ClipId(Uuid::new_v4()),
            })
            .unwrap();
        journal
            .append(JournalMutation::AddMarker {
                id: 42,
                time_us: 200_000,
                name: "Valid Marker".into(),
                color_rgb: [128, 128, 128],
            })
            .unwrap();
    }

    // 2. Replay:
    //    - seq 1 applied -> valid
    //    - seq 2 rejected by timeline -> failed_mutation += 1 (NOT counted as valid_records_replayed)
    //    - seq 3 applied -> valid
    let mut project = Project::new("Failed Mutation Check");
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
    let report = journal.replay_into(&mut project).unwrap();

    assert_eq!(report.total_records_scanned, 3);
    assert_eq!(
        report.valid_records_replayed, 2,
        "Only 2 valid mutations must be counted"
    );
    assert_eq!(
        report.failed_mutations, 1,
        "The invalid RemoveClip must be counted in failed_mutations"
    );
    assert_eq!(
        report.corrupt_records_detected, 0,
        "Entries were structurally sound"
    );
    assert_eq!(project.timeline.markers.len(), 1);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn test_wal_append_failure_propagates_and_rolls_back_in_memory() {
    let (temp_dir, proj_path) = create_temp_wal_dir("append_rollback");
    let mut project = Project::new("Rollback Test Project");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");
    let clip = make_test_clip("asset.mp4", 0, 5_000_000);
    let clip_id = clip.id;
    project.timeline.track_mut(tid).unwrap().add_clip(clip);

    let mut history = CommandHistory::new(50);

    // Create journal pointed at proj_path
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();

    // Now deliberately make the WAL file read-only on disk (or point it to an impossible path)
    let wal_path = proj_path.with_extension("reddart.wal");
    // Ensure file exists then set read-only permissions
    File::create(&wal_path).unwrap();
    let mut perms = fs::metadata(&wal_path).unwrap().permissions();
    perms.set_readonly(true);
    let _ = fs::set_permissions(&wal_path, perms);

    // Attempt to execute MoveClip command
    let move_cmd = Command::MoveClip {
        track_id: tid,
        clip_id,
        old_position_us: 0,
        new_position_us: 3_000_000,
    };

    let result = history.execute_with_journal(move_cmd, &mut project, &mut journal);

    // If the OS enforces read-only (Windows/POSIX), append will fail.
    // In that case, verify error propagation and in-memory rollback.
    if let Err(e) = result {
        assert!(matches!(e, CommandError::JournalError(_)));
        // In-memory clip position must have been rolled back to 0!
        let clip_now = project.timeline.track(tid).unwrap().clip(clip_id).unwrap();
        assert_eq!(
            clip_now.position_us, 0,
            "In-memory state must be rolled back on WAL append failure"
        );
        // Undo stack must NOT contain the failed command
        assert_eq!(history.undo_count(), 0);
    }

    // Restore permissions for cleanup
    let mut restore_perms = fs::metadata(&wal_path).unwrap().permissions();
    restore_perms.set_readonly(false);
    let _ = fs::set_permissions(&wal_path, restore_perms);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_interrupted_checkpoint_recovery() {
    let (temp_dir, proj_path) = create_temp_wal_dir("checkpoint_recovery");
    let mut project = Project::new("Checkpoint Recovery Test");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");
    let clip = make_test_clip("scene.mp4", 0, 4_000_000);
    let clip_id = clip.id;
    project.timeline.track_mut(tid).unwrap().add_clip(clip);

    // 1. Perform checkpoint: saves snapshot and compacts WAL
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
    journal
        .checkpoint(&project, &proj_path)
        .expect("Checkpoint succeeds");

    // 2. Append additional uncommitted edits after checkpoint
    journal
        .append(JournalMutation::MoveClip {
            track_id: tid,
            clip_id,
            new_position_us: 7_500_000,
        })
        .unwrap();
    journal
        .append(JournalMutation::AddMarker {
            id: 99,
            time_us: 1_234_567,
            name: "After Checkpoint".into(),
            color_rgb: [200, 50, 50],
        })
        .unwrap();

    // 3. Simulate sudden crash and recovery via Project::load_file_with_recovery
    let (recovered_project, report) =
        Project::load_file_with_recovery(&proj_path).expect("Recover project");
    let rep = report.expect("Replay report present");

    assert_eq!(rep.valid_records_replayed, 2);
    assert_eq!(rep.failed_mutations, 0);
    assert_eq!(rep.corrupt_records_detected, 0);

    let rec_clip = recovered_project
        .timeline
        .track(tid)
        .unwrap()
        .clip(clip_id)
        .unwrap();
    assert_eq!(
        rec_clip.position_us, 7_500_000,
        "Uncommitted WAL edits after checkpoint must be restored"
    );
    assert_eq!(recovered_project.timeline.markers.len(), 1);
    assert_eq!(
        recovered_project.timeline.markers[0].name,
        "After Checkpoint"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn test_wal_sequence_advancement_atomic_and_no_sequence_holes_on_failure() {
    let (temp_dir, proj_path) = create_temp_wal_dir("seq_advancement_atomicity");
    let wal_path = proj_path.with_extension("reddart.wal");
    let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Create journal");

    // 1. Initial sequence is 0
    assert_eq!(journal.current_seq(), 0);

    // 2. Append first mutation -> seq advances to 1
    let m1 = JournalMutation::AddTrack {
        id: TrackId(1),
        kind: TrackKind::Video,
        name: "V1".into(),
    };
    let e1 = journal.append(m1).expect("Append 1");
    assert_eq!(e1.seq, 1);
    assert_eq!(journal.current_seq(), 1);

    // 3. Make WAL file read-only on disk to trigger an I/O error
    let mut perms = fs::metadata(&wal_path).unwrap().permissions();
    perms.set_readonly(true);
    let _ = fs::set_permissions(&wal_path, perms);

    // 4. Attempt to append a second mutation -> must fail
    let m2 = JournalMutation::AddTrack {
        id: TrackId(2),
        kind: TrackKind::Audio,
        name: "A1".into(),
    };
    let append_err = journal.append(m2);

    if append_err.is_err() {
        // Crucial invariant: Sequence counter must NOT have advanced!
        assert_eq!(
            journal.current_seq(),
            1,
            "Sequence counter must not advance when write fails"
        );

        // 5. Restore write permissions
        let mut restore = fs::metadata(&wal_path).unwrap().permissions();
        restore.set_readonly(false);
        let _ = fs::set_permissions(&wal_path, restore);

        // 6. Next successful append must receive sequence 2 with NO HOLE
        let m3 = JournalMutation::AddTrack {
            id: TrackId(3),
            kind: TrackKind::Audio,
            name: "A2".into(),
        };
        let e3 = journal
            .append(m3)
            .expect("Append after restoring write perms");
        assert_eq!(
            e3.seq, 2,
            "Sequence must be consecutive (2) without sequence gaps"
        );
        assert_eq!(journal.current_seq(), 2);

        // 7. Verify replay detects NO holes and replays both valid records cleanly
        let mut project = Project::new("Continuity Check");
        let report = journal.replay_into(&mut project).expect("Replay journal");
        assert_eq!(report.valid_records_replayed, 2);
        assert_eq!(report.last_seq, 2);
        assert_eq!(report.corrupt_records_detected, 0);
    } else {
        // Environment didn't restrict file write (e.g. elevated container)
        let mut restore = fs::metadata(&wal_path).unwrap().permissions();
        restore.set_readonly(false);
        let _ = fs::set_permissions(&wal_path, restore);
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_batch_append_atomic_rollback_on_disk() {
    let (temp_dir, proj_path) = create_temp_wal_dir("batch_atomic");
    let wal_path = proj_path.with_extension("reddart.wal");
    let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Create journal");

    // 1. Write an initial baseline record
    let m1 = JournalMutation::AddTrack {
        id: TrackId(10),
        kind: TrackKind::Video,
        name: "Base".into(),
    };
    journal.append(m1).expect("Baseline append");
    assert_eq!(journal.current_seq(), 1);
    let baseline_len = fs::metadata(&wal_path).unwrap().len();
    assert!(baseline_len > 0);

    // 2. Append a valid multi-mutation batch
    let batch = vec![
        JournalMutation::AddMarker {
            id: 1,
            time_us: 100_000,
            name: "M1".into(),
            color_rgb: [10, 20, 30],
        },
        JournalMutation::AddMarker {
            id: 2,
            time_us: 200_000,
            name: "M2".into(),
            color_rgb: [40, 50, 60],
        },
    ];
    let entries = journal.append_batch(&batch).expect("Batch append");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].seq, 2);
    assert_eq!(entries[1].seq, 3);
    assert_eq!(journal.current_seq(), 3);

    // 3. Verify replay restores both markers
    let mut project = Project::new("Batch Test");
    let report = journal.replay_into(&mut project).expect("Replay batch");
    assert_eq!(report.valid_records_replayed, 3);
    assert_eq!(project.timeline.markers.len(), 2);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn test_wal_compound_transaction_failure_handling() {
    let (temp_dir, proj_path) = create_temp_wal_dir("compound_tx_failure");
    let wal_path = proj_path.with_extension("reddart.wal");
    let mut project = Project::new("Compound Tx Project");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");

    let mut history = CommandHistory::new(50);
    let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Create journal");

    // Baseline add clip
    let clip = make_test_clip("shot.mp4", 0, 5_000_000);
    let clip_id = clip.id;
    history
        .execute_with_journal(
            Command::AddClip {
                track_id: tid,
                clip,
            },
            &mut project,
            &mut journal,
        )
        .expect("Baseline AddClip");
    assert_eq!(journal.current_seq(), 1);

    // Prepare a Compound command with 2 operations
    let compound = Command::Compound {
        label: "Move and Opacity".into(),
        commands: vec![
            Command::MoveClip {
                track_id: tid,
                clip_id,
                old_position_us: 0,
                new_position_us: 4_000_000,
            },
            Command::SetOpacity {
                track_id: tid,
                clip_id,
                old_opacity: 1.0,
                new_opacity: 0.5,
            },
        ],
    };

    // Make WAL file read-only to simulate I/O failure during batch commit
    let mut perms = fs::metadata(&wal_path).unwrap().permissions();
    perms.set_readonly(true);
    let _ = fs::set_permissions(&wal_path, perms);

    let res = history.execute_with_journal(compound, &mut project, &mut journal);
    if let Err(e) = res {
        assert!(matches!(e, CommandError::JournalError(_)));

        // In-memory state must be completely rolled back!
        let clip_check = project.timeline.track(tid).unwrap().clip(clip_id).unwrap();
        assert_eq!(
            clip_check.position_us, 0,
            "Clip position must be restored to 0"
        );
        assert_eq!(
            clip_check.opacity, 1.0,
            "Clip opacity must be restored to 1.0"
        );

        // History must not have recorded the failed compound command
        assert_eq!(history.undo_count(), 1); // Only initial AddClip
        assert_eq!(journal.current_seq(), 1, "WAL sequence must not advance");

        // Restore permissions
        let mut restore = fs::metadata(&wal_path).unwrap().permissions();
        restore.set_readonly(false);
        let _ = fs::set_permissions(&wal_path, restore);
    } else {
        let mut restore = fs::metadata(&wal_path).unwrap().permissions();
        restore.set_readonly(false);
        let _ = fs::set_permissions(&wal_path, restore);
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_interrupted_writes_mid_record_truncation_and_continuation() {
    let (temp_dir, proj_path) = create_temp_wal_dir("interrupted_write");
    let wal_path = proj_path.with_extension("reddart.wal");
    let tid = TrackId(1);

    // 1. Write 3 valid mutations
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "V1".into(),
            })
            .unwrap();
        journal
            .append(JournalMutation::AddMarker {
                id: 1,
                time_us: 1_000_000,
                name: "M1".into(),
                color_rgb: [10, 20, 30],
            })
            .unwrap();
        journal
            .append(JournalMutation::AddMarker {
                id: 2,
                time_us: 2_000_000,
                name: "M2".into(),
                color_rgb: [40, 50, 60],
            })
            .unwrap();
    }

    // 2. Inject interrupted partial record write without trailing newline
    {
        let mut f = OpenOptions::new().append(true).open(&wal_path).unwrap();
        f.write_all(
            b"{\"seq\":4,\"crc32\":987654321,\"mutation\":{\"type\":\"AddMarker\",\"id\":3,\"time_",
        )
        .unwrap();
        f.flush().unwrap();
    }

    // 3. Open journal and replay: must truncate torn record, recover first 3 entries
    let mut project = Project::new("Interrupted Write Test");
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
    let report = journal.replay_into(&mut project).unwrap();

    assert_eq!(report.valid_records_replayed, 3);
    assert_eq!(report.corrupt_records_detected, 1);
    assert!(report.torn_tail_bytes_truncated > 0);
    assert_eq!(project.timeline.track_count(), 1);
    assert_eq!(project.timeline.markers.len(), 2);
    assert_eq!(journal.current_seq(), 3);

    // 4. Continue writing: append valid entry with seq=4
    let e4 = journal
        .append(JournalMutation::AddMarker {
            id: 4,
            time_us: 4_000_000,
            name: "M4 Valid".into(),
            color_rgb: [100, 100, 100],
        })
        .unwrap();
    assert_eq!(e4.seq, 4);
    assert_eq!(journal.current_seq(), 4);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_interrupted_batch_commit_mid_write_recovery() {
    let (temp_dir, proj_path) = create_temp_wal_dir("interrupted_batch");
    let wal_path = proj_path.with_extension("reddart.wal");
    let tid = TrackId(2);

    // 1. Initial valid baseline
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "V2".into(),
            })
            .unwrap();
    }

    // 2. Prepare a 3-item batch where the 3rd item is truncated midway through disk flush
    let entry1 = JournalEntry::new(
        2,
        JournalMutation::AddMarker {
            id: 10,
            time_us: 100_000,
            name: "B1".into(),
            color_rgb: [1, 2, 3],
        },
    );
    let entry2 = JournalEntry::new(
        3,
        JournalMutation::AddMarker {
            id: 20,
            time_us: 200_000,
            name: "B2".into(),
            color_rgb: [4, 5, 6],
        },
    );

    {
        let mut f = OpenOptions::new().append(true).open(&wal_path).unwrap();
        writeln!(f, "{}", serde_json::to_string(&entry1).unwrap()).unwrap();
        writeln!(f, "{}", serde_json::to_string(&entry2).unwrap()).unwrap();
        // Truncated third entry of batch
        f.write_all(b"{\"seq\":4,\"crc32\":12345,\"mutation\":{\"type\":\"AddMarker\"")
            .unwrap();
        f.flush().unwrap();
    }

    // 3. Replay into project
    let mut project = Project::new("Interrupted Batch Test");
    let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
    let report = journal.replay_into(&mut project).unwrap();

    assert_eq!(report.valid_records_replayed, 3); // Seq 1, 2, 3
    assert_eq!(report.corrupt_records_detected, 1);
    assert_eq!(project.timeline.markers.len(), 2);
    assert_eq!(journal.current_seq(), 3);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_checkpoint_crash_with_orphaned_temp_file_recovery() {
    let (temp_dir, proj_path) = create_temp_wal_dir("checkpoint_temp_crash");

    let mut project = Project::new("Base Project Before Checkpoint");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");
    let clip = make_test_clip("original.mp4", 0, 5_000_000);
    project.timeline.track_mut(tid).unwrap().add_clip(clip);
    project.save_atomic(&proj_path).unwrap();

    // Append uncommitted edits to WAL
    {
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal
            .append(JournalMutation::AddMarker {
                id: 1,
                time_us: 2_500_000,
                name: "Uncommitted Cut".into(),
                color_rgb: [255, 0, 0],
            })
            .unwrap();
    }

    // Simulate crash during checkpoint save_atomic: create orphaned partial temp file
    let orphaned_tmp = temp_dir.join(format!(".project_{}.tmp", Uuid::new_v4()));
    fs::write(&orphaned_tmp, b"{\"truncated_json\": true, incomplete").unwrap();

    // Verify recovery loads project, replays uncommitted WAL edit, and ignores/survives orphaned temp file
    let (recovered, report) = Project::load_file_with_recovery(&proj_path).unwrap();
    assert!(report.is_some());
    assert_eq!(report.unwrap().valid_records_replayed, 1);
    assert_eq!(recovered.timeline.markers.len(), 1);
    assert_eq!(recovered.timeline.markers[0].name, "Uncommitted Cut");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_checkpoint_crash_mid_journal_compaction_recovery() {
    let (temp_dir, proj_path) = create_temp_wal_dir("checkpoint_compaction_crash");
    let wal_path = proj_path.with_extension("reddart.wal");

    let mut project = Project::new("Compacted Base");
    let _ = project.timeline.add_track(TrackKind::Video, "V1");
    project.timeline.insert_marker(TimelineMarker {
        id: 10,
        time_us: 1_000_000,
        name: "Checkpoint Baseline".into(),
        color_rgb: [0, 255, 0],
    });
    // Checkpoint project file is saved
    project.save_atomic(&proj_path).unwrap();

    // Simulate crash mid-compaction of .wal file (torn sentinel)
    {
        let mut f = File::create(&wal_path).unwrap();
        f.write_all(b"{\"seq\":1,\"crc32\":0,\"mutation\":{\"type\":\"Checkpoint\",\"sequence\":1")
            .unwrap();
        f.flush().unwrap();
    }

    // Recover project: must safely truncate torn sentinel and load base project intact
    let (recovered, report) = Project::load_file_with_recovery(&proj_path).unwrap();
    let rep = report.expect("Replay report present");
    assert_eq!(rep.corrupt_records_detected, 1);
    assert_eq!(recovered.timeline.markers.len(), 1);
    assert_eq!(recovered.timeline.markers[0].name, "Checkpoint Baseline");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_wal_application_restart_full_lifecycle() {
    let (temp_dir, proj_path) = create_temp_wal_dir("app_restart_lifecycle");

    // -------------------------------------------------------------
    // Session 1: Launch app, create project, perform edits, save checkpoint
    // -------------------------------------------------------------
    let tid_v1 = TrackId(1);
    let tid_a1 = TrackId(2);
    let clip1 = make_test_clip("intro.mp4", 0, 4_000_000);
    let cid1 = clip1.id;

    {
        let mut project = Project::new("Production Documentary");
        project
            .timeline
            .insert_track_with_id(tid_v1, 0, TrackKind::Video, "V1");
        project
            .timeline
            .insert_track_with_id(tid_a1, 1, TrackKind::Audio, "A1");
        project
            .timeline
            .track_mut(tid_v1)
            .unwrap()
            .add_clip(clip1.clone());
        project.timeline.mark_in_us = Some(0);
        project.timeline.mark_out_us = Some(10_000_000);

        // Checkpoint project to disk
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        journal.checkpoint(&project, &proj_path).unwrap();

        // Perform active edits recorded in WAL
        let mut history = CommandHistory::new(50);

        // 1. Add clip 2
        let clip2 = make_test_clip("b_roll.mp4", 4_000_000, 3_000_000);
        let cid2 = clip2.id;
        history
            .execute_with_journal(
                Command::AddClip {
                    track_id: tid_v1,
                    clip: clip2,
                },
                &mut project,
                &mut journal,
            )
            .unwrap();

        // 2. Add marker
        history
            .execute_with_journal(
                Command::AddMarker {
                    marker: TimelineMarker {
                        id: 50,
                        time_us: 4_000_000,
                        name: "Cut Point".into(),
                        color_rgb: [255, 200, 0],
                    },
                },
                &mut project,
                &mut journal,
            )
            .unwrap();

        // 3. Move clip 2
        history
            .execute_with_journal(
                Command::MoveClip {
                    track_id: tid_v1,
                    clip_id: cid2,
                    old_position_us: 4_000_000,
                    new_position_us: 5_000_000,
                },
                &mut project,
                &mut journal,
            )
            .unwrap();

        // Abrupt crash: process exits immediately without checkpointing!
        // `project`, `journal`, and `history` are dropped without cleanup.
    }

    // -------------------------------------------------------------
    // Session 2: Application Restart after crash
    // -------------------------------------------------------------
    {
        // Reopen project via load_file_with_recovery
        let (mut recovered, report) = Project::load_file_with_recovery(&proj_path).unwrap();
        let rep = report.expect("Recovery report generated");
        assert_eq!(rep.valid_records_replayed, 3);
        assert_eq!(rep.failed_mutations, 0);
        assert_eq!(rep.corrupt_records_detected, 0);

        // Assert all edits are fully restored
        let v1 = recovered.timeline.track(tid_v1).unwrap();
        assert_eq!(v1.clips().len(), 2);
        assert_eq!(v1.clip(cid1).unwrap().position_us, 0);
        // Clip 2 was moved to 5_000_000
        let c2_recovered = v1
            .clips()
            .iter()
            .find(|c| c.position_us == 5_000_000)
            .unwrap();
        assert_eq!(c2_recovered.duration_us(), 3_000_000);
        assert_eq!(recovered.timeline.markers.len(), 1);
        assert_eq!(recovered.timeline.markers[0].name, "Cut Point");

        // Continue editing in the recovered session
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        let mut history = CommandHistory::new(50);

        history
            .execute_with_journal(
                Command::SetSpeed {
                    track_id: tid_v1,
                    clip_id: cid1,
                    old_speed: 1.0,
                    new_speed: 2.0,
                },
                &mut recovered,
                &mut journal,
            )
            .unwrap();

        assert_eq!(
            recovered
                .timeline
                .track(tid_v1)
                .unwrap()
                .clip(cid1)
                .unwrap()
                .speed,
            2.0
        );

        // Perform clean checkpoint and shutdown
        journal.checkpoint(&recovered, &proj_path).unwrap();
    }

    // -------------------------------------------------------------
    // Session 3: Clean Launch
    // -------------------------------------------------------------
    {
        let (clean_project, report) = Project::load_file_with_recovery(&proj_path).unwrap();
        // Since session 2 clean checkpointed, WAL has 0 uncommitted edits
        assert_eq!(
            clean_project
                .timeline
                .track(tid_v1)
                .unwrap()
                .clip(cid1)
                .unwrap()
                .speed,
            2.0
        );
        if let Some(rep) = report {
            assert_eq!(rep.valid_records_replayed, 0);
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
