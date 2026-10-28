//! # rd-ml: On-Device Machine Learning Runtime
//!
//! Wraps on-device ML inference for AI features:
//! - Auto captions with word-level timing (Whisper)
//! - Background removal / portrait segmentation (MODNet/MediaPipe class)
//! - Face and object tracking on 540p proxy frames
//! - Audio enhancement and noise reduction
//!
//! All inference runs locally on CPU/DirectML — zero cloud calls, zero server cost.

pub mod captions;
pub mod denoise;
pub mod reframe;
pub mod segmentation;
pub mod stabilize;
pub mod tracking;

pub use captions::{generate_caption_track, CaptionSegment, CaptionStyle, WordTiming};
pub use denoise::{AudioDenoiser, DenoiseConfig};
pub use reframe::{AutoReframer, TargetAspect};
pub use segmentation::SegmentationMask;
pub use stabilize::{FrameMotion, VideoStabilizer};
pub use tracking::{BoundingBox, TrackedPath};

/// Execution provider for on-device inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionProvider {
    Cpu,
    DirectML,
    Cuda,
}

/// On-device machine learning runtime manager.
pub struct MlRuntime {
    provider: ExecutionProvider,
    thread_count: usize,
}

impl MlRuntime {
    pub fn new(provider: ExecutionProvider) -> Self {
        Self {
            provider,
            thread_count: 4,
        }
    }

    pub fn provider(&self) -> ExecutionProvider {
        self.provider
    }

    pub fn set_threads(&mut self, threads: usize) {
        self.thread_count = threads.max(1);
    }
}

impl Default for MlRuntime {
    fn default() -> Self {
        Self::new(ExecutionProvider::DirectML)
    }
}
