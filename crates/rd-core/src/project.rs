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
}
