//! # rd-core: Reddart Project Model
//!
//! This crate contains the core data model for the Reddart video editor:
//! - **Project**: Top-level container with metadata and timeline
//! - **Timeline**: Ordered tracks of video/audio clips
//! - **Clip**: Source reference with in/out points, position, effects
//! - **Command system**: Undo/redo via command pattern with immutable snapshots
//! - **Autosave**: Serialization to versioned JSON
//!
//! This crate is platform-independent and shared across all targets.

pub mod clip;
pub mod command;
pub mod journal;
pub mod keyframe;
pub mod project;
pub mod timeline;
pub mod track;

// Re-export key types at crate root
pub use clip::{
    AudioClip, Clip, ClipEffect, ClipId, ClipKind, CompoundClip, RetimeMode, TextClip, Transform2D,
    Vec2, VideoClip,
};
pub use command::{Command, CommandError, CommandHistory};
pub use journal::{JournalEntry, JournalMutation, ProjectJournal, ReplayReport, crc32_checksum};
pub use keyframe::{Easing, Keyframe, KeyframeTrack};
pub use project::{MissingMediaItem, Project};
pub use timeline::{Timeline, TimelineMarker};
pub use track::{Track, TrackId, TrackKind};

/// Duration in microseconds. Using i64 for arithmetic safety.
pub type DurationUs = i64;

/// Timestamp in microseconds from project start.
pub type TimestampUs = i64;

/// Frames per second as a rational number (numerator, denominator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_24: Self = Self { num: 24, den: 1 };
    pub const FPS_30: Self = Self { num: 30, den: 1 };
    pub const FPS_60: Self = Self { num: 60, den: 1 };
    pub const FPS_25: Self = Self { num: 25, den: 1 };
    pub const FPS_50: Self = Self { num: 50, den: 1 };
    pub const NTSC_30: Self = Self {
        num: 30000,
        den: 1001,
    };
    pub const NTSC_60: Self = Self {
        num: 60000,
        den: 1001,
    };

    /// Microseconds per frame.
    pub fn frame_duration_us(&self) -> DurationUs {
        (self.den as i64 * 1_000_000) / self.num as i64
    }

    /// Convert a frame index to a timestamp.
    pub fn frame_to_us(&self, frame: i64) -> TimestampUs {
        (frame * self.den as i64 * 1_000_000) / self.num as i64
    }

    /// Convert a timestamp to the nearest frame index.
    pub fn us_to_frame(&self, us: TimestampUs) -> i64 {
        (us * self.num as i64) / (self.den as i64 * 1_000_000)
    }

    /// Frames per second as f64.
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

impl Default for FrameRate {
    fn default() -> Self {
        Self::FPS_30
    }
}

/// Resolution in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub const HD_720: Self = Self {
        width: 1280,
        height: 720,
    };
    pub const HD_1080: Self = Self {
        width: 1920,
        height: 1080,
    };
    pub const UHD_4K: Self = Self {
        width: 3840,
        height: 2160,
    };
    pub const PROXY_540: Self = Self {
        width: 960,
        height: 540,
    };

    /// Aspect ratio as f64.
    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self::HD_1080
    }
}
