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
pub mod saliency;
pub mod segmentation;
pub mod stabilize;
pub mod tracking;
pub mod whisper;

pub use captions::{
    CaptionSegment, CaptionStyle, WordTiming, export_srt, export_vtt, format_timestamp_srt,
    format_timestamp_vtt, generate_caption_track, parse_srt, parse_timestamp_srt,
    parse_timestamp_vtt, parse_vtt, wrap_segments,
};
pub use denoise::{
    AudioDenoiser, BiquadFilter, DenoiseConfig, DenoiseMode, DenoiseProfile, HumNotchFilter,
    fft_radix2,
};
pub use reframe::{AutoReframer, ReframeConfig, ReframeTracks, TargetAspect};
pub use saliency::{SaliencyConfig, SaliencyDetector, SaliencyMap};
pub use segmentation::{
    CompositeBackground, GuidedFilter, PortraitMattingConfig, PortraitMattingEngine,
    SegmentationMask, Trimap, composite_segmented_frame, suppress_color_spill,
};
pub use stabilize::{FrameMotion, VideoStabilizer};
pub use tracking::{BoundingBox, TrackedPath};
pub use whisper::{
    AudioPreprocessor, SpeechBoundary, VoiceActivityDetector, WHISPER_FRAME_SIZE, WHISPER_HOP_SIZE,
    WHISPER_SAMPLE_RATE, WhisperConfig, WhisperEngine, WhisperModelSize, WhisperStatus,
};

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
