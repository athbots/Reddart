//! # rd-audio: Real-Time Audio Mixer
//!
//! Handles audio decoding to PCM, real-time mixing on a dedicated thread,
//! and provides the master clock that video follows.

pub mod clock;
pub mod decode;
pub mod engine;
pub mod eq;
pub mod mixer;
pub mod spatial;
pub mod spectrum;
pub mod timeline;

pub use clock::{AudioClock, SyncDecision};
pub use decode::decode_audio_to_pcm;
pub use engine::{AudioEngine, AudioTelemetry, MeteringState, amp_to_dbfs};
pub use eq::{BiquadCoeffs, BiquadState, EqBand, FilterType, ParametricEqualizer};
pub use mixer::AudioMixer;
pub use spatial::{
    ChannelLayout, ItuDownmixer, MultichannelMixer, MultichannelTelemetry, MultichannelTrack,
    SpatialPanner, SpatialParams,
};
pub use spectrum::{AudioSampleRing, FftPlan, ISO_31_FREQUENCIES, SpectrumAnalyzer, WindowType};
pub use timeline::TimelineAudioScheduler;

/// Audio sample rate in Hz.
pub const SAMPLE_RATE: u32 = 48_000;
/// Audio channels (stereo).
pub const CHANNELS: u16 = 2;
/// Audio buffer size in frames.
pub const BUFFER_FRAMES: u32 = 512;
