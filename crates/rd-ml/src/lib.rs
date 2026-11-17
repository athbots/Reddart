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
pub mod inpainting;
pub mod reframe;
pub mod saliency;
pub mod seam_carving;
pub mod segmentation;
pub mod stabilize;
pub mod tracking;
pub mod whisper;

pub use captions::{
    CaptionSegment, CaptionStyle, CaptionValidationIssue, CaptionValidationSeverity, WordTiming,
    apply_captions_to_timeline, export_srt, export_timeline_subtitles_srt,
    export_timeline_subtitles_vtt, export_vtt, format_timestamp_srt, format_timestamp_vtt,
    generate_caption_track, parse_srt, parse_timestamp_srt, parse_timestamp_vtt, parse_vtt,
    resolve_caption_overlaps, sanitize_subtitle_text, validate_caption_segments, wrap_segments,
};
pub use denoise::{
    AudioDenoiser, BiquadFilter, DenoiseConfig, DenoiseMode, DenoiseProfile, HumNotchFilter,
    fft_radix2,
};
pub use inpainting::{
    InpaintConfig, InpaintMask, InpaintMode, OpticalFlowField, SpatialInpainter,
    TemporalVideoInpainter,
};
pub use reframe::{AutoReframer, ReframeConfig, ReframeTracks, TargetAspect};
pub use saliency::{SaliencyConfig, SaliencyDetector, SaliencyMap};
pub use seam_carving::SeamCarver;
pub use segmentation::{
    CompositeBackground, DespillMode, GuidedFilter, GuidedFilterMode, PortraitMattingConfig,
    PortraitMattingEngine, SegmentationMask, Trimap, VirtualGreenScreen, composite_segmented_frame,
    suppress_color_spill, suppress_color_spill_advanced,
};
pub use stabilize::{
    BorderFillMode, CameraTransform, CornerConfig, CumulativeTrajectory, DynamicMarginManager,
    FeaturePoint, FrameMotion, KalmanConfig, KalmanTrajectorySmoother, LucasKanadeConfig,
    MotionEstimator, OpticalFlowTracker, RansacConfig, ShiTomasiCornerDetector,
    StabilizationQualityReport, TrajectoryPoint, VideoStabilizer, compute_luminance,
    sample_bilinear_luma,
};
pub use tracking::{BoundingBox, TrackedPath};
pub use whisper::{
    AcousticAligner, AudioPreprocessor, LogMelSpectrogram, SpeechBoundary, VoiceActivityDetector,
    WHISPER_FRAME_SIZE, WHISPER_HOP_SIZE, WHISPER_N_FFT, WHISPER_N_MELS, WHISPER_SAMPLE_RATE,
    WhisperConfig, WhisperEngine, WhisperModelSize, WhisperStatus, compute_log_mel_spectrogram,
    create_mel_filterbank, hz_to_mel, mel_to_hz,
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
