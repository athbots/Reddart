//! Low-latency hardware audio engine powered by CPAL.
//!
//! Streams real-time 48 kHz stereo floating-point PCM audio to the default
//! audio endpoint (WASAPI on Windows), computes true peak & RMS dBFS levels,
//! and synchronizes with the master AudioClock.

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use log::{info, warn};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::clock::AudioClock;
use crate::eq::ParametricEqualizer;
use crate::mixer::AudioMixer;
use crate::spectrum::{AudioSampleRing, SpectrumAnalyzer};
use crate::{CHANNELS, SAMPLE_RATE};
use rd_core::TimestampUs;

/// Lock-free atomic representation of an f32.
#[derive(Debug)]
pub struct AtomicF32 {
    bits: AtomicU32,
}

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        Self {
            bits: AtomicU32::new(val.to_bits()),
        }
    }

    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.bits.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn store(&self, val: f32) {
        self.bits.store(val.to_bits(), Ordering::Relaxed);
    }
}

/// Convert linear amplitude (0.0 to 1.0+) to standard decibels relative to Full Scale (dBFS).
#[inline]
pub fn amp_to_dbfs(amp: f32) -> f32 {
    if amp <= 0.000_001 {
        -96.0 // digital noise floor
    } else {
        (20.0 * amp.log10()).max(-96.0)
    }
}

/// Real-time audio telemetry metrics computed in the high-priority render callback.
pub struct AudioTelemetry {
    pub peak_l_dbfs: AtomicF32,
    pub peak_r_dbfs: AtomicF32,
    pub rms_l_dbfs: AtomicF32,
    pub rms_r_dbfs: AtomicF32,
    pub peak_hold_l_dbfs: AtomicF32,
    pub peak_hold_r_dbfs: AtomicF32,
    pub clipped_l: AtomicBool,
    pub clipped_r: AtomicBool,
}

impl AudioTelemetry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            peak_l_dbfs: AtomicF32::new(-96.0),
            peak_r_dbfs: AtomicF32::new(-96.0),
            rms_l_dbfs: AtomicF32::new(-96.0),
            rms_r_dbfs: AtomicF32::new(-96.0),
            peak_hold_l_dbfs: AtomicF32::new(-96.0),
            peak_hold_r_dbfs: AtomicF32::new(-96.0),
            clipped_l: AtomicBool::new(false),
            clipped_r: AtomicBool::new(false),
        })
    }

    /// Reset clipping indicators.
    pub fn reset_clip(&self) {
        self.clipped_l.store(false, Ordering::Relaxed);
        self.clipped_r.store(false, Ordering::Relaxed);
    }
}

/// Instantaneous metering state snapshot for UI drawing.
#[derive(Debug, Clone, Copy)]
pub struct MeteringState {
    pub peak_l_dbfs: f32,
    pub peak_r_dbfs: f32,
    pub rms_l_dbfs: f32,
    pub rms_r_dbfs: f32,
    pub peak_hold_l_dbfs: f32,
    pub peak_hold_r_dbfs: f32,
    pub clipped_l: bool,
    pub clipped_r: bool,
}

/// Real-time audio playback and metering engine.
pub struct AudioEngine {
    clock: Arc<AudioClock>,
    mixer: Arc<Mutex<AudioMixer>>,
    _stream: Option<Stream>,
    telemetry: Arc<AudioTelemetry>,
    is_playing: Arc<AtomicBool>,
    sample_rate: u32,
    pub eq: Arc<Mutex<ParametricEqualizer>>,
    sample_ring: Arc<Mutex<AudioSampleRing>>,
    spectrum_analyzer: Arc<Mutex<SpectrumAnalyzer>>,
}

impl AudioEngine {
    /// Initialize audio subsystem with default host and audio endpoint.
    pub fn new(clock: Arc<AudioClock>) -> Result<Self> {
        let mixer = Arc::new(Mutex::new(AudioMixer::new(clock.clone())));
        let telemetry = AudioTelemetry::new();
        let is_playing = Arc::new(AtomicBool::new(false));
        let actual_sample_rate = SAMPLE_RATE;
        let eq = Arc::new(Mutex::new(ParametricEqualizer::new(
            actual_sample_rate as f32,
        )));
        let sample_ring = Arc::new(Mutex::new(AudioSampleRing::new(4096)));
        let spectrum_analyzer = Arc::new(Mutex::new(SpectrumAnalyzer::new(
            1024,
            actual_sample_rate as f32,
            128,
        )));

        let host = cpal::default_host();
        let device_opt = host.default_output_device();

        let mut stream_opt = None;

        if let Some(device) = device_opt {
            let device_name = device.name().unwrap_or_else(|_| "Unknown Device".into());
            info!("Found default audio output device: {}", device_name);

            // Attempt to build 48 kHz stereo output stream
            let config = StreamConfig {
                channels: CHANNELS,
                sample_rate: cpal::SampleRate(SAMPLE_RATE),
                buffer_size: cpal::BufferSize::Default,
            };

            let mixer_clone = mixer.clone();
            let telemetry_clone = telemetry.clone();
            let playing_clone = is_playing.clone();
            let eq_clone = eq.clone();
            let ring_clone = sample_ring.clone();

            let err_fn = |err| {
                warn!("CPAL audio stream error: {:?}", err);
            };

            let stream_res = device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if playing_clone.load(Ordering::Acquire) {
                        if let Ok(mut mix) = mixer_clone.try_lock() {
                            mix.mix_interleaved(data);
                        } else {
                            data.fill(0.0);
                        }
                    } else {
                        data.fill(0.0);
                    }

                    // Apply real-time parametric equalizer
                    if let Ok(mut eq_proc) = eq_clone.try_lock() {
                        eq_proc.process_interleaved(data);
                    }

                    // Push post-EQ audio samples to spectrum analyzer circular buffer
                    if let Ok(mut ring) = ring_clone.try_lock() {
                        ring.push_interleaved(data, 2);
                    }

                    // Compute true peak and RMS metrics per channel without allocation
                    let channels = 2usize;
                    let frames = data.len() / channels;

                    let mut max_l = 0.0_f32;
                    let mut max_r = 0.0_f32;
                    let mut sum_sq_l = 0.0_f32;
                    let mut sum_sq_r = 0.0_f32;

                    for frame in data.chunks_exact(channels) {
                        let l = frame[0].abs();
                        let r = frame[1].abs();
                        if l > max_l {
                            max_l = l;
                        }
                        if r > max_r {
                            max_r = r;
                        }
                        sum_sq_l += l * l;
                        sum_sq_r += r * r;
                    }

                    let rms_l = if frames > 0 {
                        (sum_sq_l / frames as f32).sqrt()
                    } else {
                        0.0
                    };
                    let rms_r = if frames > 0 {
                        (sum_sq_r / frames as f32).sqrt()
                    } else {
                        0.0
                    };

                    let peak_l_db = amp_to_dbfs(max_l);
                    let peak_r_db = amp_to_dbfs(max_r);
                    let rms_l_db = amp_to_dbfs(rms_l);
                    let rms_r_db = amp_to_dbfs(rms_r);

                    telemetry_clone.peak_l_dbfs.store(peak_l_db);
                    telemetry_clone.peak_r_dbfs.store(peak_r_db);
                    telemetry_clone.rms_l_dbfs.store(rms_l_db);
                    telemetry_clone.rms_r_dbfs.store(rms_r_db);

                    if max_l >= 0.999 {
                        telemetry_clone.clipped_l.store(true, Ordering::Relaxed);
                    }
                    if max_r >= 0.999 {
                        telemetry_clone.clipped_r.store(true, Ordering::Relaxed);
                    }
                },
                err_fn,
                None,
            );

            match stream_res {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        warn!("Failed to start CPAL audio stream: {:?}", e);
                    } else {
                        info!(
                            "CPAL real-time audio output stream started successfully at 48 kHz stereo"
                        );
                        stream_opt = Some(stream);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to build CPAL audio output stream: {:?}. Audio will run in clock-only mode.",
                        e
                    );
                }
            }
        } else {
            warn!("No audio output endpoint detected. Engine will run in clock-only mode.");
        }

        Ok(Self {
            clock,
            mixer,
            _stream: stream_opt,
            telemetry,
            is_playing,
            sample_rate: actual_sample_rate,
            eq,
            sample_ring,
            spectrum_analyzer,
        })
    }

    /// Set the consolidated timeline PCM audio buffer into the real-time mixer.
    pub fn set_timeline_pcm(&self, pcm_data: Vec<f32>) {
        if let Ok(mut mix) = self.mixer.lock() {
            mix.clear();
            mix.add_track(pcm_data, 1.0);
        }
    }

    /// Check whether a live CPAL hardware output stream is currently attached.
    pub fn has_hardware_stream(&self) -> bool {
        self._stream.is_some()
    }

    /// Check whether the audio engine is actively playing.
    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Acquire)
    }

    /// Load decoded PCM audio buffer into the real-time mixer.
    pub fn load_track_pcm(&self, pcm_data: Vec<f32>, volume: f32) {
        if let Ok(mut mix) = self.mixer.lock() {
            mix.add_track(pcm_data, volume);
        }
    }

    /// Clear all loaded audio tracks.
    pub fn clear_tracks(&self) {
        if let Ok(mut mix) = self.mixer.lock() {
            *mix = AudioMixer::new(self.clock.clone());
        }
    }

    /// Set master bus output gain (0.0 to 2.0).
    pub fn set_master_volume(&self, volume: f32) {
        if let Ok(mut mix) = self.mixer.lock() {
            mix.set_master_volume(volume);
        }
    }

    /// Reposition audio playback head to a timestamp in microseconds.
    pub fn seek_to_time_us(&self, timestamp_us: TimestampUs) {
        let sample_pos = ((timestamp_us.max(0) as f64 / 1_000_000.0)
            * self.sample_rate as f64
            * CHANNELS as f64)
            .round() as usize;

        if let Ok(mut mix) = self.mixer.lock() {
            mix.seek_to_sample(sample_pos);
        }
        self.clock.seek_to(timestamp_us);
    }

    /// Start audio playback.
    pub fn play(&self) {
        self.is_playing.store(true, Ordering::Release);
        self.clock.set_playing(true);
    }

    /// Pause audio playback.
    pub fn pause(&self) {
        self.is_playing.store(false, Ordering::Release);
        self.clock.set_playing(false);
    }

    /// Read dynamic telemetry metrics with studio meter ballistics (smooth decay and peak-hold).
    pub fn read_metering(&self) -> MeteringState {
        let raw_peak_l = self.telemetry.peak_l_dbfs.load();
        let raw_peak_r = self.telemetry.peak_r_dbfs.load();
        let rms_l = self.telemetry.rms_l_dbfs.load();
        let rms_r = self.telemetry.rms_r_dbfs.load();

        // Decay ballistics: 0.35 dB per frame decay (~21 dB/sec)
        let decay_step = 0.35_f32;

        let prev_hold_l = self.telemetry.peak_hold_l_dbfs.load();
        let new_hold_l = if raw_peak_l >= prev_hold_l {
            raw_peak_l
        } else {
            (prev_hold_l - decay_step).max(-96.0)
        };
        self.telemetry.peak_hold_l_dbfs.store(new_hold_l);

        let prev_hold_r = self.telemetry.peak_hold_r_dbfs.load();
        let new_hold_r = if raw_peak_r >= prev_hold_r {
            raw_peak_r
        } else {
            (prev_hold_r - decay_step).max(-96.0)
        };
        self.telemetry.peak_hold_r_dbfs.store(new_hold_r);

        MeteringState {
            peak_l_dbfs: raw_peak_l,
            peak_r_dbfs: raw_peak_r,
            rms_l_dbfs: rms_l,
            rms_r_dbfs: rms_r,
            peak_hold_l_dbfs: new_hold_l,
            peak_hold_r_dbfs: new_hold_r,
            clipped_l: self.telemetry.clipped_l.load(Ordering::Relaxed),
            clipped_r: self.telemetry.clipped_r.load(Ordering::Relaxed),
        }
    }

    /// Access reference to the master clock.
    pub fn clock(&self) -> Arc<AudioClock> {
        self.clock.clone()
    }

    /// Read real-time spectrum telemetry (31-band ISO dBFS and smooth 128-point log curve).
    pub fn read_spectrum(&self, dt_s: f32) -> ([f32; 31], Vec<f32>) {
        let mut ring_samples = [0.0f32; 1024];
        if let Ok(ring) = self.sample_ring.lock() {
            ring.read_latest(&mut ring_samples);
        }
        if let Ok(mut analyzer) = self.spectrum_analyzer.lock() {
            analyzer.process_samples(&ring_samples, dt_s);
            (*analyzer.bands(), analyzer.curve().to_vec())
        } else {
            ([-120.0; 31], vec![-120.0; 128])
        }
    }

    /// Update a specific band of the parametric equalizer.
    pub fn set_eq_band(&self, index: usize, freq_hz: f32, gain_db: f32, q: f32, enabled: bool) {
        if let Ok(mut eq) = self.eq.lock() {
            eq.set_band_params(index, freq_hz, gain_db, q, enabled);
        }
    }

    /// Set master parametric EQ enabled/bypass state.
    pub fn set_eq_enabled(&self, enabled: bool) {
        if let Ok(mut eq) = self.eq.lock() {
            eq.enabled = enabled;
        }
    }

    /// Set master parametric EQ output trim in decibels.
    pub fn set_eq_output_gain(&self, gain_db: f32) {
        if let Ok(mut eq) = self.eq.lock() {
            eq.output_gain_db = gain_db;
        }
    }

    /// Retrieve the composite frequency response curve for the parametric equalizer across `num_points`.
    pub fn get_eq_composite_curve(&self, num_points: usize) -> Vec<f32> {
        if let Ok(eq) = self.eq.lock() {
            eq.compute_composite_curve(num_points)
        } else {
            vec![0.0; num_points]
        }
    }

    /// Get a clone of the parametric equalizer state for inspection or serialization.
    pub fn get_eq(&self) -> ParametricEqualizer {
        if let Ok(eq) = self.eq.lock() {
            eq.clone()
        } else {
            ParametricEqualizer::new(self.sample_rate as f32)
        }
    }
}

// CPAL Stream is held for lifetime management; audio callback access is synchronized via Arc and atomics.
unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}
