//! Industrial-Grade Write-Ahead Log (WAL) Journal for Reddart.
//!
//! Provides transactional persistence, checksum integrity (CRC32),
//! atomic checkpointing, and interrupted-write crash recovery.
//! Every project mutation is appended to an on-disk transaction log
//! prior to or in lockstep with in-memory state application.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::{Clip, ClipId, Project, TimelineMarker, TrackId, TrackKind};

/// Precomputed CRC32 IEEE 802.3 lookup table for high-throughput checksum validation.
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            if c & 1 != 0 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            j += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// Calculate 32-bit CRC32 checksum over arbitrary byte slices.
pub fn crc32_checksum(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        let idx = ((crc ^ (b as u32)) & 0xFF) as usize;
        crc = CRC32_TABLE[idx] ^ (crc >> 8);
    }
    !crc
}

/// Atomic mutation operations recorded in the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JournalMutation {
    AddTrack {
        id: TrackId,
        kind: TrackKind,
        name: String,
    },
    RemoveTrack {
        id: TrackId,
    },
    AddClip {
        track_id: TrackId,
        clip: Clip,
    },
    RemoveClip {
        track_id: TrackId,
        clip_id: ClipId,
    },
    MoveClip {
        track_id: TrackId,
        clip_id: ClipId,
        new_position_us: i64,
    },
    MoveClipTrack {
        source_track_id: TrackId,
        target_track_id: TrackId,
        clip_id: ClipId,
        new_position_us: i64,
    },
    SplitClip {
        track_id: TrackId,
        clip_id: ClipId,
        split_time_us: i64,
        new_clip: Clip,
    },
    RippleTrimIn {
        track_id: TrackId,
        clip_id: ClipId,
        delta_us: i64,
    },
    RippleTrimOut {
        track_id: TrackId,
        clip_id: ClipId,
        delta_us: i64,
    },
    RollEdit {
        track_id: TrackId,
        outgoing_id: ClipId,
        incoming_id: ClipId,
        delta_us: i64,
    },
    SlipEdit {
        track_id: TrackId,
        clip_id: ClipId,
        delta_us: i64,
    },
    SlideEdit {
        track_id: TrackId,
        clip_id: ClipId,
        delta_us: i64,
    },
    RippleDelete {
        track_id: TrackId,
        clip_id: ClipId,
    },
    SetMarkInOut {
        mark_in_us: Option<i64>,
        mark_out_us: Option<i64>,
    },
    AddMarker {
        id: u64,
        time_us: i64,
        name: String,
        color_rgb: [u8; 3],
    },
    RemoveMarker {
        id: u64,
    },
    TrimClip {
        track_id: TrackId,
        clip_id: ClipId,
        new_source_in_us: i64,
        new_source_out_us: i64,
        new_position_us: i64,
    },
    SetSpeed {
        track_id: TrackId,
        clip_id: ClipId,
        new_speed: f64,
    },
    SetOpacity {
        track_id: TrackId,
        clip_id: ClipId,
        new_opacity: f32,
    },
    RelinkMedia {
        old_path: String,
        new_path: String,
    },
    Checkpoint {
        sequence: u64,
        snapshot_time_ms: i64,
    },
}

impl JournalMutation {
    /// Apply this mutation directly to a mutable project instance.
    pub fn apply(&self, project: &mut Project) -> Result<(), &'static str> {
        match self {
            JournalMutation::AddTrack { id, kind, name } => {
                let tid = project.timeline.add_track(*kind, name);
                if let Some(t) = project.timeline.track_mut(tid) {
                    t.id = *id;
                }
                project.touch();
                Ok(())
            }
            JournalMutation::RemoveTrack { id } => {
                if project.timeline.track(*id).is_none() {
                    return Err("Track not found");
                }
                project.timeline.remove_track(*id);
                project.touch();
                Ok(())
            }
            JournalMutation::AddClip { track_id, clip } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                if track.clip(clip.id).is_some() {
                    return Err("Clip with this ID already exists on track");
                }
                track.add_clip(clip.clone());
                project.touch();
                Ok(())
            }
            JournalMutation::RemoveClip { track_id, clip_id } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                track
                    .remove_clip(*clip_id)
                    .ok_or("Clip not found on track")?;
                project.touch();
                Ok(())
            }
            JournalMutation::MoveClip {
                track_id,
                clip_id,
                new_position_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                let clip = track.clip_mut(*clip_id).ok_or("Clip not found")?;
                clip.position_us = *new_position_us;
                project.touch();
                Ok(())
            }
            JournalMutation::MoveClipTrack {
                source_track_id,
                target_track_id,
                clip_id,
                new_position_us,
            } => {
                let clip_opt = project
                    .timeline
                    .track_mut(*source_track_id)
                    .and_then(|t| t.remove_clip(*clip_id));
                if let Some(mut clip) = clip_opt {
                    clip.position_us = *new_position_us;
                    let target_track = project
                        .timeline
                        .track_mut(*target_track_id)
                        .ok_or("Target track not found")?;
                    target_track.add_clip(clip);
                    project.touch();
                    Ok(())
                } else {
                    Err("Clip or source track not found")
                }
            }
            JournalMutation::SplitClip {
                track_id,
                clip_id,
                split_time_us,
                new_clip,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                let new_id = track
                    .split_clip_at(*clip_id, *split_time_us)
                    .ok_or("Cannot split clip at specified timestamp")?;
                if let Some(created) = track.clip_mut(new_id) {
                    created.id = new_clip.id;
                }
                project.touch();
                Ok(())
            }
            JournalMutation::RippleTrimIn {
                track_id,
                clip_id,
                delta_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                track.ripple_trim_in(*clip_id, *delta_us)?;
                project.touch();
                Ok(())
            }
            JournalMutation::RippleTrimOut {
                track_id,
                clip_id,
                delta_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                track.ripple_trim_out(*clip_id, *delta_us)?;
                project.touch();
                Ok(())
            }
            JournalMutation::RollEdit {
                track_id,
                outgoing_id,
                incoming_id,
                delta_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                track.roll_edit(*outgoing_id, *incoming_id, *delta_us)?;
                project.touch();
                Ok(())
            }
            JournalMutation::SlipEdit {
                track_id,
                clip_id,
                delta_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                track.slip_edit(*clip_id, *delta_us)?;
                project.touch();
                Ok(())
            }
            JournalMutation::SlideEdit {
                track_id,
                clip_id,
                delta_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                track.slide_edit(*clip_id, *delta_us)?;
                project.touch();
                Ok(())
            }
            JournalMutation::RippleDelete { track_id, clip_id } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                if track.clip(*clip_id).is_none() {
                    return Err("Clip not found for ripple delete");
                }
                track.ripple_delete(*clip_id);
                project.touch();
                Ok(())
            }
            JournalMutation::SetMarkInOut {
                mark_in_us,
                mark_out_us,
            } => {
                project.timeline.mark_in_us = *mark_in_us;
                project.timeline.mark_out_us = *mark_out_us;
                project.touch();
                Ok(())
            }
            JournalMutation::AddMarker {
                id,
                time_us,
                name,
                color_rgb,
            } => {
                if project.timeline.markers.iter().any(|m| m.id == *id) {
                    return Err("Marker with this ID already exists");
                }
                project.timeline.markers.push(TimelineMarker {
                    id: *id,
                    time_us: *time_us,
                    name: name.clone(),
                    color_rgb: *color_rgb,
                });
                project.touch();
                Ok(())
            }
            JournalMutation::RemoveMarker { id } => {
                let initial_len = project.timeline.markers.len();
                project.timeline.remove_marker(*id);
                if project.timeline.markers.len() == initial_len {
                    return Err("Marker not found to remove");
                }
                project.touch();
                Ok(())
            }
            JournalMutation::TrimClip {
                track_id,
                clip_id,
                new_source_in_us,
                new_source_out_us,
                new_position_us,
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                let clip = track.clip_mut(*clip_id).ok_or("Clip not found")?;
                if *new_source_in_us >= 0 {
                    clip.source_in_us = *new_source_in_us;
                }
                if *new_source_out_us >= 0 {
                    clip.source_out_us = *new_source_out_us;
                }
                if *new_position_us >= 0 {
                    clip.position_us = *new_position_us;
                }
                project.touch();
                Ok(())
            }
            JournalMutation::SetSpeed {
                track_id,
                clip_id,
                new_speed,
            } => {
                if *new_speed <= 0.0 || !new_speed.is_finite() {
                    return Err("Invalid clip speed");
                }
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                let clip = track.clip_mut(*clip_id).ok_or("Clip not found")?;
                clip.speed = *new_speed;
                project.touch();
                Ok(())
            }
            JournalMutation::SetOpacity {
                track_id,
                clip_id,
                new_opacity,
            } => {
                if *new_opacity < 0.0 || *new_opacity > 1.0 || !new_opacity.is_finite() {
                    return Err("Invalid clip opacity");
                }
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or("Track not found")?;
                let clip = track.clip_mut(*clip_id).ok_or("Clip not found")?;
                clip.opacity = *new_opacity;
                project.touch();
                Ok(())
            }
            JournalMutation::RelinkMedia { old_path, new_path } => {
                project.relink_media(old_path, new_path);
                project.touch();
                Ok(())
            }
            JournalMutation::Checkpoint { .. } => Ok(()),
        }
    }
}

/// An immutable, checksummed transaction entry in the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub tx_id: Uuid,
    pub timestamp_ms: i64,
    pub mutation: JournalMutation,
    pub checksum: u32,
}

impl JournalEntry {
    /// Create a new entry and calculate its CRC32 checksum.
    pub fn new(seq: u64, mutation: JournalMutation) -> Self {
        let tx_id = Uuid::new_v4();
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // Compute checksum over serialized mutation payload
        let payload_bytes =
            serde_json::to_vec(&(&seq, &tx_id, &timestamp_ms, &mutation)).unwrap_or_default();
        let checksum = crc32_checksum(&payload_bytes);

        Self {
            seq,
            tx_id,
            timestamp_ms,
            mutation,
            checksum,
        }
    }

    /// Verify whether this entry matches its embedded CRC32 checksum.
    pub fn verify_checksum(&self) -> bool {
        let payload_bytes =
            serde_json::to_vec(&(&self.seq, &self.tx_id, &self.timestamp_ms, &self.mutation))
                .unwrap_or_default();
        let expected = crc32_checksum(&payload_bytes);
        self.checksum == expected
    }
}

/// Metrics returned when replaying a journal during startup or recovery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayReport {
    pub total_records_scanned: usize,
    pub valid_records_replayed: usize,
    pub failed_mutations: usize,
    pub corrupt_records_detected: usize,
    pub torn_tail_bytes_truncated: usize,
    pub last_seq: u64,
}

/// Production Write-Ahead Log journal manager.
pub struct ProjectJournal {
    wal_path: PathBuf,
    current_seq: u64,
}

impl ProjectJournal {
    /// Open or initialize a WAL journal for a specific project file path.
    pub fn open_or_create(project_path: impl AsRef<Path>) -> std::io::Result<Self> {
        let base = project_path.as_ref();
        let wal_path = base.with_extension("reddart.wal");

        let mut journal = Self {
            wal_path,
            current_seq: 0,
        };

        // Scan existing file to determine next monotonic sequence number
        if journal.wal_path.exists() {
            let file = File::open(&journal.wal_path)?;
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(entry) = serde_json::from_str::<JournalEntry>(&line)
                    && entry.verify_checksum()
                {
                    journal.current_seq = journal.current_seq.max(entry.seq);
                }
            }
        }

        Ok(journal)
    }

    /// Path to the current WAL file.
    pub fn path(&self) -> &Path {
        &self.wal_path
    }

    /// Current highest sequence number.
    pub fn current_seq(&self) -> u64 {
        self.current_seq
    }

    /// Append a batch of mutations to the journal atomically.
    ///
    /// Durability & Consistency guarantees:
    /// 1. Entries are serialized in-memory first; any serialization error fails before touching disk.
    /// 2. If file write, flush, or sync fails, the file on disk is rolled back (truncated)
    ///    to its pre-write length and synced.
    /// 3. In-memory sequence counter `self.current_seq` is advanced ONLY AFTER physical
    ///    durability (`file.sync_all()`) is confirmed. On any failure, `self.current_seq` is
    ///    unmodified, guaranteeing zero sequence holes or gaps in subsequent operations.
    pub fn append_batch(
        &mut self,
        mutations: &[JournalMutation],
    ) -> std::io::Result<Vec<JournalEntry>> {
        if mutations.is_empty() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::with_capacity(mutations.len());
        let mut buffer = Vec::new();
        let mut next_seq = self.current_seq;

        for mutation in mutations {
            next_seq += 1;
            let entry = JournalEntry::new(next_seq, mutation.clone());
            let line = serde_json::to_string(&entry)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            buffer.extend_from_slice(line.as_bytes());
            buffer.push(b'\n');
            entries.push(entry);
        }

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&self.wal_path)?;

        let start_len = file.metadata()?.len();
        file.seek(SeekFrom::End(0))?;

        let write_res = (|| -> std::io::Result<()> {
            file.write_all(&buffer)?;
            file.flush()?;
            file.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_res {
            let _ = file.set_len(start_len);
            let _ = file.sync_all();
            return Err(e);
        }

        // Monotonic sequence only advances upon verified physical sync
        self.current_seq = next_seq;
        Ok(entries)
    }

    /// Append a new mutation to the journal, syncing it to physical storage.
    pub fn append(&mut self, mutation: JournalMutation) -> std::io::Result<JournalEntry> {
        let mut entries = self.append_batch(&[mutation])?;
        Ok(entries.remove(0))
    }

    /// Replay uncommitted journal entries into a project with strict sequence continuity,
    /// CRC32 integrity validation, and torn-tail fault tolerance.
    pub fn replay_into(&mut self, project: &mut Project) -> std::io::Result<ReplayReport> {
        if !self.wal_path.exists() {
            return Ok(ReplayReport::default());
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.wal_path)?;

        let mut report = ReplayReport::default();
        let mut last_valid_offset = 0u64;
        let mut reader = BufReader::new(&mut file);
        let mut buffer = String::new();
        let mut expected_seq: Option<u64> = None;

        loop {
            let start_offset = last_valid_offset;
            buffer.clear();
            let bytes_read = reader.read_line(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let trimmed = buffer.trim_end();
            if trimmed.is_empty() {
                last_valid_offset += bytes_read as u64;
                continue;
            }

            report.total_records_scanned += 1;

            match serde_json::from_str::<JournalEntry>(trimmed) {
                Ok(entry) => {
                    // 1. Verify CRC32 checksum
                    if !entry.verify_checksum() {
                        log::warn!(
                            "Project WAL recovery detected corrupted checksum at offset {}. Truncating tail.",
                            start_offset
                        );
                        report.corrupt_records_detected += 1;
                        let total_len = file.metadata()?.len();
                        report.torn_tail_bytes_truncated = (total_len - start_offset) as usize;
                        file.set_len(start_offset)?;
                        file.seek(SeekFrom::Start(start_offset))?;
                        file.flush()?;
                        file.sync_all()?;
                        break;
                    }

                    // 2. Verify sequence continuity (strictly monotonic without regressions or unexpected gaps)
                    if let Some(exp) = expected_seq {
                        if entry.seq != exp {
                            log::warn!(
                                "Project WAL sequence discontinuity at byte offset {}: expected seq {}, encountered {}. Halting replay to preserve valid project state.",
                                start_offset,
                                exp,
                                entry.seq
                            );
                            report.corrupt_records_detected += 1;
                            let total_len = file.metadata()?.len();
                            report.torn_tail_bytes_truncated = (total_len - start_offset) as usize;
                            file.set_len(start_offset)?;
                            file.seek(SeekFrom::Start(start_offset))?;
                            file.flush()?;
                            file.sync_all()?;
                            break;
                        }
                    }

                    // 3. Apply mutation with transactional validation
                    let is_checkpoint =
                        matches!(entry.mutation, JournalMutation::Checkpoint { .. });
                    match entry.mutation.apply(project) {
                        Ok(()) => {
                            if !is_checkpoint {
                                report.valid_records_replayed += 1;
                            }
                            report.last_seq = entry.seq;
                            self.current_seq = self.current_seq.max(entry.seq);
                            expected_seq = Some(entry.seq + 1);
                            last_valid_offset += bytes_read as u64;
                        }
                        Err(err) => {
                            report.failed_mutations += 1;
                            log::warn!(
                                "Project WAL mutation at seq {} rejected by timeline: {}. Preserving project state.",
                                entry.seq,
                                err
                            );
                            expected_seq = Some(entry.seq + 1);
                            last_valid_offset += bytes_read as u64;
                        }
                    }
                }
                Err(err) => {
                    // Encountered a malformed or torn JSON record
                    log::warn!(
                        "Project WAL recovery detected torn/malformed record at byte offset {}: {}. Truncating tail.",
                        start_offset,
                        err
                    );
                    report.corrupt_records_detected += 1;
                    let total_len = file.metadata()?.len();
                    report.torn_tail_bytes_truncated = (total_len - start_offset) as usize;
                    file.set_len(start_offset)?;
                    file.seek(SeekFrom::Start(start_offset))?;
                    file.flush()?;
                    file.sync_all()?;
                    break;
                }
            }
        }

        Ok(report)
    }

    /// Write an atomic project checkpoint and compact the transaction journal.
    pub fn checkpoint(
        &mut self,
        project: &Project,
        project_path: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        // 1. Atomic write of complete project snapshot
        project.save_atomic(project_path)?;

        // 2. Compact the journal: truncate and write Checkpoint sentinel
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.wal_path)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let sentinel = JournalEntry::new(
            self.current_seq,
            JournalMutation::Checkpoint {
                sequence: self.current_seq,
                snapshot_time_ms: now,
            },
        );

        let line = serde_json::to_string(&sentinel)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        writeln!(file, "{}", line)?;
        file.flush()?;
        file.sync_all()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::VideoClip;
    use crate::{ClipKind, Transform2D};

    fn make_test_clip(name: &str, pos_us: i64, dur_us: i64) -> Clip {
        Clip {
            id: ClipId(Uuid::new_v4()),
            kind: ClipKind::Video(VideoClip {
                source_path: name.into(),
                proxy_path: None,
                has_audio: false,
            }),
            position_us: pos_us,
            source_in_us: 0,
            source_out_us: dur_us,
            speed: 1.0,
            retime_mode: crate::clip::RetimeMode::default(),
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
    fn test_crc32_checksum_determinism_and_verification() {
        let data = b"Reddart Professional NLE Journal Verification";
        let c1 = crc32_checksum(data);
        let c2 = crc32_checksum(data);
        assert_eq!(c1, c2);
        assert_ne!(c1, 0);

        let mut corrupted = data.to_vec();
        corrupted[4] ^= 0xFF;
        assert_ne!(crc32_checksum(&corrupted), c1);
    }

    #[test]
    fn test_journal_append_and_replay() {
        let temp_dir = std::env::temp_dir().join(format!("reddart_wal_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let proj_path = temp_dir.join("test_proj.reddart");

        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();

        // Append AddTrack
        let tid = TrackId(10);
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "Video Master".into(),
            })
            .unwrap();

        // Append AddClip
        let clip_a = make_test_clip("scene_1.mp4", 0, 5_000_000);
        let clip_a_id = clip_a.id;
        journal
            .append(JournalMutation::AddClip {
                track_id: tid,
                clip: clip_a,
            })
            .unwrap();

        // Append MoveClip
        journal
            .append(JournalMutation::MoveClip {
                track_id: tid,
                clip_id: clip_a_id,
                new_position_us: 1_000_000,
            })
            .unwrap();

        // Append Marker
        journal
            .append(JournalMutation::AddMarker {
                id: 1,
                time_us: 2_000_000,
                name: "Cut Cue".into(),
                color_rgb: [220, 38, 38],
            })
            .unwrap();

        // Now replay into fresh project
        let mut restored_project = Project::new("Empty Base");
        let report = journal.replay_into(&mut restored_project).unwrap();

        assert_eq!(report.valid_records_replayed, 4);
        assert_eq!(report.torn_tail_bytes_truncated, 0);
        assert_eq!(restored_project.timeline.track_count(), 1);
        let track = restored_project.timeline.track(tid).unwrap();
        assert_eq!(track.clips().len(), 1);
        assert_eq!(track.clips()[0].position_us, 1_000_000);
        assert_eq!(restored_project.timeline.markers.len(), 1);
        assert_eq!(restored_project.timeline.markers[0].name, "Cut Cue");

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_journal_torn_write_recovery() {
        let temp_dir = std::env::temp_dir().join(format!("reddart_torn_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let proj_path = temp_dir.join("torn_proj.reddart");

        let tid = TrackId(1);
        let clip = make_test_clip("take1.mp4", 0, 3_000_000);

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
                .append(JournalMutation::AddClip {
                    track_id: tid,
                    clip,
                })
                .unwrap();
        }

        // Simulate a power failure / torn write by appending incomplete half-written JSON
        let wal_path = proj_path.with_extension("reddart.wal");
        {
            let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
            file.write_all(b"{\"seq\":3,\"tx_id\":\"incomplete-torn-record")
                .unwrap();
            file.flush().unwrap();
        }

        // Replay should safely truncate the torn tail and recover the 2 valid records
        let mut project = Project::new("Torn Recovery");
        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();
        let report = journal.replay_into(&mut project).unwrap();

        assert_eq!(report.valid_records_replayed, 2);
        assert!(report.torn_tail_bytes_truncated > 0);
        assert_eq!(project.timeline.track_count(), 1);
        assert_eq!(project.timeline.total_clip_count(), 1);

        // Subsequent append should work cleanly without any file corruption
        journal
            .append(JournalMutation::AddMarker {
                id: 1,
                time_us: 500_000,
                name: "Post Recovery Marker".into(),
                color_rgb: [56, 189, 248],
            })
            .unwrap();

        let mut re_restored = Project::new("Check After Append");
        let report2 = journal.replay_into(&mut re_restored).unwrap();
        assert_eq!(report2.valid_records_replayed, 3);
        assert_eq!(report2.torn_tail_bytes_truncated, 0);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_journal_checkpoint_and_compaction() {
        let temp_dir = std::env::temp_dir().join(format!("reddart_ckpt_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let proj_path = temp_dir.join("compact_proj.reddart");

        let mut journal = ProjectJournal::open_or_create(&proj_path).unwrap();

        let tid = TrackId(1);
        journal
            .append(JournalMutation::AddTrack {
                id: tid,
                kind: TrackKind::Video,
                name: "V1".into(),
            })
            .unwrap();

        let clip = make_test_clip("c.mp4", 0, 2_000_000);
        journal
            .append(JournalMutation::AddClip {
                track_id: tid,
                clip,
            })
            .unwrap();

        // Apply to project
        let mut test_proj = Project::new("Base");
        journal.replay_into(&mut test_proj).unwrap();

        // Checkpoint & compact
        journal.checkpoint(&test_proj, &proj_path).unwrap();

        // WAL file should now only contain the single Checkpoint sentinel line
        let file = File::open(journal.path()).unwrap();
        let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Checkpoint"));

        // Base project file exists on disk
        assert!(proj_path.exists());
        let loaded = Project::load_file(&proj_path).unwrap();
        assert_eq!(loaded.timeline.total_clip_count(), 1);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
