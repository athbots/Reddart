//! Robert Bristow-Johnson (RBJ) Audio EQ Cookbook Parametric Equalizer.
//!
//! Provides a mathematically rigorous 5-band studio-grade parametric equalizer
//! with Direct Form II Transposed (DF2T) execution, analytical transfer function
//! magnitude response evaluation for real-time UI curve tracing, and stereo processing.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Parametric EQ filter band types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    /// Resonant 2nd-order High-Pass / Low-Cut filter (12 dB/octave slope).
    LowCut,
    /// Low-frequency shelving filter.
    LowShelf,
    /// Peaking / Bell parametric equalizer with adjustable center frequency and bandwidth Q.
    Bell,
    /// High-frequency shelving filter.
    HighShelf,
    /// Resonant 2nd-order Low-Pass / High-Cut filter (12 dB/octave slope).
    HighCut,
    /// Notch / Band-stop rejection filter.
    Notch,
}

/// Normalized biquad filter transfer function coefficients:
/// $H(z) = \frac{b_0 + b_1 z^{-1} + b_2 z^{-2}}{1 + a_1 z^{-1} + a_2 z^{-2}}$
#[derive(Debug, Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

impl BiquadCoeffs {
    /// Compute normalized biquad coefficients for a specified filter topology.
    pub fn compute(
        filter_type: FilterType,
        freq_hz: f32,
        gain_db: f32,
        q: f32,
        sample_rate: f32,
    ) -> Self {
        // Clamp parameters to prevent numerical instability or Nyquist folding
        let nyquist = sample_rate * 0.499;
        let f0 = freq_hz.clamp(10.0, nyquist);
        let q_clamped = q.clamp(0.1, 20.0);
        let a_amp = 10.0_f32.powf(gain_db / 40.0); // sqrt(10^(gain/20))

        let omega = 2.0 * PI * f0 / sample_rate;
        let cos_w = omega.cos();
        let sin_w = omega.sin();
        let alpha = sin_w / (2.0 * q_clamped);

        let (b0_raw, b1_raw, b2_raw, a0_raw, a1_raw, a2_raw) = match filter_type {
            FilterType::LowCut => {
                let b0 = (1.0 + cos_w) * 0.5;
                let b1 = -(1.0 + cos_w);
                let b2 = (1.0 + cos_w) * 0.5;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighCut => {
                let b0 = (1.0 - cos_w) * 0.5;
                let b1 = 1.0 - cos_w;
                let b2 = (1.0 - cos_w) * 0.5;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::Bell => {
                let b0 = 1.0 + alpha * a_amp;
                let b1 = -2.0 * cos_w;
                let b2 = 1.0 - alpha * a_amp;
                let a0 = 1.0 + alpha / a_amp;
                let a1 = -2.0 * cos_w;
                let a2 = 1.0 - alpha / a_amp;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::LowShelf => {
                let sqrt_a = a_amp.sqrt();
                let beta = 2.0 * sqrt_a * alpha;
                let a_plus_1 = a_amp + 1.0;
                let a_minus_1 = a_amp - 1.0;

                let b0 = a_amp * (a_plus_1 - a_minus_1 * cos_w + beta);
                let b1 = 2.0 * a_amp * (a_minus_1 - a_plus_1 * cos_w);
                let b2 = a_amp * (a_plus_1 - a_minus_1 * cos_w - beta);
                let a0 = a_plus_1 + a_minus_1 * cos_w + beta;
                let a1 = -2.0 * (a_minus_1 + a_plus_1 * cos_w);
                let a2 = a_plus_1 + a_minus_1 * cos_w - beta;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighShelf => {
                let sqrt_a = a_amp.sqrt();
                let beta = 2.0 * sqrt_a * alpha;
                let a_plus_1 = a_amp + 1.0;
                let a_minus_1 = a_amp - 1.0;

                let b0 = a_amp * (a_plus_1 + a_minus_1 * cos_w + beta);
                let b1 = -2.0 * a_amp * (a_minus_1 + a_plus_1 * cos_w);
                let b2 = a_amp * (a_plus_1 + a_minus_1 * cos_w - beta);
                let a0 = a_plus_1 - a_minus_1 * cos_w + beta;
                let a1 = 2.0 * (a_minus_1 - a_plus_1 * cos_w);
                let a2 = a_plus_1 - a_minus_1 * cos_w - beta;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cos_w;
                let b2 = 1.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        let inv_a0 = 1.0 / a0_raw;
        Self {
            b0: b0_raw * inv_a0,
            b1: b1_raw * inv_a0,
            b2: b2_raw * inv_a0,
            a1: a1_raw * inv_a0,
            a2: a2_raw * inv_a0,
        }
    }

    /// Evaluate magnitude response in decibels ($20 \log_{10} |H(e^{j\omega})|$) at frequency `f_hz`.
    pub fn response_db(&self, f_hz: f32, sample_rate: f32) -> f32 {
        let omega = 2.0 * PI * f_hz / sample_rate;
        let cos1 = omega.cos();
        let sin1 = omega.sin();
        let cos2 = (2.0 * omega).cos();
        let sin2 = (2.0 * omega).sin();

        // Numerator: b0 + b1*e^(-jw) + b2*e^(-2jw)
        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = -self.b1 * sin1 - self.b2 * sin2;
        let num_mag_sq = num_re * num_re + num_im * num_im;

        // Denominator: 1.0 + a1*e^(-jw) + a2*e^(-2jw)
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = -self.a1 * sin1 - self.a2 * sin2;
        let den_mag_sq = den_re * den_re + den_im * den_im;

        if den_mag_sq <= 1e-20 {
            return 0.0;
        }

        let mag_sq = num_mag_sq / den_mag_sq;
        if mag_sq <= 1e-12 {
            -120.0
        } else {
            10.0 * mag_sq.log10()
        }
    }
}

/// Direct Form II Transposed Filter Channel State.
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadState {
    pub s1: f32,
    pub s2: f32,
}

impl BiquadState {
    /// Reset filter state delay registers.
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Process a single sample using Direct Form II Transposed structure:
    /// $y[n] = b_0 x[n] + s_1[n-1]$
    /// $s_1[n] = b_1 x[n] - a_1 y[n] + s_2[n-1]$
    /// $s_2[n] = b_2 x[n] - a_2 y[n]$
    #[inline]
    pub fn process_sample(&mut self, x: f32, coeffs: &BiquadCoeffs) -> f32 {
        let y = coeffs.b0 * x + self.s1;
        self.s1 = coeffs.b1 * x - coeffs.a1 * y + self.s2;
        self.s2 = coeffs.b2 * x - coeffs.a2 * y;

        // Flush subnormal floating point numbers to prevent CPU denormal stalls
        if self.s1.abs() < 1e-15 {
            self.s1 = 0.0;
        }
        if self.s2.abs() < 1e-15 {
            self.s2 = 0.0;
        }

        y
    }
}

/// Configuration and state for an individual parametric EQ band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBand {
    pub enabled: bool,
    pub filter_type: FilterType,
    pub frequency_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    #[serde(skip)]
    pub coeffs: BiquadCoeffs,
    #[serde(skip)]
    pub state_l: BiquadState,
    #[serde(skip)]
    pub state_r: BiquadState,
}

impl EqBand {
    pub fn new(filter_type: FilterType, freq_hz: f32, gain_db: f32, q: f32) -> Self {
        let mut band = Self {
            enabled: true,
            filter_type,
            frequency_hz: freq_hz,
            gain_db,
            q,
            coeffs: BiquadCoeffs::default(),
            state_l: BiquadState::default(),
            state_r: BiquadState::default(),
        };
        band.recompute(48000.0);
        band
    }

    /// Recompute biquad coefficients for the specified sample rate.
    pub fn recompute(&mut self, sample_rate: f32) {
        if self.enabled {
            self.coeffs = BiquadCoeffs::compute(
                self.filter_type,
                self.frequency_hz,
                self.gain_db,
                self.q,
                sample_rate,
            );
        } else {
            self.coeffs = BiquadCoeffs::default();
        }
    }

    /// Reset internal filter state registers.
    pub fn reset_state(&mut self) {
        self.state_l.reset();
        self.state_r.reset();
    }
}

/// Studio 5-Band Cinema-Grade Parametric Equalizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricEqualizer {
    pub enabled: bool,
    pub output_gain_db: f32,
    pub bands: Vec<EqBand>,
    #[serde(skip)]
    pub sample_rate: f32,
}

impl Default for ParametricEqualizer {
    fn default() -> Self {
        Self::new(48000.0)
    }
}

impl ParametricEqualizer {
    /// Create standard cinema 5-band mastering equalizer:
    /// Band 1: Low Cut (High Pass) at 40 Hz
    /// Band 2: Low Shelf at 100 Hz
    /// Band 3: Parametric Mid Bell at 1000 Hz
    /// Band 4: High Shelf at 8000 Hz
    /// Band 5: High Cut (Low Pass) at 18000 Hz
    pub fn new(sample_rate: f32) -> Self {
        let bands = vec![
            EqBand::new(FilterType::LowCut, 40.0, 0.0, 0.707),
            EqBand::new(FilterType::LowShelf, 100.0, 0.0, 0.707),
            EqBand::new(FilterType::Bell, 1000.0, 0.0, 1.414),
            EqBand::new(FilterType::HighShelf, 8000.0, 0.0, 0.707),
            EqBand::new(FilterType::HighCut, 18000.0, 0.0, 0.707),
        ];

        let mut eq = Self {
            enabled: true,
            output_gain_db: 0.0,
            bands,
            sample_rate,
        };
        eq.recompute_all();
        eq
    }

    /// Recompute all filter coefficients across all bands.
    pub fn recompute_all(&mut self) {
        for band in &mut self.bands {
            band.recompute(self.sample_rate);
        }
    }

    /// Reset filter memory across all bands.
    pub fn reset_state(&mut self) {
        for band in &mut self.bands {
            band.reset_state();
        }
    }

    /// Update band parameters and immediately refresh its biquad coefficients.
    pub fn set_band_params(
        &mut self,
        index: usize,
        freq_hz: f32,
        gain_db: f32,
        q: f32,
        enabled: bool,
    ) {
        if let Some(band) = self.bands.get_mut(index) {
            band.frequency_hz = freq_hz;
            band.gain_db = gain_db;
            band.q = q;
            band.enabled = enabled;
            band.recompute(self.sample_rate);
        }
    }

    /// Evaluate composite frequency response curve in decibels across `num_points` logarithmic sample frequencies.
    pub fn compute_composite_curve(&self, num_points: usize) -> Vec<f32> {
        let count = num_points.max(32);
        let mut curve = Vec::with_capacity(count);

        if !self.enabled {
            curve.resize(count, self.output_gain_db);
            return curve;
        }

        const F_MIN: f32 = 20.0;
        const F_MAX: f32 = 20_000.0;
        let log_ratio = (F_MAX / F_MIN).ln();

        for i in 0..count {
            let frac = i as f32 / ((count - 1) as f32);
            let f = F_MIN * (frac * log_ratio).exp();

            let mut total_db = self.output_gain_db;
            for band in &self.bands {
                if band.enabled {
                    total_db += band.coeffs.response_db(f, self.sample_rate);
                }
            }
            curve.push(total_db);
        }

        curve
    }

    /// Process interleaved stereo audio in-place.
    pub fn process_interleaved(&mut self, buffer: &mut [f32]) {
        if !self.enabled {
            return;
        }

        let out_amp = 10.0_f32.powf(self.output_gain_db / 20.0);

        for frame in buffer.chunks_exact_mut(2) {
            let mut left = frame[0];
            let mut right = frame[1];

            for band in &mut self.bands {
                if band.enabled {
                    left = band.state_l.process_sample(left, &band.coeffs);
                    right = band.state_r.process_sample(right, &band.coeffs);
                }
            }

            frame[0] = left * out_amp;
            frame[1] = right * out_amp;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biquad_peaking_filter_gain() {
        let sample_rate = 48000.0;
        let target_freq = 1000.0;
        let boost_db = 6.0;
        let q = 2.0;

        let coeffs = BiquadCoeffs::compute(FilterType::Bell, target_freq, boost_db, q, sample_rate);

        // At center frequency, response should equal boost_db (+6 dB)
        let response_center = coeffs.response_db(target_freq, sample_rate);
        assert!(
            (response_center - boost_db).abs() < 0.1,
            "Center response {:.2} dB deviates from expected {:.2} dB",
            response_center,
            boost_db
        );

        // Far away from center frequency, response should be near 0 dB
        let response_far_low = coeffs.response_db(50.0, sample_rate);
        let response_far_high = coeffs.response_db(15000.0, sample_rate);
        assert!(
            response_far_low.abs() < 0.5,
            "Far low response {:.2} dB should be ~0 dB",
            response_far_low
        );
        assert!(
            response_far_high.abs() < 0.5,
            "Far high response {:.2} dB should be ~0 dB",
            response_far_high
        );
    }

    #[test]
    fn test_biquad_low_cut_rolloff() {
        let sample_rate = 48000.0;
        let cutoff_hz = 100.0;
        let coeffs = BiquadCoeffs::compute(FilterType::LowCut, cutoff_hz, 0.0, 0.707, sample_rate);

        // At cutoff frequency, response is approx -3 dB (Butterworth)
        let response_cutoff = coeffs.response_db(cutoff_hz, sample_rate);
        assert!(
            (response_cutoff - (-3.0)).abs() < 0.5,
            "Cutoff response {:.2} dB should be approx -3 dB",
            response_cutoff
        );

        // One octave below cutoff (50 Hz), response should be approx -15 dB (-12 dB/oct rolloff)
        let response_octave_low = coeffs.response_db(50.0, sample_rate);
        assert!(
            response_octave_low < -12.0,
            "Octave low response {:.2} dB should have substantial attenuation",
            response_octave_low
        );

        // High frequency passband (10 kHz), response should be 0 dB
        let response_pass = coeffs.response_db(10000.0, sample_rate);
        assert!(
            response_pass.abs() < 0.1,
            "Passband response {:.2} dB should be 0 dB",
            response_pass
        );
    }

    #[test]
    fn test_composite_curve_generation() {
        let mut eq = ParametricEqualizer::new(48000.0);
        // Boost 1 kHz by +12 dB
        eq.set_band_params(2, 1000.0, 12.0, 1.414, true);

        let curve = eq.compute_composite_curve(128);
        assert_eq!(curve.len(), 128);

        // Find peak on the curve
        let mut peak_db = -100.0;
        for &db in &curve {
            if db > peak_db {
                peak_db = db;
            }
        }
        assert!(
            (peak_db - 12.0).abs() < 0.5,
            "Curve peak {:.2} dB should match EQ boost of 12 dB",
            peak_db
        );
    }

    #[test]
    fn test_parametric_eq_audio_processing() {
        let mut eq = ParametricEqualizer::new(48000.0);
        // Set a +6 dB bell boost at 1 kHz
        eq.set_band_params(2, 1000.0, 6.0, 1.414, true);

        // Generate 1 kHz sine stereo buffer
        let mut buffer = vec![0.0f32; 2048];
        for i in 0..1024 {
            let s = (2.0 * PI * 1000.0 * (i as f32) / 48000.0).sin() * 0.2;
            buffer[i * 2] = s;
            buffer[i * 2 + 1] = s;
        }

        let input_peak = 0.2_f32;
        eq.process_interleaved(&mut buffer);

        // Measure output peak in steady-state (last 256 samples)
        let mut out_max = 0.0_f32;
        for &val in &buffer[1500..] {
            if val.abs() > out_max {
                out_max = val.abs();
            }
        }

        // Expected output peak: 0.2 * 10^(6/20) ~= 0.2 * 1.995 = 0.399
        let expected_peak = input_peak * 10.0_f32.powf(6.0 / 20.0);
        assert!(
            (out_max - expected_peak).abs() < 0.05,
            "Output peak {:.3} deviates from expected boosted peak {:.3}",
            out_max,
            expected_peak
        );
    }
}
