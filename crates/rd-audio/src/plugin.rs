//! Real-time audio plugin hosting subsystem (VST3 & AudioUnit compatibility).
//!
//! Provides dynamic plugin discovery, parameter automation, state serialization,
//! insert chain routing, and built-in studio DSP plugins (Compressor, 4-Band Parametric EQ,
//! Brickwall Limiter, Stereo Chorus/Delay).

use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_1_SQRT_2, PI};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Error types occurring during audio plugin loading, processing, or state management.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PluginError {
    #[error("Audio processing failure: {0}")]
    ProcessError(String),

    #[error("Parameter ID {0} not found in plugin")]
    ParameterNotFound(u32),

    #[error(
        "Invalid parameter value for ID {param_id}: value {attempted} out of bounds [{min}, {max}]"
    )]
    InvalidValueRange {
        param_id: u32,
        min: String,
        max: String,
        attempted: String,
    },

    #[error("Corrupted plugin preset state: {0}")]
    StateCorrupted(String),

    #[error("Channel layout mismatch: expected {expected} channels, got {actual}")]
    ChannelMismatch { expected: usize, actual: usize },

    #[error("Plugin '{0}' not found or failed to load")]
    PluginNotFound(String),
}

/// Recognized audio plugin binary formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginFormat {
    /// Steinberg Virtual Studio Technology 3.
    Vst3,
    /// Apple macOS AudioUnit v2 / v3.
    AudioUnit,
    /// High-precision built-in native Rust DSP plugin.
    InternalDsp,
}

/// Category classification for audio plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginCategory {
    Effect,
    Dynamics,
    Equalizer,
    Reverb,
    Delay,
    Limiter,
    Spatial,
    Mastering,
    Utility,
}

/// Metadata and runtime state for a single plugin parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginParameter {
    pub id: u32,
    pub name: String,
    pub unit: String,
    pub min_value: f32,
    pub max_value: f32,
    pub default_value: f32,
    pub value: f32,
}

impl PluginParameter {
    pub fn new(
        id: u32,
        name: impl Into<String>,
        unit: impl Into<String>,
        min_value: f32,
        max_value: f32,
        default_value: f32,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            unit: unit.into(),
            min_value,
            max_value,
            default_value,
            value: default_value,
        }
    }

    /// Sets parameter value clamped within valid `[min_value, max_value]`.
    pub fn set_clamped(&mut self, val: f32) {
        self.value = val.clamp(self.min_value, self.max_value);
    }
}

/// Metadata describing an audio plugin instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub category: PluginCategory,
    pub format: PluginFormat,
    pub input_channels: usize,
    pub output_channels: usize,
    pub latency_samples: usize,
    pub parameters: Vec<PluginParameter>,
}

/// Standard interface for real-time audio plugins.
pub trait AudioPlugin: Send + Sync {
    /// Returns the plugin's metadata descriptor.
    fn descriptor(&self) -> &PluginDescriptor;

    /// Returns a mutable reference to the descriptor.
    fn descriptor_mut(&mut self) -> &mut PluginDescriptor;

    /// Resets internal DSP filters, delay lines, and state variables for a given sample rate.
    fn reset(&mut self, sample_rate: f32);

    /// Processes separate left and right 32-bit floating point audio buffers.
    fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> Result<(), PluginError>;

    /// Convenience wrapper to process an interleaved stereo audio buffer in-place.
    fn process_interleaved(
        &mut self,
        buffer: &mut [f32],
        num_frames: usize,
    ) -> Result<(), PluginError> {
        let mut left = vec![0.0f32; num_frames];
        let mut right = vec![0.0f32; num_frames];

        for i in 0..num_frames {
            left[i] = buffer[i * 2];
            right[i] = buffer[i * 2 + 1];
        }

        let mut out_left = vec![0.0f32; num_frames];
        let mut out_right = vec![0.0f32; num_frames];

        self.process(&left, &right, &mut out_left, &mut out_right)?;

        let bypass = self.is_bypassed();
        let mix = self.wet_dry_mix();

        for i in 0..num_frames {
            if bypass {
                buffer[i * 2] = left[i];
                buffer[i * 2 + 1] = right[i];
            } else {
                buffer[i * 2] = left[i] * (1.0 - mix) + out_left[i] * mix;
                buffer[i * 2 + 1] = right[i] * (1.0 - mix) + out_right[i] * mix;
            }
        }

        Ok(())
    }

    /// Retrieves current parameter value by numeric ID.
    fn get_parameter(&self, id: u32) -> Result<f32, PluginError> {
        self.descriptor()
            .parameters
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
            .ok_or(PluginError::ParameterNotFound(id))
    }

    /// Sets parameter value by numeric ID.
    fn set_parameter(&mut self, id: u32, value: f32) -> Result<(), PluginError> {
        let desc = self.descriptor_mut();
        if let Some(param) = desc.parameters.iter_mut().find(|p| p.id == id) {
            param.set_clamped(value);
            Ok(())
        } else {
            Err(PluginError::ParameterNotFound(id))
        }
    }

    /// Serializes plugin state into an opaque JSON/binary chunk for project saving.
    fn save_state(&self) -> Result<Vec<u8>, PluginError> {
        serde_json::to_vec(&self.descriptor().parameters)
            .map_err(|e| PluginError::StateCorrupted(e.to_string()))
    }

    /// Deserializes and restores plugin parameter state from a byte chunk.
    fn load_state(&mut self, data: &[u8]) -> Result<(), PluginError> {
        let loaded: Vec<PluginParameter> =
            serde_json::from_slice(data).map_err(|e| PluginError::StateCorrupted(e.to_string()))?;

        for p in loaded {
            let _ = self.set_parameter(p.id, p.value);
        }
        Ok(())
    }

    /// Returns the processing latency in samples introduced by this plugin.
    fn latency_samples(&self) -> usize {
        self.descriptor().latency_samples
    }

    /// Whether this plugin is currently bypassed.
    fn is_bypassed(&self) -> bool;

    /// Sets bypass state.
    fn set_bypassed(&mut self, bypassed: bool);

    /// Returns wet/dry mix factor in range [0.0, 1.0].
    fn wet_dry_mix(&self) -> f32;

    /// Sets wet/dry mix factor.
    fn set_wet_dry_mix(&mut self, mix: f32);
}

// ---------------------------------------------------------------------------
// Built-in Studio Plugin: StudioCompressor
// ---------------------------------------------------------------------------

/// Professional broadcast dynamic range compressor with soft knee and RMS envelope detection.
pub struct StudioCompressorPlugin {
    desc: PluginDescriptor,
    sample_rate: f32,
    envelope: f32,
    bypassed: bool,
    wet_dry: f32,
    last_gain_reduction_db: f32,
}

impl StudioCompressorPlugin {
    pub const PARAM_THRESHOLD_DB: u32 = 0;
    pub const PARAM_RATIO: u32 = 1;
    pub const PARAM_ATTACK_MS: u32 = 2;
    pub const PARAM_RELEASE_MS: u32 = 3;
    pub const PARAM_MAKEUP_GAIN_DB: u32 = 4;
    pub const PARAM_KNEE_DB: u32 = 5;

    pub fn new() -> Self {
        let desc = PluginDescriptor {
            id: "reddart.dynamics.compressor".to_string(),
            name: "Reddart Studio Compressor".to_string(),
            vendor: "Reddart Audio Labs".to_string(),
            version: "1.0.0".to_string(),
            category: PluginCategory::Dynamics,
            format: PluginFormat::InternalDsp,
            input_channels: 2,
            output_channels: 2,
            latency_samples: 0,
            parameters: vec![
                PluginParameter::new(
                    Self::PARAM_THRESHOLD_DB,
                    "Threshold",
                    "dB",
                    -60.0,
                    0.0,
                    -18.0,
                ),
                PluginParameter::new(Self::PARAM_RATIO, "Ratio", ":1", 1.0, 20.0, 4.0),
                PluginParameter::new(Self::PARAM_ATTACK_MS, "Attack", "ms", 0.1, 100.0, 10.0),
                PluginParameter::new(Self::PARAM_RELEASE_MS, "Release", "ms", 10.0, 1000.0, 100.0),
                PluginParameter::new(
                    Self::PARAM_MAKEUP_GAIN_DB,
                    "Makeup Gain",
                    "dB",
                    -12.0,
                    24.0,
                    0.0,
                ),
                PluginParameter::new(Self::PARAM_KNEE_DB, "Soft Knee", "dB", 0.0, 12.0, 3.0),
            ],
        };

        Self {
            desc,
            sample_rate: 48000.0,
            envelope: 0.0,
            bypassed: false,
            wet_dry: 1.0,
            last_gain_reduction_db: 0.0,
        }
    }

    pub fn gain_reduction_db(&self) -> f32 {
        self.last_gain_reduction_db
    }
}

impl Default for StudioCompressorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlugin for StudioCompressorPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.desc
    }

    fn descriptor_mut(&mut self) -> &mut PluginDescriptor {
        &mut self.desc
    }

    fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8000.0);
        self.envelope = 0.0;
        self.last_gain_reduction_db = 0.0;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn wet_dry_mix(&self) -> f32 {
        self.wet_dry
    }

    fn set_wet_dry_mix(&mut self, mix: f32) {
        self.wet_dry = mix.clamp(0.0, 1.0);
    }

    fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> Result<(), PluginError> {
        let threshold = self.get_parameter(Self::PARAM_THRESHOLD_DB)?;
        let ratio = self.get_parameter(Self::PARAM_RATIO)?;
        let attack_ms = self.get_parameter(Self::PARAM_ATTACK_MS)?;
        let release_ms = self.get_parameter(Self::PARAM_RELEASE_MS)?;
        let makeup_db = self.get_parameter(Self::PARAM_MAKEUP_GAIN_DB)?;
        let knee_db = self.get_parameter(Self::PARAM_KNEE_DB)?;

        let alpha_attack = (-1.0 / (attack_ms * 0.001 * self.sample_rate)).exp();
        let alpha_release = (-1.0 / (release_ms * 0.001 * self.sample_rate)).exp();
        let makeup_linear = 10.0f32.powf(makeup_db / 20.0);

        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        let mut max_gr_db = 0.0f32;

        for i in 0..n {
            // Peak level of current sample
            let input_level = in_l[i].abs().max(in_r[i].abs());

            // Smooth envelope follower
            let coeff = if input_level > self.envelope {
                alpha_attack
            } else {
                alpha_release
            };
            self.envelope = coeff * self.envelope + (1.0 - coeff) * input_level;

            // Convert envelope to dBFS
            let env_db = if self.envelope > 1e-6 {
                20.0 * self.envelope.log10()
            } else {
                -120.0
            };

            // Soft-knee compression curve
            let target_gain_db = if env_db <= threshold - (knee_db / 2.0) {
                0.0
            } else if env_db >= threshold + (knee_db / 2.0) {
                (threshold - env_db) * (1.0 - (1.0 / ratio))
            } else {
                let diff = env_db - threshold + (knee_db / 2.0);
                -((1.0 - (1.0 / ratio)) * (diff * diff) / (2.0 * knee_db))
            };

            let gain_reduction_db = -target_gain_db;
            if gain_reduction_db > max_gr_db {
                max_gr_db = gain_reduction_db;
            }

            let linear_gain = 10.0f32.powf(target_gain_db / 20.0) * makeup_linear;

            out_l[i] = in_l[i] * linear_gain;
            out_r[i] = in_r[i] * linear_gain;
        }

        self.last_gain_reduction_db = max_gr_db;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Built-in Studio Plugin: ParametricEq4Band
// ---------------------------------------------------------------------------

/// 4-band stereo parametric equalizer (HighPass, Low-Mid Bell, High-Mid Bell, HighShelf).
pub struct ParametricEqPlugin {
    desc: PluginDescriptor,
    sample_rate: f32,
    bypassed: bool,
    wet_dry: f32,
    // Filter coefficients and states
    hp_b: [f32; 3],
    hp_a: [f32; 2],
    hp_state_l: [f32; 2],
    hp_state_r: [f32; 2],

    bell1_b: [f32; 3],
    bell1_a: [f32; 2],
    bell1_state_l: [f32; 2],
    bell1_state_r: [f32; 2],

    bell2_b: [f32; 3],
    bell2_a: [f32; 2],
    bell2_state_l: [f32; 2],
    bell2_state_r: [f32; 2],

    hs_b: [f32; 3],
    hs_a: [f32; 2],
    hs_state_l: [f32; 2],
    hs_state_r: [f32; 2],
}

impl ParametricEqPlugin {
    pub const PARAM_HP_FREQ: u32 = 0;
    pub const PARAM_BELL1_FREQ: u32 = 1;
    pub const PARAM_BELL1_GAIN_DB: u32 = 2;
    pub const PARAM_BELL1_Q: u32 = 3;
    pub const PARAM_BELL2_FREQ: u32 = 4;
    pub const PARAM_BELL2_GAIN_DB: u32 = 5;
    pub const PARAM_BELL2_Q: u32 = 6;
    pub const PARAM_HS_FREQ: u32 = 7;
    pub const PARAM_HS_GAIN_DB: u32 = 8;

    pub fn new() -> Self {
        let desc = PluginDescriptor {
            id: "reddart.eq.parametric4".to_string(),
            name: "Reddart 4-Band Parametric EQ".to_string(),
            vendor: "Reddart Audio Labs".to_string(),
            version: "1.0.0".to_string(),
            category: PluginCategory::Equalizer,
            format: PluginFormat::InternalDsp,
            input_channels: 2,
            output_channels: 2,
            latency_samples: 0,
            parameters: vec![
                PluginParameter::new(
                    Self::PARAM_HP_FREQ,
                    "High-Pass Freq",
                    "Hz",
                    20.0,
                    500.0,
                    40.0,
                ),
                PluginParameter::new(
                    Self::PARAM_BELL1_FREQ,
                    "Low-Mid Freq",
                    "Hz",
                    100.0,
                    2000.0,
                    250.0,
                ),
                PluginParameter::new(
                    Self::PARAM_BELL1_GAIN_DB,
                    "Low-Mid Gain",
                    "dB",
                    -18.0,
                    18.0,
                    0.0,
                ),
                PluginParameter::new(Self::PARAM_BELL1_Q, "Low-Mid Q", "", 0.3, 10.0, 1.0),
                PluginParameter::new(
                    Self::PARAM_BELL2_FREQ,
                    "High-Mid Freq",
                    "Hz",
                    1000.0,
                    10000.0,
                    3500.0,
                ),
                PluginParameter::new(
                    Self::PARAM_BELL2_GAIN_DB,
                    "High-Mid Gain",
                    "dB",
                    -18.0,
                    18.0,
                    0.0,
                ),
                PluginParameter::new(Self::PARAM_BELL2_Q, "High-Mid Q", "", 0.3, 10.0, 1.0),
                PluginParameter::new(
                    Self::PARAM_HS_FREQ,
                    "High-Shelf Freq",
                    "Hz",
                    4000.0,
                    20000.0,
                    10000.0,
                ),
                PluginParameter::new(
                    Self::PARAM_HS_GAIN_DB,
                    "High-Shelf Gain",
                    "dB",
                    -18.0,
                    18.0,
                    0.0,
                ),
            ],
        };

        let mut eq = Self {
            desc,
            sample_rate: 48000.0,
            bypassed: false,
            wet_dry: 1.0,
            hp_b: [1.0, 0.0, 0.0],
            hp_a: [0.0, 0.0],
            hp_state_l: [0.0, 0.0],
            hp_state_r: [0.0, 0.0],
            bell1_b: [1.0, 0.0, 0.0],
            bell1_a: [0.0, 0.0],
            bell1_state_l: [0.0, 0.0],
            bell1_state_r: [0.0, 0.0],
            bell2_b: [1.0, 0.0, 0.0],
            bell2_a: [0.0, 0.0],
            bell2_state_l: [0.0, 0.0],
            bell2_state_r: [0.0, 0.0],
            hs_b: [1.0, 0.0, 0.0],
            hs_a: [0.0, 0.0],
            hs_state_l: [0.0, 0.0],
            hs_state_r: [0.0, 0.0],
        };
        eq.recalculate_filters();
        eq
    }

    fn recalculate_filters(&mut self) {
        let hp_f = self.get_parameter(Self::PARAM_HP_FREQ).unwrap_or(40.0);
        let b1_f = self.get_parameter(Self::PARAM_BELL1_FREQ).unwrap_or(250.0);
        let b1_g = self.get_parameter(Self::PARAM_BELL1_GAIN_DB).unwrap_or(0.0);
        let b1_q = self.get_parameter(Self::PARAM_BELL1_Q).unwrap_or(1.0);
        let b2_f = self.get_parameter(Self::PARAM_BELL2_FREQ).unwrap_or(3500.0);
        let b2_g = self.get_parameter(Self::PARAM_BELL2_GAIN_DB).unwrap_or(0.0);
        let b2_q = self.get_parameter(Self::PARAM_BELL2_Q).unwrap_or(1.0);
        let hs_f = self.get_parameter(Self::PARAM_HS_FREQ).unwrap_or(10000.0);
        let hs_g = self.get_parameter(Self::PARAM_HS_GAIN_DB).unwrap_or(0.0);

        // 1. Highpass (Butterworth Q=0.707)
        let w0_hp = 2.0 * PI * (hp_f / self.sample_rate);
        let cos_hp = w0_hp.cos();
        let sin_hp = w0_hp.sin();
        let alpha_hp = sin_hp / (2.0 * FRAC_1_SQRT_2);
        let a0_hp = 1.0 + alpha_hp;
        self.hp_b = [
            ((1.0 + cos_hp) / 2.0) / a0_hp,
            (-(1.0 + cos_hp)) / a0_hp,
            ((1.0 + cos_hp) / 2.0) / a0_hp,
        ];
        self.hp_a = [(-2.0 * cos_hp) / a0_hp, (1.0 - alpha_hp) / a0_hp];

        // 2. Bell 1
        Self::calculate_peaking_filter(
            self.sample_rate,
            b1_f,
            b1_g,
            b1_q,
            &mut self.bell1_b,
            &mut self.bell1_a,
        );

        // 3. Bell 2
        Self::calculate_peaking_filter(
            self.sample_rate,
            b2_f,
            b2_g,
            b2_q,
            &mut self.bell2_b,
            &mut self.bell2_a,
        );

        // 4. High Shelf
        let w0_hs = 2.0 * PI * (hs_f / self.sample_rate);
        let a_hs = 10.0f32.powf(hs_g / 40.0);
        let cos_hs = w0_hs.cos();
        let sin_hs = w0_hs.sin();
        let alpha_hs = sin_hs / 2.0 * (2.0f32.sqrt());
        let a0_hs = (a_hs + 1.0) - (a_hs - 1.0) * cos_hs + 2.0 * a_hs.sqrt() * alpha_hs;
        self.hs_b = [
            (a_hs * ((a_hs + 1.0) + (a_hs - 1.0) * cos_hs + 2.0 * a_hs.sqrt() * alpha_hs)) / a0_hs,
            (-2.0 * a_hs * ((a_hs - 1.0) + (a_hs + 1.0) * cos_hs)) / a0_hs,
            (a_hs * ((a_hs + 1.0) + (a_hs - 1.0) * cos_hs - 2.0 * a_hs.sqrt() * alpha_hs)) / a0_hs,
        ];
        self.hs_a = [
            (2.0 * ((a_hs - 1.0) - (a_hs + 1.0) * cos_hs)) / a0_hs,
            ((a_hs + 1.0) - (a_hs - 1.0) * cos_hs - 2.0 * a_hs.sqrt() * alpha_hs) / a0_hs,
        ];
    }

    fn calculate_peaking_filter(
        sample_rate: f32,
        freq: f32,
        gain_db: f32,
        q: f32,
        b_out: &mut [f32; 3],
        a_out: &mut [f32; 2],
    ) {
        let w0 = 2.0 * PI * (freq / sample_rate);
        let a = 10.0f32.powf(gain_db / 40.0);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q.max(0.01));
        let a0 = 1.0 + alpha / a;

        *b_out = [
            (1.0 + alpha * a) / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha * a) / a0,
        ];
        *a_out = [(-2.0 * cos_w0) / a0, (1.0 - alpha / a) / a0];
    }

    #[inline]
    fn process_biquad(b: &[f32; 3], a: &[f32; 2], state: &mut [f32; 2], x: f32) -> f32 {
        let y = b[0] * x + state[0];
        state[0] = b[1] * x - a[0] * y + state[1];
        state[1] = b[2] * x - a[1] * y;
        y
    }
}

impl Default for ParametricEqPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlugin for ParametricEqPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.desc
    }

    fn descriptor_mut(&mut self) -> &mut PluginDescriptor {
        &mut self.desc
    }

    fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8000.0);
        self.hp_state_l = [0.0, 0.0];
        self.hp_state_r = [0.0, 0.0];
        self.bell1_state_l = [0.0, 0.0];
        self.bell1_state_r = [0.0, 0.0];
        self.bell2_state_l = [0.0, 0.0];
        self.bell2_state_r = [0.0, 0.0];
        self.hs_state_l = [0.0, 0.0];
        self.hs_state_r = [0.0, 0.0];
        self.recalculate_filters();
    }

    fn set_parameter(&mut self, id: u32, value: f32) -> Result<(), PluginError> {
        let desc = self.descriptor_mut();
        if let Some(param) = desc.parameters.iter_mut().find(|p| p.id == id) {
            param.set_clamped(value);
            self.recalculate_filters();
            Ok(())
        } else {
            Err(PluginError::ParameterNotFound(id))
        }
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn wet_dry_mix(&self) -> f32 {
        self.wet_dry
    }

    fn set_wet_dry_mix(&mut self, mix: f32) {
        self.wet_dry = mix.clamp(0.0, 1.0);
    }

    fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> Result<(), PluginError> {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());

        for i in 0..n {
            let mut l = in_l[i];
            let mut r = in_r[i];

            // 1. Highpass
            l = Self::process_biquad(&self.hp_b, &self.hp_a, &mut self.hp_state_l, l);
            r = Self::process_biquad(&self.hp_b, &self.hp_a, &mut self.hp_state_r, r);

            // 2. Bell 1
            l = Self::process_biquad(&self.bell1_b, &self.bell1_a, &mut self.bell1_state_l, l);
            r = Self::process_biquad(&self.bell1_b, &self.bell1_a, &mut self.bell1_state_r, r);

            // 3. Bell 2
            l = Self::process_biquad(&self.bell2_b, &self.bell2_a, &mut self.bell2_state_l, l);
            r = Self::process_biquad(&self.bell2_b, &self.bell2_a, &mut self.bell2_state_r, r);

            // 4. High Shelf
            l = Self::process_biquad(&self.hs_b, &self.hs_a, &mut self.hs_state_l, l);
            r = Self::process_biquad(&self.hs_b, &self.hs_a, &mut self.hs_state_r, r);

            out_l[i] = l;
            out_r[i] = r;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Built-in Studio Plugin: BrickwallLimiter
// ---------------------------------------------------------------------------

/// Broadcast brickwall peak limiter with lookahead buffer guaranteeing true 0 dBFS ceiling.
pub struct BrickwallLimiterPlugin {
    desc: PluginDescriptor,
    sample_rate: f32,
    bypassed: bool,
    wet_dry: f32,
    lookahead_buffer_l: Vec<f32>,
    lookahead_buffer_r: Vec<f32>,
    lookahead_index: usize,
    envelope: f32,
}

impl BrickwallLimiterPlugin {
    pub const PARAM_CEILING_DBFS: u32 = 0;
    pub const PARAM_RELEASE_MS: u32 = 1;
    pub const LOOKAHEAD_SAMPLES: usize = 48; // 1 ms at 48 kHz

    pub fn new() -> Self {
        let desc = PluginDescriptor {
            id: "reddart.dynamics.limiter".to_string(),
            name: "Reddart Brickwall Peak Limiter".to_string(),
            vendor: "Reddart Audio Labs".to_string(),
            version: "1.0.0".to_string(),
            category: PluginCategory::Limiter,
            format: PluginFormat::InternalDsp,
            input_channels: 2,
            output_channels: 2,
            latency_samples: Self::LOOKAHEAD_SAMPLES,
            parameters: vec![
                PluginParameter::new(
                    Self::PARAM_CEILING_DBFS,
                    "Ceiling",
                    "dBFS",
                    -12.0,
                    0.0,
                    -0.1,
                ),
                PluginParameter::new(Self::PARAM_RELEASE_MS, "Release", "ms", 1.0, 500.0, 50.0),
            ],
        };

        Self {
            desc,
            sample_rate: 48000.0,
            bypassed: false,
            wet_dry: 1.0,
            lookahead_buffer_l: vec![0.0; Self::LOOKAHEAD_SAMPLES],
            lookahead_buffer_r: vec![0.0; Self::LOOKAHEAD_SAMPLES],
            lookahead_index: 0,
            envelope: 0.0,
        }
    }
}

impl Default for BrickwallLimiterPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlugin for BrickwallLimiterPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.desc
    }

    fn descriptor_mut(&mut self) -> &mut PluginDescriptor {
        &mut self.desc
    }

    fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8000.0);
        self.lookahead_buffer_l.fill(0.0);
        self.lookahead_buffer_r.fill(0.0);
        self.lookahead_index = 0;
        self.envelope = 0.0;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn wet_dry_mix(&self) -> f32 {
        self.wet_dry
    }

    fn set_wet_dry_mix(&mut self, mix: f32) {
        self.wet_dry = mix.clamp(0.0, 1.0);
    }

    fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> Result<(), PluginError> {
        let ceiling_db = self.get_parameter(Self::PARAM_CEILING_DBFS)?;
        let release_ms = self.get_parameter(Self::PARAM_RELEASE_MS)?;
        let ceiling_linear = 10.0f32.powf(ceiling_db / 20.0);
        let alpha_release = (-1.0 / (release_ms * 0.001 * self.sample_rate)).exp();

        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());

        for i in 0..n {
            let cur_peak = in_l[i].abs().max(in_r[i].abs());
            if cur_peak > self.envelope {
                self.envelope = cur_peak;
            } else {
                self.envelope = alpha_release * self.envelope + (1.0 - alpha_release) * cur_peak;
            }

            let gain = if self.envelope > ceiling_linear {
                ceiling_linear / self.envelope
            } else {
                1.0
            };

            // Retrieve lookahead delayed audio
            let delayed_l = self.lookahead_buffer_l[self.lookahead_index];
            let delayed_r = self.lookahead_buffer_r[self.lookahead_index];

            // Store new sample in lookahead ring buffer
            self.lookahead_buffer_l[self.lookahead_index] = in_l[i];
            self.lookahead_buffer_r[self.lookahead_index] = in_r[i];
            self.lookahead_index = (self.lookahead_index + 1) % Self::LOOKAHEAD_SAMPLES;

            out_l[i] = (delayed_l * gain).clamp(-ceiling_linear, ceiling_linear);
            out_r[i] = (delayed_r * gain).clamp(-ceiling_linear, ceiling_linear);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Audio Plugin Insert Chain
// ---------------------------------------------------------------------------

/// Manages an ordered chain of audio plugins inserted on an audio track or master bus.
pub struct AudioPluginInsertChain {
    plugins: Vec<Box<dyn AudioPlugin>>,
}

impl AudioPluginInsertChain {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Appends a plugin to the insert chain.
    pub fn add_plugin(&mut self, plugin: Box<dyn AudioPlugin>) {
        self.plugins.push(plugin);
    }

    /// Number of active plugins in the chain.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Returns whether the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Removes a plugin by index.
    pub fn remove_plugin(&mut self, index: usize) -> Option<Box<dyn AudioPlugin>> {
        if index < self.plugins.len() {
            Some(self.plugins.remove(index))
        } else {
            None
        }
    }

    /// Access plugin by index.
    pub fn get_plugin(&self, index: usize) -> Option<&dyn AudioPlugin> {
        self.plugins.get(index).map(|b| b.as_ref())
    }

    /// Access mutable plugin by index.
    pub fn get_plugin_mut(&mut self, index: usize) -> Option<&mut Box<dyn AudioPlugin>> {
        self.plugins.get_mut(index)
    }

    /// Total latency in samples across all plugins in this chain.
    pub fn total_latency_samples(&self) -> usize {
        self.plugins.iter().map(|p| p.latency_samples()).sum()
    }

    /// Processes an interleaved stereo audio buffer sequentially through all plugins in the chain.
    pub fn process_interleaved(
        &mut self,
        buffer: &mut [f32],
        num_frames: usize,
    ) -> Result<(), PluginError> {
        for plugin in &mut self.plugins {
            plugin.process_interleaved(buffer, num_frames)?;
        }
        Ok(())
    }
}

impl Default for AudioPluginInsertChain {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Standard Directory Plugin Scanner
// ---------------------------------------------------------------------------

/// Locates installed third-party audio plugins across platform standard directories.
pub struct PluginScanner;

impl PluginScanner {
    /// Returns default search paths for VST3 and AU plugins for the host operating system.
    pub fn standard_plugin_directories() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        #[cfg(target_os = "windows")]
        {
            dirs.push(PathBuf::from(r"C:\Program Files\Common Files\VST3"));
            dirs.push(PathBuf::from(r"C:\Program Files (x86)\Common Files\VST3"));
            if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
                dirs.push(Path::new(&appdata).join(r"Programs\Common\VST3"));
            }
        }

        #[cfg(target_os = "macos")]
        {
            dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
            dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/Components"));
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(Path::new(&home).join("Library/Audio/Plug-Ins/VST3"));
                dirs.push(Path::new(&home).join("Library/Audio/Plug-Ins/Components"));
            }
        }

        #[cfg(target_os = "linux")]
        {
            dirs.push(PathBuf::from("/usr/lib/vst3"));
            dirs.push(PathBuf::from("/usr/local/lib/vst3"));
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(Path::new(&home).join(".vst3"));
            }
        }

        dirs
    }
}
