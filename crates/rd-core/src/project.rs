//! Project: top-level container for a Reddart editing project.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{FrameRate, Resolution, Timeline};

/// Current project file format version.
pub const PROJECT_FORMAT_VERSION: u32 = 1;

/// A Reddart editing project.
///
/// Contains all data needed to reconstruct an edit: timeline, metadata,
/// and project settings. Serialized as versioned JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique project identifier.
    pub id: Uuid,
    /// Human-readable project name.
    pub name: String,
    /// File format version for migrations.
    pub format_version: u32,
    /// Project output resolution.
    pub resolution: Resolution,
    /// Project frame rate.
    pub frame_rate: FrameRate,
    /// The timeline containing all tracks and clips.
    pub timeline: Timeline,
    /// When the project was created (Unix timestamp ms).
    pub created_at: i64,
    /// When the project was last modified (Unix timestamp ms).
    pub modified_at: i64,
}

impl Project {
    /// Create a new empty project with default settings.
    pub fn new(name: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            format_version: PROJECT_FORMAT_VERSION,
            resolution: Resolution::default(),
            frame_rate: FrameRate::default(),
            timeline: Timeline::new(),
            created_at: now,
            modified_at: now,
        }
    }

    /// Serialize the project to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize a project from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Mark the project as modified (updates `modified_at`).
    pub fn touch(&mut self) {
        self.modified_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
    }

    /// Atomically save the project to disk (write to temp file then rename).
    /// This prevents corruption if the process or machine crashes mid-write.
    pub fn save_atomic(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        let json = self.to_json().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let temp_filename = format!(
            ".{}_{}.tmp",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("project"),
            Uuid::new_v4()
        );
        let temp_path = parent.join(temp_filename);

        // Write content to temporary file
        std::fs::write(&temp_path, json.as_bytes())?;

        // Atomic replace / rename
        if let Err(e) = std::fs::rename(&temp_path, path) {
            // Clean up temp file on failure
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }

        Ok(())
    }

    /// Load a project from a file.
    pub fn load_file(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Scan all clips and return paths of media files that do not exist on disk.
    pub fn find_missing_media(&self) -> Vec<std::path::PathBuf> {
        let mut missing = Vec::new();
        for track in self.timeline.tracks() {
            for clip in track.clips() {
                if let Some(src) = clip.source_path() {
                    let path = std::path::PathBuf::from(src);
                    if !path.exists() && !missing.contains(&path) {
                        missing.push(path);
                    }
                }
            }
        }
        missing
    }

    /// Relink media matching `old_path` to `new_path` across all tracks.
    /// Returns the number of clips updated.
    pub fn relink_media(&mut self, old_path: &str, new_path: &str) -> usize {
        let mut count = 0;
        for track in self.timeline.tracks_mut() {
            for clip in track.clips_mut() {
                if let Some(src) = clip.source_path() {
                    if src == old_path {
                        if clip.set_source_path(new_path) {
                            count += 1;
                        }
                    }
                }
            }
        }
        if count > 0 {
            self.touch();
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_roundtrip() {
        let project = Project::new("Test Project");
        let json = project.to_json().unwrap();
        let restored = Project::from_json(&json).unwrap();
        assert_eq!(project.id, restored.id);
        assert_eq!(project.name, restored.name);
        assert_eq!(project.format_version, restored.format_version);
    }

    #[test]
    fn test_project_defaults() {
        let project = Project::new("Default Test");
        assert_eq!(project.resolution, Resolution::HD_1080);
        assert_eq!(project.frame_rate, FrameRate::FPS_30);
        assert_eq!(project.format_version, PROJECT_FORMAT_VERSION);
    }

    #[test]
    fn test_project_save_atomic_and_load() {
        let dir = std::env::temp_dir().join(format!("reddart_test_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let proj_path = dir.join("test_project.reddart");

        let mut project = Project::new("Atomic Test");
        let tid = project.timeline.add_track(crate::TrackKind::Video, "V1");
        project.timeline.track_mut(tid).unwrap().add_clip(crate::Clip::new_video("missing_file.mp4", 0, 0, 5_000_000));

        // Save atomic
        project.save_atomic(&proj_path).unwrap();
        assert!(proj_path.exists());

        // Load file
        let loaded = Project::load_file(&proj_path).unwrap();
        assert_eq!(loaded.name, "Atomic Test");
        assert_eq!(loaded.timeline.total_clip_count(), 1);

        // Missing media check
        let missing = loaded.find_missing_media();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], std::path::PathBuf::from("missing_file.mp4"));

        // Relink media
        let mut relinked = loaded;
        let count = relinked.relink_media("missing_file.mp4", "new_valid_file.mp4");
        assert_eq!(count, 1);
        assert_eq!(relinked.timeline.tracks()[0].clips()[0].source_path().unwrap(), "new_valid_file.mp4");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
