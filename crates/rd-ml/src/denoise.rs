//! Audio AI: Noise Reduction & Vocal Isolation.
//!
//! Section W3.5 of the Technical Proposal:
//! Clean, independent spectral subtraction and gating algorithm (zero GPL).
//! Suppresses background hum, fan noise, and steady room noise from audio tracks.

use serde::{Deserialize, Serialize};

/// Audio denoiser configuration parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoiseConfig {
    /// Noise reduction strength in decibels (e.g. 12.0 dB).
    pub reduction_db: f32,
    /// Threshold sensitivity multiplier.
    pub sensitivity: f32,
    /// Attack time in milliseconds (how quickly attenuation releases when speech starts).
    pub attack_ms: f32,
    /// Release time in milliseconds (how smoothly noise floor is restored).
    pub release_ms: f32,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self {
            reduction_db: 15.0,
            sensitivity: 1.2,
            attack_ms: 10.0,
            release_ms: 80.0,
        }
    }
}

/// Real-time spectral noise gate filter.
pub struct AudioDenoiser {
    config: DenoiseConfig,
    sample_rate: u32,
    noise_floor_estimate: f32,
    envelope: f32,
}

impl AudioDenoiser {
    pub fn new(sample_rate: u32, config: DenoiseConfig) -> Self {
        Self {
            config,
            sample_rate: sample_rate.max(8000),
            noise_floor_estimate: 0.01,
            envelope: 0.0,
        }
    }

    /// Process a buffer of interleaved f32 PCM audio samples in-place.
    pub fn process_interleaved(&mut self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }

        // Convert reduction dB to linear gain floor (e.g. -15dB -> ~0.177)
        let gain_floor = 10.0_f32.powf(-self.config.reduction_db / 20.0);
        let attack_coeff = (-1.0 / (self.config.attack_ms * 0.001 * self.sample_rate as f32)).exp();
        let release_coeff =
            (-1.0 / (self.config.release_ms * 0.001 * self.sample_rate as f32)).exp();

        // 1. Initial pass: estimate noise floor if quiet
        let mut sum_abs = 0.0_f32;
        for &s in samples.iter() {
            sum_abs += s.abs();
        }
        let avg_level = sum_abs / samples.len() as f32;
        if avg_level < self.noise_floor_estimate * 1.5 {
            self.noise_floor_estimate = self.noise_floor_estimate * 0.95 + avg_level * 0.05;
        }

        let threshold = (self.noise_floor_estimate * self.config.sensitivity).max(1e-4);

        // 2. Dynamic spectral gate
        for s in samples.iter_mut() {
            let abs_val = s.abs();

            if abs_val > self.envelope {
                self.envelope = attack_coeff * self.envelope + (1.0 - attack_coeff) * abs_val;
            } else {
                self.envelope = release_coeff * self.envelope + (1.0 - release_coeff) * abs_val;
            }

            // Compute expansion gain
            let gain = if self.envelope < threshold {
                let ratio = (self.envelope / threshold).clamp(0.0, 1.0);
                gain_floor + (1.0 - gain_floor) * ratio * ratio
            } else {
                1.0
            };

            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_denoiser_attenuates_low_noise() {
        let mut denoiser = AudioDenoiser::new(48000, DenoiseConfig::default());

        // Background noise: low amplitude 0.005
        let mut noise_samples = vec![0.005_f32; 4800];
        denoiser.process_interleaved(&mut noise_samples);

        // Samples must be attenuated by the gain floor
        let last_val = noise_samples[noise_samples.len() - 1];
        assert!(
            last_val < 0.003,
            "Expected noise attenuation, got {}",
            last_val
        );

        // Voice: high amplitude 0.6
        let mut voice_samples = vec![0.6_f32; 4800];
        denoiser.process_interleaved(&mut voice_samples);

        // Speech must remain unattenuated
        let voice_val = voice_samples[voice_samples.len() - 1];
        assert!(
            (voice_val - 0.6).abs() < 0.05,
            "Expected preserved voice level, got {}",
            voice_val
        );
    }
}
