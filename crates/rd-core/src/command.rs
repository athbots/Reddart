//! Command system: undo/redo via command pattern.
//!
//! Every mutation to the project goes through a Command. Commands are
//! reversible and composable. The CommandHistory maintains the undo/redo
//! stacks and triggers autosave on commit.

use serde::{Deserialize, Serialize};

use crate::project::Project;

/// A command that can be applied and reversed on a project.
///
/// Commands use a snapshot-based approach: each command stores the
/// minimal state needed to undo the operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Add a clip to a track.
    AddClip {
        track_id: crate::TrackId,
        clip: crate::Clip,
    },
    /// Remove a clip from a track.
    RemoveClip {
        track_id: crate::TrackId,
        clip: crate::Clip,
    },
    /// Move a clip to a new position on the timeline.
    MoveClip {
        track_id: crate::TrackId,
        clip_id: crate::ClipId,
        old_position_us: crate::TimestampUs,
        new_position_us: crate::TimestampUs,
    },
    /// Trim a clip's in-point.
    TrimIn {
        track_id: crate::TrackId,
        clip_id: crate::ClipId,
        old_source_in_us: crate::TimestampUs,
        new_source_in_us: crate::TimestampUs,
        old_position_us: crate::TimestampUs,
        new_position_us: crate::TimestampUs,
    },
    /// Trim a clip's out-point.
    TrimOut {
        track_id: crate::TrackId,
        clip_id: crate::ClipId,
        old_source_out_us: crate::TimestampUs,
        new_source_out_us: crate::TimestampUs,
    },
    /// Split a clip at a timestamp.
    SplitClip {
        track_id: crate::TrackId,
        original_clip_id: crate::ClipId,
        new_clip_id: crate::ClipId,
        split_time_us: crate::TimestampUs,
        /// Snapshot of the original clip before split (for undo).
        original_clip_snapshot: crate::Clip,
    },
    /// Change a clip's speed.
    SetSpeed {
        track_id: crate::TrackId,
        clip_id: crate::ClipId,
        old_speed: f64,
        new_speed: f64,
    },
    /// Change a clip's opacity.
    SetOpacity {
        track_id: crate::TrackId,
        clip_id: crate::ClipId,
        old_opacity: f32,
        new_opacity: f32,
    },
    /// Add a track.
    AddTrack {
        track_id: crate::TrackId,
        kind: crate::TrackKind,
        name: String,
        index: usize,
    },
    /// Remove a track (stores complete track for undo).
    RemoveTrack { track: crate::Track, index: usize },
    /// Relink media matching old_path to new_path across the project.
    RelinkMedia { old_path: String, new_path: String },
    /// A group of commands applied atomically.
    Compound {
        label: String,
        commands: Vec<Command>,
    },
}

impl Command {
    /// Apply this command to a project. Returns Ok if successful.
    pub fn apply(&self, project: &mut Project) -> Result<(), CommandError> {
        match self {
            Command::AddClip { track_id, clip } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                track.add_clip(clip.clone());
                Ok(())
            }
            Command::RemoveClip { track_id, clip } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                track
                    .remove_clip(clip.id)
                    .ok_or(CommandError::ClipNotFound(clip.id))?;
                Ok(())
            }
            Command::MoveClip {
                track_id,
                clip_id,
                new_position_us,
                ..
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                let clip = track
                    .clip_mut(*clip_id)
                    .ok_or(CommandError::ClipNotFound(*clip_id))?;
                clip.position_us = *new_position_us;
                Ok(())
            }
            Command::TrimIn {
                track_id,
                clip_id,
                new_source_in_us,
                new_position_us,
                ..
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                let clip = track
                    .clip_mut(*clip_id)
                    .ok_or(CommandError::ClipNotFound(*clip_id))?;
                clip.source_in_us = *new_source_in_us;
                clip.position_us = *new_position_us;
                Ok(())
            }
            Command::TrimOut {
                track_id,
                clip_id,
                new_source_out_us,
                ..
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                let clip = track
                    .clip_mut(*clip_id)
                    .ok_or(CommandError::ClipNotFound(*clip_id))?;
                clip.source_out_us = *new_source_out_us;
                Ok(())
            }
            Command::SplitClip {
                track_id,
                original_clip_id,
                split_time_us,
                ..
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                track
                    .split_clip_at(*original_clip_id, *split_time_us)
                    .ok_or(CommandError::SplitFailed)?;
                Ok(())
            }
            Command::SetSpeed {
                track_id,
                clip_id,
                new_speed,
                ..
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                let clip = track
                    .clip_mut(*clip_id)
                    .ok_or(CommandError::ClipNotFound(*clip_id))?;
                clip.speed = *new_speed;
                Ok(())
            }
            Command::SetOpacity {
                track_id,
                clip_id,
                new_opacity,
                ..
            } => {
                let track = project
                    .timeline
                    .track_mut(*track_id)
                    .ok_or(CommandError::TrackNotFound(*track_id))?;
                let clip = track
                    .clip_mut(*clip_id)
                    .ok_or(CommandError::ClipNotFound(*clip_id))?;
                clip.opacity = *new_opacity;
                Ok(())
            }
            Command::AddTrack {
                kind, name, index, ..
            } => {
                project.timeline.insert_track(*index, *kind, name.clone());
                Ok(())
            }
            Command::RemoveTrack { track, .. } => {
                project.timeline.remove_track(track.id);
                Ok(())
            }
            Command::RelinkMedia { old_path, new_path } => {
                project.relink_media(old_path, new_path);
                Ok(())
            }
            Command::Compound { commands, .. } => {
                for cmd in commands {
                    cmd.apply(project)?;
                }
                Ok(())
            }
        }
    }

    /// Create the inverse of this command (for undo).
    pub fn inverse(&self) -> Command {
        match self {
            Command::AddClip { track_id, clip } => Command::RemoveClip {
                track_id: *track_id,
                clip: clip.clone(),
            },
            Command::RemoveClip { track_id, clip } => Command::AddClip {
                track_id: *track_id,
                clip: clip.clone(),
            },
            Command::MoveClip {
                track_id,
                clip_id,
                old_position_us,
                new_position_us,
            } => Command::MoveClip {
                track_id: *track_id,
                clip_id: *clip_id,
                old_position_us: *new_position_us,
                new_position_us: *old_position_us,
            },
            Command::TrimIn {
                track_id,
                clip_id,
                old_source_in_us,
                new_source_in_us,
                old_position_us,
                new_position_us,
            } => Command::TrimIn {
                track_id: *track_id,
                clip_id: *clip_id,
                old_source_in_us: *new_source_in_us,
                new_source_in_us: *old_source_in_us,
                old_position_us: *new_position_us,
                new_position_us: *old_position_us,
            },
            Command::TrimOut {
                track_id,
                clip_id,
                old_source_out_us,
                new_source_out_us,
            } => Command::TrimOut {
                track_id: *track_id,
                clip_id: *clip_id,
                old_source_out_us: *new_source_out_us,
                new_source_out_us: *old_source_out_us,
            },
            Command::SplitClip {
                track_id,
                original_clip_id,
                new_clip_id,
                split_time_us,
                original_clip_snapshot,
            } => {
                // Undo split: remove the new clip, restore the original
                Command::Compound {
                    label: "Undo Split".into(),
                    commands: vec![
                        Command::RemoveClip {
                            track_id: *track_id,
                            clip: crate::Clip {
                                id: *new_clip_id,
                                ..original_clip_snapshot.clone()
                            },
                        },
                        // The original clip needs to be restored to its pre-split state.
                        // This is handled by removing both halves and re-adding the original.
                        Command::RemoveClip {
                            track_id: *track_id,
                            clip: crate::Clip {
                                id: *original_clip_id,
                                ..original_clip_snapshot.clone()
                            },
                        },
                        Command::AddClip {
                            track_id: *track_id,
                            clip: crate::Clip {
                                id: *original_clip_id,
                                source_in_us: original_clip_snapshot.source_in_us,
                                source_out_us: original_clip_snapshot.source_out_us
                                    + (*split_time_us - original_clip_snapshot.position_us),
                                ..original_clip_snapshot.clone()
                            },
                        },
                    ],
                }
            }
            Command::SetSpeed {
                track_id,
                clip_id,
                old_speed,
                new_speed,
            } => Command::SetSpeed {
                track_id: *track_id,
                clip_id: *clip_id,
                old_speed: *new_speed,
                new_speed: *old_speed,
            },
            Command::SetOpacity {
                track_id,
                clip_id,
                old_opacity,
                new_opacity,
            } => Command::SetOpacity {
                track_id: *track_id,
                clip_id: *clip_id,
                old_opacity: *new_opacity,
                new_opacity: *old_opacity,
            },
            Command::AddTrack {
                track_id,
                kind,
                name,
                index,
            } => Command::RemoveTrack {
                track: crate::Track::new(*track_id, *kind, name.clone()),
                index: *index,
            },
            Command::RemoveTrack { track, index } => Command::AddTrack {
                track_id: track.id,
                kind: track.kind,
                name: track.name.clone(),
                index: *index,
            },
            Command::RelinkMedia { old_path, new_path } => Command::RelinkMedia {
                old_path: new_path.clone(),
                new_path: old_path.clone(),
            },
            Command::Compound { label, commands } => Command::Compound {
                label: format!("Undo {}", label),
                commands: commands.iter().rev().map(|c| c.inverse()).collect(),
            },
        }
    }

    /// Convert this command into corresponding Write-Ahead Log journal mutations.
    pub fn to_mutations(&self) -> Vec<crate::journal::JournalMutation> {
        use crate::journal::JournalMutation;
        match self {
            Command::AddClip { track_id, clip } => vec![JournalMutation::AddClip {
                track_id: *track_id,
                clip: clip.clone(),
            }],
            Command::RemoveClip { track_id, clip } => vec![JournalMutation::RemoveClip {
                track_id: *track_id,
                clip_id: clip.id,
            }],
            Command::MoveClip {
                track_id,
                clip_id,
                new_position_us,
                ..
            } => vec![JournalMutation::MoveClip {
                track_id: *track_id,
                clip_id: *clip_id,
                new_position_us: *new_position_us,
            }],
            Command::TrimIn {
                track_id,
                clip_id,
                new_source_in_us,
                new_position_us,
                ..
            } => vec![JournalMutation::TrimClip {
                track_id: *track_id,
                clip_id: *clip_id,
                new_source_in_us: *new_source_in_us,
                new_source_out_us: -1,
                new_position_us: *new_position_us,
            }],
            Command::TrimOut {
                track_id,
                clip_id,
                new_source_out_us,
                ..
            } => vec![JournalMutation::TrimClip {
                track_id: *track_id,
                clip_id: *clip_id,
                new_source_in_us: -1,
                new_source_out_us: *new_source_out_us,
                new_position_us: -1,
            }],
            Command::SplitClip {
                track_id,
                original_clip_id,
                new_clip_id,
                split_time_us,
                original_clip_snapshot,
            } => vec![JournalMutation::SplitClip {
                track_id: *track_id,
                clip_id: *original_clip_id,
                split_time_us: *split_time_us,
                new_clip: crate::Clip {
                    id: *new_clip_id,
                    ..original_clip_snapshot.clone()
                },
            }],
            Command::SetSpeed {
                track_id,
                clip_id,
                new_speed,
                ..
            } => vec![JournalMutation::SetSpeed {
                track_id: *track_id,
                clip_id: *clip_id,
                new_speed: *new_speed,
            }],
            Command::SetOpacity {
                track_id,
                clip_id,
                new_opacity,
                ..
            } => vec![JournalMutation::SetOpacity {
                track_id: *track_id,
                clip_id: *clip_id,
                new_opacity: *new_opacity,
            }],
            Command::AddTrack {
                track_id,
                kind,
                name,
                ..
            } => vec![JournalMutation::AddTrack {
                id: *track_id,
                kind: *kind,
                name: name.clone(),
            }],
            Command::RemoveTrack { track, .. } => {
                vec![JournalMutation::RemoveTrack { id: track.id }]
            }
            Command::RelinkMedia { old_path, new_path } => {
                vec![JournalMutation::RelinkMedia {
                    old_path: old_path.clone(),
                    new_path: new_path.clone(),
                }]
            }
            Command::Compound { commands, .. } => {
                commands.iter().flat_map(|c| c.to_mutations()).collect()
            }
        }
    }
}

/// Command history managing undo/redo stacks.
#[derive(Debug)]
pub struct CommandHistory {
    /// Commands that have been applied (for undo).
    undo_stack: Vec<Command>,
    /// Commands that have been undone (for redo).
    redo_stack: Vec<Command>,
    /// Maximum undo depth.
    max_depth: usize,
}

impl CommandHistory {
    /// Create a new command history.
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    /// Execute a command and push it onto the undo stack.
    /// Clears the redo stack (forward history is lost).
    pub fn execute(&mut self, command: Command, project: &mut Project) -> Result<(), CommandError> {
        command.apply(project)?;
        self.undo_stack.push(command);
        self.redo_stack.clear();

        // Enforce max depth
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }

        project.touch();
        Ok(())
    }

    /// Undo the last command.
    pub fn undo(&mut self, project: &mut Project) -> Result<bool, CommandError> {
        if let Some(command) = self.undo_stack.pop() {
            let inverse = command.inverse();
            inverse.apply(project)?;
            self.redo_stack.push(command);
            project.touch();
            Ok(true)
        } else {
            Ok(false) // Nothing to undo
        }
    }

    /// Redo the last undone command.
    pub fn redo(&mut self, project: &mut Project) -> Result<bool, CommandError> {
        if let Some(command) = self.redo_stack.pop() {
            command.apply(project)?;
            self.undo_stack.push(command);
            project.touch();
            Ok(true)
        } else {
            Ok(false) // Nothing to redo
        }
    }

    /// Execute a command, persist mutations to the WAL journal with durability,
    /// and push to the undo stack.
    ///
    /// Transaction semantics:
    /// 1. Command is applied to the in-memory project.
    /// 2. Corresponding mutations are appended and synced to the durable WAL journal.
    /// 3. If journal append fails, the in-memory mutation is rolled back via the inverse command,
    ///    no entry is pushed to the undo stack, and `Err(CommandError::JournalError)` is returned.
    pub fn execute_with_journal(
        &mut self,
        command: Command,
        project: &mut Project,
        journal: &mut crate::journal::ProjectJournal,
    ) -> Result<(), CommandError> {
        command.apply(project)?;

        let mutations = command.to_mutations();
        for mutation in &mutations {
            if let Err(e) = journal.append(mutation.clone()) {
                // WAL failed: Roll back in-memory application so project state is preserved
                let _ = command.inverse().apply(project);
                log::error!("WAL append failed during execute_with_journal: {}", e);
                return Err(CommandError::JournalError(e));
            }
        }

        self.undo_stack.push(command);
        self.redo_stack.clear();

        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }

        project.touch();
        Ok(())
    }

    /// Undo the last command and record inverse mutations to the WAL journal.
    ///
    /// Transaction semantics:
    /// If journal persistence fails, the in-memory undo is rolled back by reapplying the command,
    /// the command remains on the undo stack, and `Err(CommandError::JournalError)` is returned.
    pub fn undo_with_journal(
        &mut self,
        project: &mut Project,
        journal: &mut crate::journal::ProjectJournal,
    ) -> Result<bool, CommandError> {
        if let Some(command) = self.undo_stack.pop() {
            let inverse = command.inverse();
            if let Err(e) = inverse.apply(project) {
                self.undo_stack.push(command);
                return Err(e);
            }

            for mutation in inverse.to_mutations() {
                if let Err(e) = journal.append(mutation) {
                    // Roll back in-memory inverse by reapplying original command
                    let _ = command.apply(project);
                    self.undo_stack.push(command);
                    log::error!("WAL append failed during undo_with_journal: {}", e);
                    return Err(CommandError::JournalError(e));
                }
            }

            self.redo_stack.push(command);
            project.touch();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Redo the last undone command and record mutations to the WAL journal.
    ///
    /// Transaction semantics:
    /// If journal persistence fails, the in-memory redo is rolled back by applying the inverse,
    /// the command remains on the redo stack, and `Err(CommandError::JournalError)` is returned.
    pub fn redo_with_journal(
        &mut self,
        project: &mut Project,
        journal: &mut crate::journal::ProjectJournal,
    ) -> Result<bool, CommandError> {
        if let Some(command) = self.redo_stack.pop() {
            if let Err(e) = command.apply(project) {
                self.redo_stack.push(command);
                return Err(e);
            }

            for mutation in command.to_mutations() {
                if let Err(e) = journal.append(mutation) {
                    // Roll back by applying inverse
                    let _ = command.inverse().apply(project);
                    self.redo_stack.push(command);
                    log::error!("WAL append failed during redo_with_journal: {}", e);
                    return Err(CommandError::JournalError(e));
                }
            }

            self.undo_stack.push(command);
            project.touch();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all undo and redo history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Number of commands in the undo stack.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of commands in the redo stack.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

/// Errors that can occur during command execution.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Track not found: {0:?}")]
    TrackNotFound(crate::TrackId),
    #[error("Clip not found: {0:?}")]
    ClipNotFound(crate::ClipId),
    #[error("Split failed: split point is outside clip range")]
    SplitFailed,
    #[error("WAL journal persistence failure: {0}")]
    JournalError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{ClipKind, VideoClip};
    use crate::track::TrackKind;

    fn make_test_project() -> Project {
        let mut project = Project::new("Test");
        project.timeline.add_track(TrackKind::Video, "Video 1");
        project
    }

    fn make_test_clip(position_us: i64) -> crate::Clip {
        crate::Clip {
            id: crate::ClipId(uuid::Uuid::new_v4()),
            kind: ClipKind::Video(VideoClip {
                source_path: "test.mp4".into(),
                proxy_path: None,
                has_audio: true,
            }),
            position_us,
            source_in_us: 0,
            source_out_us: 2_000_000,
            speed: 1.0,
            opacity: 1.0,
            transform: crate::clip::Transform2D::default(),
            effects: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            keyframes: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_undo_redo_add_clip() {
        let mut project = make_test_project();
        let mut history = CommandHistory::new(100);
        let track_id = project.timeline.tracks()[0].id;

        let clip = make_test_clip(0);
        let cmd = Command::AddClip {
            track_id,
            clip: clip.clone(),
        };

        // Apply
        history.execute(cmd, &mut project).unwrap();
        assert_eq!(project.timeline.tracks()[0].clips().len(), 1);

        // Undo
        history.undo(&mut project).unwrap();
        assert_eq!(project.timeline.tracks()[0].clips().len(), 0);

        // Redo
        history.redo(&mut project).unwrap();
        assert_eq!(project.timeline.tracks()[0].clips().len(), 1);
    }

    #[test]
    fn test_redo_cleared_on_new_command() {
        let mut project = make_test_project();
        let mut history = CommandHistory::new(100);
        let track_id = project.timeline.tracks()[0].id;

        let clip1 = make_test_clip(0);
        let clip2 = make_test_clip(3_000_000);

        history
            .execute(
                Command::AddClip {
                    track_id,
                    clip: clip1,
                },
                &mut project,
            )
            .unwrap();
        history.undo(&mut project).unwrap();
        assert!(history.can_redo());

        // New command clears redo
        history
            .execute(
                Command::AddClip {
                    track_id,
                    clip: clip2,
                },
                &mut project,
            )
            .unwrap();
        assert!(!history.can_redo());
    }

    #[test]
    fn test_command_journal_integration_undo_redo_recovery() {
        let dir = std::env::temp_dir().join(format!("reddart_cmd_wal_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let proj_path = dir.join("test_command_wal.reddart");

        let mut project = make_test_project();
        project.save_atomic(&proj_path).unwrap();

        let mut journal = crate::journal::ProjectJournal::open_or_create(&proj_path).unwrap();
        let mut history = CommandHistory::new(100);
        let track_id = project.timeline.tracks()[0].id;

        let clip = make_test_clip(1_000_000);
        let clip_id = clip.id;

        // 1. Add clip with journal
        history
            .execute_with_journal(
                Command::AddClip { track_id, clip },
                &mut project,
                &mut journal,
            )
            .unwrap();
        assert_eq!(project.timeline.tracks()[0].clips().len(), 1);

        // 2. Modify speed with journal
        history
            .execute_with_journal(
                Command::SetSpeed {
                    track_id,
                    clip_id,
                    old_speed: 1.0,
                    new_speed: 2.0,
                },
                &mut project,
                &mut journal,
            )
            .unwrap();
        assert_eq!(project.timeline.tracks()[0].clips()[0].speed, 2.0);

        // 3. Undo the speed change with journal
        let undone = history
            .undo_with_journal(&mut project, &mut journal)
            .unwrap();
        assert!(undone);
        assert_eq!(project.timeline.tracks()[0].clips()[0].speed, 1.0);

        // 4. Simulate crash: drop in-memory state and reload project from disk via Project::load_file_with_recovery
        drop(project);
        drop(journal);
        drop(history);

        let (recovered, report) = Project::load_file_with_recovery(&proj_path).unwrap();
        assert!(report.is_some());
        assert_eq!(recovered.timeline.tracks()[0].clips().len(), 1);
        assert_eq!(recovered.timeline.tracks()[0].clips()[0].speed, 1.0);
        assert_eq!(
            recovered.timeline.tracks()[0].clips()[0].position_us,
            1_000_000
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
