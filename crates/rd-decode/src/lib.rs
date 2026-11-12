//! # rd-decode: Video Decoder Abstraction
//!
//! Provides the decoder pool, proxy generation, predictive prefetch,
//! and platform-agnostic decoder interface.
//!
//! Technical Proposal Sections 3.1 & 3.2:
//! - All-intra 540p proxies for zero-cost scrubbing.
//! - Decoder pool with per-device concurrency caps.
//! - Predictive prefetch ring buffer.

pub mod ffmpeg;
pub mod pool;
pub mod prefetch;
pub mod proxy;
pub mod service;
pub mod synthetic;

pub use ffmpeg::{FfmpegDecoder, find_ffmpeg_executable, find_ffprobe_executable};
pub use prefetch::{PlayDirection, PrefetchRingBuffer};
pub use service::{AsyncDecoderService, FrameCache, FrameRequest, FrameResponse};
pub use synthetic::{PatternStyle, SyntheticDecoder};

use anyhow::Result;
use rd_core::{DurationUs, TimestampUs};

/// Decoded frame data (platform-agnostic representation).
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    /// Frame index in the source video.
    pub frame_index: i64,
    /// Presentation timestamp in microseconds.
    pub pts_us: TimestampUs,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Uncompressed RGBA pixel data (for CPU uploads / test stubs).
    pub rgba_data: Option<Vec<u8>>,
    /// Whether this is an IDR / intra keyframe.
    pub is_keyframe: bool,
}

/// Capabilities of a hardware or software decoder.
#[derive(Debug, Clone)]
pub struct DecoderCapabilities {
    pub max_width: u32,
    pub max_height: u32,
    pub max_instances: usize,
    pub supports_zero_copy: bool,
    pub codec_name: String,
}

/// Abstract video decoder interface implemented per-platform
/// (Media Foundation on Windows, MediaCodec on Android, VideoToolbox on Apple).
pub trait VideoDecoder: Send {
    /// Seek to a specific presentation timestamp in microseconds.
    fn seek(&mut self, timestamp_us: TimestampUs) -> Result<()>;

    /// Decode the next sequential frame.
    /// Returns Ok(None) at the end of the video stream.
    fn decode_next(&mut self) -> Result<Option<DecodedFrame>>;

    /// Total media duration in microseconds.
    fn duration_us(&self) -> DurationUs;

    /// Video native resolution (width, height).
    fn resolution(&self) -> (u32, u32);

    /// Average frame rate in frames per second.
    fn frame_rate(&self) -> f64;

    /// Decoder hardware capabilities.
    fn capabilities(&self) -> DecoderCapabilities;
}
