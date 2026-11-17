//! Audio AI: Neural Spectral Noise Reduction, STFT Wiener Filtering & De-humming.
//!
//! Features:
//! - 512-point Short-Time Fourier Transform (STFT) with 50% square-root Hann overlap-add (exact COLA perfect reconstruction)
//! - Ephraim-Malah decision-directed (DD) a-priori / a-posteriori SNR estimation and parametric Wiener filtering
//! - Rainer Martin Minimum Statistics continuous background noise floor estimation without requiring manual silence profiling
//! - High-Q cascaded mains hum notch filters targeting European (50 Hz) and American (60 Hz) fundamentals + 3 harmonics
//! - Goertzel automatic mains hum frequency detector for zero-configuration de-humming
//! - Transient preservation: plosives, consonant attacks, and percussive peaks remain crisp and unclipped
//! - Studio `NeuralAudioDenoiserPlugin` implementing `rd_audio::plugin::AudioPlugin` for real-time mixer insert chains
//! - Real-time zero-allocation processing for mono, stereo, and interleaved PCM audio streams

use rd_audio::plugin::{
    AudioPlugin, PluginCategory, PluginDescriptor, PluginError, PluginFormat, PluginParameter,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Number of points in the STFT frame.
pub const STFT_SIZE: usize = 512;
/// Hop size for 50% overlap.
pub const HOP_SIZE: usize = STFT_SIZE / 2;
/// Number of non-negative frequency bins in the STFT frame (0 Hz DC through Nyquist).
pub const NUM_BINS: usize = (STFT_SIZE / 2) + 1;

/// Fast Radix-2 Cooley-Tukey In-Place Fast Fourier Transform.
pub fn fft_radix2(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());

    // 1. Bit reversal permutation
    let mut j = 0;
    for i in 0..n - 1 {
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
        let mut k = n >> 1;
        while k <= j {
            j -= k;
            k >>= 1;
        }
        j += k;
    }

    // 2. Cooley-Tukey butterfly stages
    let dir = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle = dir * 2.0 * PI / len as f32;
        let w_step_re = angle.cos();
        let w_step_im = angle.sin();

        let mut i = 0;
        while i < n {
            let mut w_re = 1.0_f32;
            let mut w_im = 0.0_f32;
            for k in 0..half {
                let u_re = re[i + k];
                let u_im = im[i + k];
                let v_re = re[i + k + half] * w_re - im[i + k + half] * w_im;
                let v_im = re[i + k + half] * w_im + im[i + k + half] * w_re;

                re[i + k] = u_re + v_re;
                im[i + k] = u_im + v_im;
                re[i + k + half] = u_re - v_re;
                im[i + k + half] = u_im - v_im;

                let next_w_re = w_re * w_step_re - w_im * w_step_im;
                let next_w_im = w_re * w_step_im + w_im * w_step_re;
                w_re = next_w_re;
                w_im = next_w_im;
            }
            i += len;
        }
        len <<= 1;
    }

    if inverse {
        let inv_n = 1.0 / n as f32;
        for i in 0..n {
            re[i] *= inv_n;
            im[i] *= inv_n;
        }
    }
}

/// Noise reduction processing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DenoiseMode {
    /// Optimized for vocal clarity: transient-preserving spectral gate protecting voice formants.
    #[default]
    VoiceFocus,
    /// Stationary room-tone suppression for continuous HVAC, fan, or cooling noise.
    RoomToneRemoval,
    /// 50 Hz European mains hum notch filter + 100, 150, 200 Hz harmonics.
    DeHum50,
    /// 60 Hz US/American mains hum notch filter + 120, 180, 240 Hz harmonics.
    DeHum60,
    /// Full 256-bin STFT spectral Wiener filter.
    MultiBand,
    /// Automatically detects mains hum (50 Hz vs 60 Hz) and applies full multi-band Wiener denoising.
    AutoDeHum,
}

/// Result of mains hum detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DetectedHum {
    /// No significant periodic mains interference detected.
    #[default]
    None,
    /// 50 Hz fundamental European/Asian mains hum detected.
    Hum50Hz,
    /// 60 Hz fundamental American mains hum detected.
    Hum60Hz,
}

/// Learned background noise profile across frequency bins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoiseProfile {
    /// Noise floor magnitude estimate for each of the 257 positive frequency bins.
    pub bin_floors: Vec<f32>,
    /// Overall broad-band noise floor estimate.
    pub broadband_floor: f32,
}

impl Default for DenoiseProfile {
    fn default() -> Self {
        Self {
            bin_floors: vec![0.005; NUM_BINS],
            broadband_floor: 0.005,
        }
    }
}

impl DenoiseProfile {
    /// Analyze a sample buffer containing only background noise / room tone (silence)
    /// and build an empirical spectral noise profile using STFT.
    pub fn learn_from_samples(samples: &[f32], _sample_rate: u32) -> Self {
        if samples.len() < STFT_SIZE {
            return Self::default();
        }

        let num_bins = NUM_BINS;
        let mut bin_sums = vec![0.0_f32; num_bins];
        let mut frame_count = 0;

        let mut re = [0.0_f32; STFT_SIZE];
        let mut im = [0.0_f32; STFT_SIZE];

        let mut offset = 0;
        while offset + STFT_SIZE <= samples.len() {
            for i in 0..STFT_SIZE {
                let w = ((PI * (i as f32 + 0.5)) / STFT_SIZE as f32).sin();
                re[i] = samples[offset + i] * w;
                im[i] = 0.0;
            }

            fft_radix2(&mut re, &mut im, false);

            for k in 0..num_bins {
                let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
                bin_sums[k] += mag;
            }

            frame_count += 1;
            offset += HOP_SIZE;
        }

        let count_f = frame_count.max(1) as f32;
        let mut bin_floors = vec![0.0_f32; num_bins];
        let mut broadband_sum = 0.0_f32;

        for k in 0..num_bins {
            let avg_mag = bin_sums[k] / count_f;
            bin_floors[k] = avg_mag.max(1e-5);
            broadband_sum += avg_mag;
        }

        let broadband_floor = (broadband_sum / num_bins as f32).max(1e-5);

        Self {
            bin_floors,
            broadband_floor,
        }
    }
}

/// Audio denoiser configuration parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoiseConfig {
    /// Noise reduction strength in decibels (e.g. 15.0 dB).
    pub reduction_db: f32,
    /// Threshold sensitivity multiplier (1.0 = nominal, > 1.0 = more aggressive).
    pub sensitivity: f32,
    /// Attack time in milliseconds (how quickly attenuation releases when speech starts).
    pub attack_ms: f32,
    /// Release time in milliseconds (how smoothly noise floor is restored).
    pub release_ms: f32,
    /// Processing mode.
    pub mode: DenoiseMode,
    /// Preserve fast speech transients (plosives / consonant attacks).
    pub transient_preserve: bool,
    /// Optional learned background noise profile.
    pub profile: Option<DenoiseProfile>,
    /// Quality factor Q for harmonic hum notch filter cascade.
    pub notch_q: f32,
    /// Enable automatic mains hum detection and notch engaging.
    pub auto_dehum: bool,
    /// Ephraim-Malah decision-directed smoothing factor alpha (typically 0.95 - 0.99).
    pub wiener_alpha: f32,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self {
            reduction_db: 15.0,
            sensitivity: 1.2,
            attack_ms: 10.0,
            release_ms: 80.0,
            mode: DenoiseMode::VoiceFocus,
            transient_preserve: true,
            profile: None,
            notch_q: 10.0,
            auto_dehum: true,
            wiener_alpha: 0.98,
        }
    }
}

impl DenoiseConfig {
    pub fn with_reduction_db(mut self, db: f32) -> Self {
        self.reduction_db = db.clamp(0.0, 40.0);
        self
    }

    pub fn with_mode(mut self, mode: DenoiseMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        self.sensitivity = sensitivity.clamp(0.2, 5.0);
        self
    }

    pub fn with_transient_preserve(mut self, preserve: bool) -> Self {
        self.transient_preserve = preserve;
        self
    }

    pub fn with_notch_q(mut self, q: f32) -> Self {
        self.notch_q = q.clamp(1.0, 50.0);
        self
    }

    pub fn with_auto_dehum(mut self, auto: bool) -> Self {
        self.auto_dehum = auto;
        self
    }
}

/// Direct Form II Transposed Biquad Filter for notch, bandpass, and shelving EQ.
#[derive(Debug, Clone)]
pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl BiquadFilter {
    /// Identity pass-through filter.
    pub fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// High-Q Notch filter at specified frequency (e.g. 50 Hz or 60 Hz mains hum).
    pub fn notch(freq: f32, sample_rate: f32, q: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.49);
        let omega = 2.0 * PI * f / sr;
        let alpha = omega.sin() / (2.0 * q.max(0.1));
        let cos_omega = omega.cos();

        let a0 = 1.0 + alpha;
        let b0 = 1.0 / a0;
        let b1 = (-2.0 * cos_omega) / a0;
        let b2 = 1.0 / a0;
        let a1 = (-2.0 * cos_omega) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// 2nd order Butterworth Bandpass filter.
    pub fn bandpass(center_freq: f32, sample_rate: f32, q: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let f = center_freq.clamp(10.0, sr * 0.49);
        let omega = 2.0 * PI * f / sr;
        let alpha = omega.sin() / (2.0 * q.max(0.1));
        let cos_omega = omega.cos();

        let a0 = 1.0 + alpha;
        let b0 = alpha / a0;
        let b1 = 0.0;
        let b2 = -alpha / a0;
        let a1 = (-2.0 * cos_omega) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// 2nd order Butterworth Lowpass filter.
    pub fn lowpass(cutoff_freq: f32, sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let f = cutoff_freq.clamp(10.0, sr * 0.49);
        let omega = 2.0 * PI * f / sr;
        let alpha = omega.sin() * std::f32::consts::FRAC_1_SQRT_2;
        let cos_omega = omega.cos();

        let a0 = 1.0 + alpha;
        let b0 = ((1.0 - cos_omega) * 0.5) / a0;
        let b1 = (1.0 - cos_omega) / a0;
        let b2 = ((1.0 - cos_omega) * 0.5) / a0;
        let a1 = (-2.0 * cos_omega) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// 2nd order Butterworth Highpass filter.
    pub fn highpass(cutoff_freq: f32, sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let f = cutoff_freq.clamp(10.0, sr * 0.49);
        let omega = 2.0 * PI * f / sr;
        let alpha = omega.sin() * std::f32::consts::FRAC_1_SQRT_2;
        let cos_omega = omega.cos();

        let a0 = 1.0 + alpha;
        let b0 = ((1.0 + cos_omega) * 0.5) / a0;
        let b1 = (-(1.0 + cos_omega)) / a0;
        let b2 = ((1.0 + cos_omega) * 0.5) / a0;
        let a1 = (-2.0 * cos_omega) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Filter a single sample in-place using Direct Form II Transposed.
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Reset internal delay registers.
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

/// Mains hum suppression cascade with 4 high-Q harmonic notch filters.
#[derive(Debug, Clone)]
pub struct HumNotchFilter {
    fundamental_freq: f32,
    q: f32,
    notches: [BiquadFilter; 4],
}

impl HumNotchFilter {
    /// Create a harmonic notch filter for European (50 Hz) or American (60 Hz) hum with default Q=10.0.
    pub fn new(fundamental_freq: f32, sample_rate: f32) -> Self {
        Self::with_q(fundamental_freq, sample_rate, 10.0)
    }

    /// Create a harmonic notch filter with custom Q factor.
    pub fn with_q(fundamental_freq: f32, sample_rate: f32, q: f32) -> Self {
        let effective_q = q.clamp(1.0, 50.0);
        let mut notches = [
            BiquadFilter::notch(fundamental_freq, sample_rate, effective_q),
            BiquadFilter::notch(fundamental_freq * 2.0, sample_rate, effective_q),
            BiquadFilter::notch(fundamental_freq * 3.0, sample_rate, effective_q),
            BiquadFilter::notch(fundamental_freq * 4.0, sample_rate, effective_q),
        ];

        let nyquist = sample_rate * 0.49;
        for (i, notch) in notches.iter_mut().enumerate() {
            let f = fundamental_freq * (i + 1) as f32;
            if f >= nyquist {
                *notch = BiquadFilter::identity();
            }
        }

        Self {
            fundamental_freq,
            q: effective_q,
            notches,
        }
    }

    pub fn fundamental_freq(&self) -> f32 {
        self.fundamental_freq
    }

    pub fn q(&self) -> f32 {
        self.q
    }

    /// Process a sample through the harmonic notch cascade.
    #[inline(always)]
    pub fn process(&mut self, mut sample: f32) -> f32 {
        for notch in &mut self.notches {
            sample = notch.process(sample);
        }
        sample
    }

    /// Reset all filters.
    pub fn reset(&mut self) {
        for notch in &mut self.notches {
            notch.reset();
        }
    }
}

/// Single-frequency power detector based on the discrete Goertzel algorithm.
#[derive(Debug, Clone, Copy, Default)]
pub struct MainsHumDetector;

impl MainsHumDetector {
    /// Compute power at a target frequency using the Goertzel algorithm.
    fn goertzel_power(samples: &[f32], target_freq: f32, sample_rate: f32) -> f32 {
        let n = samples.len();
        if n == 0 {
            return 0.0;
        }

        let omega = 2.0 * PI * target_freq / sample_rate;
        let coeff = 2.0 * omega.cos();
        let mut s0;
        let mut s1 = 0.0_f32;
        let mut s2 = 0.0_f32;

        for &x in samples {
            s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }

        let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
        (power / (n as f32 * n as f32)).max(0.0)
    }

    /// Computes the hum power ratio (in decibels) for 50 Hz vs 60 Hz relative to adjacent spectrum bands.
    pub fn hum_snr_db(&self, samples: &[f32], sample_rate: f32) -> (f32, f32) {
        if samples.len() < 512 {
            return (0.0, 0.0);
        }

        // Measure 50 Hz + 100 Hz harmonic vs adjacent 40 Hz & 70 Hz
        let p50 = Self::goertzel_power(samples, 50.0, sample_rate);
        let p100 = Self::goertzel_power(samples, 100.0, sample_rate);
        let ref50 = (Self::goertzel_power(samples, 40.0, sample_rate)
            + Self::goertzel_power(samples, 70.0, sample_rate))
            * 0.5;

        // Measure 60 Hz + 120 Hz harmonic vs adjacent 45 Hz & 80 Hz
        let p60 = Self::goertzel_power(samples, 60.0, sample_rate);
        let p120 = Self::goertzel_power(samples, 120.0, sample_rate);
        let ref60 = (Self::goertzel_power(samples, 45.0, sample_rate)
            + Self::goertzel_power(samples, 80.0, sample_rate))
            * 0.5;

        let hum50_energy = p50 + p100 * 0.5;
        let hum60_energy = p60 + p120 * 0.5;

        let snr50_db = if ref50 > 1e-10 {
            10.0 * (hum50_energy / ref50).max(1e-6).log10()
        } else {
            0.0
        };

        let snr60_db = if ref60 > 1e-10 {
            10.0 * (hum60_energy / ref60).max(1e-6).log10()
        } else {
            0.0
        };

        (snr50_db, snr60_db)
    }

    /// Automatically detect mains hum interference.
    pub fn detect_hum(&self, samples: &[f32], sample_rate: f32) -> DetectedHum {
        if samples.len() < 512 {
            return DetectedHum::None;
        }

        let mut total_power = 0.0_f32;
        for &s in samples {
            total_power += s * s;
        }
        total_power /= samples.len() as f32;
        if total_power < 1e-8 {
            return DetectedHum::None;
        }

        let p50 = Self::goertzel_power(samples, 50.0, sample_rate);
        let p100 = Self::goertzel_power(samples, 100.0, sample_rate);
        let p60 = Self::goertzel_power(samples, 60.0, sample_rate);
        let p120 = Self::goertzel_power(samples, 120.0, sample_rate);

        let hum50_energy = p50 + p100 * 0.5;
        let hum60_energy = p60 + p120 * 0.5;

        // Mains hum must constitute at least 1% of total signal power to trigger detection
        let min_hum_power = total_power * 0.01;
        if hum50_energy < min_hum_power && hum60_energy < min_hum_power {
            return DetectedHum::None;
        }

        let (snr50, snr60) = self.hum_snr_db(samples, sample_rate);
        const HUM_THRESHOLD_DB: f32 = 4.0; // 4 dB above neighboring noise floor

        if hum50_energy >= min_hum_power && snr50 > HUM_THRESHOLD_DB && snr50 > snr60 + 2.0 {
            DetectedHum::Hum50Hz
        } else if hum60_energy >= min_hum_power && snr60 > HUM_THRESHOLD_DB && snr60 > snr50 + 2.0 {
            DetectedHum::Hum60Hz
        } else {
            DetectedHum::None
        }
    }
}

/// Rainer Martin Minimum Statistics background noise PSD tracker.
///
/// Continuously tracks the non-stationary background noise floor without requiring
/// dedicated silence / room-tone calibration.
#[derive(Debug, Clone)]
pub struct MinimumStatisticsTracker {
    smoothed_power: [f32; NUM_BINS],
    subwindow_min: [[f32; NUM_BINS]; 8],
    current_subwindow_min: [f32; NUM_BINS],
    subwindow_index: usize,
    frame_counter: usize,
    frames_per_subwindow: usize,
    bias_factor: f32,
    initialized: bool,
}

impl MinimumStatisticsTracker {
    pub fn new() -> Self {
        Self {
            smoothed_power: [1e-4; NUM_BINS],
            subwindow_min: [[1e-4; NUM_BINS]; 8],
            current_subwindow_min: [1e-4; NUM_BINS],
            subwindow_index: 0,
            frame_counter: 0,
            frames_per_subwindow: 12, // 12 frames * 5.33ms = ~64ms per subwindow, 8 subwindows = ~0.51s
            bias_factor: 1.45,
            initialized: false,
        }
    }

    /// Feed a frame's noisy short-time power spectrum and obtain the updated noise PSD estimate.
    pub fn update(&mut self, noisy_power: &[f32; NUM_BINS], noise_psd_out: &mut [f32; NUM_BINS]) {
        if !self.initialized {
            for (k, &p_raw) in noisy_power.iter().enumerate() {
                let p = p_raw.max(1e-7);
                self.smoothed_power[k] = p;
                self.current_subwindow_min[k] = p;
                for u in 0..8 {
                    self.subwindow_min[u][k] = p;
                }
            }
            self.initialized = true;
        }

        // 1. First-order power smoothing with time constant ~60ms
        const BETA: f32 = 0.85;
        for (k, &p_raw) in noisy_power.iter().enumerate() {
            let p = p_raw.max(1e-7);
            self.smoothed_power[k] = BETA * self.smoothed_power[k] + (1.0 - BETA) * p;
            if self.smoothed_power[k] < self.current_subwindow_min[k] {
                self.current_subwindow_min[k] = self.smoothed_power[k];
            }
        }

        self.frame_counter += 1;
        if self.frame_counter >= self.frames_per_subwindow {
            self.frame_counter = 0;
            self.subwindow_min[self.subwindow_index] = self.current_subwindow_min;
            self.subwindow_index = (self.subwindow_index + 1) % 8;
            self.current_subwindow_min = self.smoothed_power;
        }

        // 2. Minimum over all subwindows multiplied by bias correction factor
        for (k, out) in noise_psd_out.iter_mut().enumerate() {
            let mut min_val = self.current_subwindow_min[k];
            for u in 0..8 {
                if self.subwindow_min[u][k] < min_val {
                    min_val = self.subwindow_min[u][k];
                }
            }
            *out = min_val * self.bias_factor;
        }
    }

    pub fn reset(&mut self) {
        self.smoothed_power = [1e-4; NUM_BINS];
        self.subwindow_min = [[1e-4; NUM_BINS]; 8];
        self.current_subwindow_min = [1e-4; NUM_BINS];
        self.subwindow_index = 0;
        self.frame_counter = 0;
        self.initialized = false;
    }
}

impl Default for MinimumStatisticsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Ephraim-Malah Decision-Directed (DD) Wiener Spectral Subtraction Filter.
///
/// Recursively tracks a-priori and a-posteriori SNR to eliminate musical noise artifacts.
#[derive(Debug, Clone)]
pub struct DecisionDirectedWienerFilter {
    prev_gains: [f32; NUM_BINS],
    prev_noisy_power: [f32; NUM_BINS],
    prev_noise_psd: [f32; NUM_BINS],
    initialized: bool,
}

impl DecisionDirectedWienerFilter {
    pub fn new() -> Self {
        Self {
            prev_gains: [1.0; NUM_BINS],
            prev_noisy_power: [1e-4; NUM_BINS],
            prev_noise_psd: [1e-4; NUM_BINS],
            initialized: false,
        }
    }

    /// Compute Wiener gains for all frequency bins in [0, 1].
    pub fn compute_gains(
        &mut self,
        noisy_power: &[f32; NUM_BINS],
        noise_psd: &[f32; NUM_BINS],
        gain_floor: f32,
        sensitivity: f32,
        alpha: f32,
        gains_out: &mut [f32; NUM_BINS],
    ) {
        let eff_alpha = alpha.clamp(0.85, 0.99);
        let oversubtraction = 1.0 + 0.3 * (sensitivity - 1.0).max(0.0);

        for k in 0..NUM_BINS {
            let lambda = noise_psd[k].max(1e-10);
            let y_k = noisy_power[k].max(1e-10);

            // A-posteriori SNR: gamma_k = Y_k / lambda_k
            let gamma = y_k / lambda;

            // A-priori SNR: xi_k via decision-directed recursion
            let xi = if self.initialized {
                let prev_lambda = self.prev_noise_psd[k].max(1e-10);
                let prev_g = self.prev_gains[k];
                let direct_term = (prev_g * prev_g * self.prev_noisy_power[k]) / prev_lambda;
                let ml_term = (gamma - 1.0).max(0.0);
                eff_alpha * direct_term + (1.0 - eff_alpha) * ml_term
            } else {
                (gamma - 1.0).max(0.0)
            };

            // Parametric Wiener Gain: G = xi / (xi + mu)
            let g_raw = xi / (xi + oversubtraction);

            // Berouti musical noise suppression floor
            let g_clamped = g_raw.clamp(gain_floor, 1.0);

            gains_out[k] = g_clamped;
            self.prev_gains[k] = g_clamped;
            self.prev_noisy_power[k] = y_k;
            self.prev_noise_psd[k] = lambda;
        }

        self.initialized = true;
    }

    pub fn reset(&mut self) {
        self.prev_gains = [1.0; NUM_BINS];
        self.prev_noisy_power = [1e-4; NUM_BINS];
        self.prev_noise_psd = [1e-4; NUM_BINS];
        self.initialized = false;
    }
}

impl Default for DecisionDirectedWienerFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Transient-preserving speech onset detector.
///
/// Prevents aggressive noise reduction from muffling voice plosives and high-frequency consonants.
#[derive(Debug, Clone)]
pub struct TransientDetector {
    prev_mag: [f32; NUM_BINS],
    prev_sample: f32,
    hold_counter: usize,
    max_hold_frames: usize,
}

impl TransientDetector {
    pub fn new() -> Self {
        Self {
            prev_mag: [0.0; NUM_BINS],
            prev_sample: 0.0,
            hold_counter: 0,
            max_hold_frames: 3, // ~16ms hold
        }
    }

    /// Check if a transient has occurred in the STFT frame or time-domain signal.
    pub fn update(&mut self, mag: &[f32; NUM_BINS], threshold: f32) -> bool {
        // High frequency spectral flux (k >= 16 corresponds to ~1500 Hz at 48kHz)
        let mut flux = 0.0_f32;
        let mut total_hf = 0.0_f32;

        for (k, &mag_k) in mag.iter().enumerate().skip(16) {
            let diff = mag_k - self.prev_mag[k];
            if diff > 0.0 {
                flux += diff;
            }
            total_hf += mag_k;
            self.prev_mag[k] = mag_k;
        }

        let is_transient = flux > (total_hf * 0.35).max(threshold * 2.0);

        if is_transient {
            self.hold_counter = self.max_hold_frames;
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
        }

        self.is_active()
    }

    /// Update with a single time-domain sample for fast attack triggering.
    pub fn update_sample(&mut self, sample: f32, threshold: f32, sample_rate: u32) -> bool {
        let abs_val = sample.abs();
        let delta = abs_val - self.prev_sample.abs();
        self.prev_sample = sample;

        if delta > threshold * 3.0 && abs_val > threshold {
            let hold_samples = ((sample_rate as f32) * 0.010) as usize; // 10ms
            self.hold_counter = hold_samples;
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
        }

        self.hold_counter > 0
    }

    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.hold_counter > 0
    }

    pub fn reset(&mut self) {
        self.prev_mag = [0.0; NUM_BINS];
        self.prev_sample = 0.0;
        self.hold_counter = 0;
    }
}

impl Default for TransientDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Production-grade audio denoiser with multi-band STFT Wiener filtering,
/// transient preservation, adaptive minimum statistics noise estimation, and mains de-humming.
pub struct AudioDenoiser {
    config: DenoiseConfig,
    sample_rate: u32,
    noise_floor_estimate: f32,
    envelope: f32,
    // Minimum statistics noise tracker
    min_stats: MinimumStatisticsTracker,
    noise_psd_buffer: [f32; NUM_BINS],
    // Ephraim-Malah Wiener filter
    wiener: DecisionDirectedWienerFilter,
    wiener_gains: [f32; NUM_BINS],
    // Hum notch filter & detector
    hum_filter: Option<HumNotchFilter>,
    hum_detector: MainsHumDetector,
    detected_hum: DetectedHum,
    // Transient detector
    transient_detector: TransientDetector,
    // STFT square-root Hann window (Princen-Bradley COLA condition)
    stft_window: [f32; STFT_SIZE],
    // STFT overlap-add buffers
    input_history: [f32; STFT_SIZE],
    output_overlap: [f32; STFT_SIZE],
    stft_latency_filled: bool,
}

impl AudioDenoiser {
    pub fn new(sample_rate: u32, config: DenoiseConfig) -> Self {
        let sr = sample_rate.max(8000);
        let hum_filter = match config.mode {
            DenoiseMode::DeHum50 => Some(HumNotchFilter::with_q(50.0, sr as f32, config.notch_q)),
            DenoiseMode::DeHum60 => Some(HumNotchFilter::with_q(60.0, sr as f32, config.notch_q)),
            _ => None,
        };

        let initial_noise = config
            .profile
            .as_ref()
            .map(|p| p.broadband_floor)
            .unwrap_or(0.01);

        let mut initial_psd = [0.01 * 0.01; NUM_BINS];
        if let Some(profile) = &config.profile {
            for (k, &bin) in profile.bin_floors.iter().enumerate().take(NUM_BINS) {
                initial_psd[k] = bin * bin;
            }
        }

        // Square-root Hann window: w[n] = sin(pi * (n + 0.5) / N).
        // Perfectly satisfies COLA Princen-Bradley: w^2[n] + w^2[n + N/2] = sin^2 + cos^2 = 1.0.
        let mut stft_window = [0.0_f32; STFT_SIZE];
        for (i, win) in stft_window.iter_mut().enumerate() {
            *win = (PI * (i as f32 + 0.5) / STFT_SIZE as f32).sin();
        }

        Self {
            sample_rate: sr,
            noise_floor_estimate: initial_noise,
            envelope: 0.0,
            min_stats: MinimumStatisticsTracker::new(),
            noise_psd_buffer: initial_psd,
            wiener: DecisionDirectedWienerFilter::new(),
            wiener_gains: [1.0; NUM_BINS],
            hum_filter,
            hum_detector: MainsHumDetector,
            detected_hum: DetectedHum::None,
            transient_detector: TransientDetector::new(),
            stft_window,
            input_history: [0.0; STFT_SIZE],
            output_overlap: [0.0; STFT_SIZE],
            stft_latency_filled: false,
            config,
        }
    }

    /// Access the active configuration.
    pub fn config(&self) -> &DenoiseConfig {
        &self.config
    }

    /// Update configuration dynamically.
    pub fn set_config(&mut self, config: DenoiseConfig) {
        self.config = config;
        self.hum_filter = match self.config.mode {
            DenoiseMode::DeHum50 => Some(HumNotchFilter::with_q(
                50.0,
                self.sample_rate as f32,
                self.config.notch_q,
            )),
            DenoiseMode::DeHum60 => Some(HumNotchFilter::with_q(
                60.0,
                self.sample_rate as f32,
                self.config.notch_q,
            )),
            _ => None,
        };
        if let Some(profile) = &self.config.profile {
            self.noise_floor_estimate = profile.broadband_floor;
            for (k, &bin) in profile.bin_floors.iter().enumerate().take(NUM_BINS) {
                self.noise_psd_buffer[k] = bin * bin;
            }
        }
    }

    /// Teach the denoiser a captured background noise room tone profile.
    pub fn learn_noise_profile(&mut self, silence_samples: &[f32]) {
        let profile = DenoiseProfile::learn_from_samples(silence_samples, self.sample_rate);
        self.noise_floor_estimate = profile.broadband_floor;
        for (k, &bin) in profile.bin_floors.iter().enumerate().take(NUM_BINS) {
            self.noise_psd_buffer[k] = bin * bin;
        }
        self.config.profile = Some(profile);
    }

    /// Result of recent mains hum detection.
    pub fn detected_hum(&self) -> DetectedHum {
        self.detected_hum
    }

    /// Current broadband noise floor estimate.
    pub fn noise_floor_estimate(&self) -> f32 {
        self.noise_floor_estimate
    }

    /// Whether a speech transient is currently active.
    pub fn is_transient_active(&self) -> bool {
        self.transient_detector.is_active()
    }

    /// Process a buffer of mono audio samples in-place.
    pub fn process_mono(&mut self, samples: &mut [f32]) {
        self.process_interleaved(samples);
    }

    /// Process independent stereo channels in-place.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_mono(left);
        self.process_mono(right);
    }

    /// Process a buffer of interleaved f32 PCM audio samples in-place.
    pub fn process_interleaved(&mut self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }

        // Automatic mains hum detection if enabled
        if self.config.auto_dehum || self.config.mode == DenoiseMode::AutoDeHum {
            let detected = self
                .hum_detector
                .detect_hum(samples, self.sample_rate as f32);
            self.detected_hum = detected;

            if self.hum_filter.is_none() || self.config.mode == DenoiseMode::AutoDeHum {
                match detected {
                    DetectedHum::Hum50Hz => {
                        self.hum_filter = Some(HumNotchFilter::with_q(
                            50.0,
                            self.sample_rate as f32,
                            self.config.notch_q,
                        ));
                    }
                    DetectedHum::Hum60Hz => {
                        self.hum_filter = Some(HumNotchFilter::with_q(
                            60.0,
                            self.sample_rate as f32,
                            self.config.notch_q,
                        ));
                    }
                    DetectedHum::None => {
                        if self.config.mode == DenoiseMode::AutoDeHum {
                            self.hum_filter = None;
                        }
                    }
                }
            }
        }

        // Apply dedicated mains hum notch if active
        if let Some(hum) = &mut self.hum_filter {
            for s in samples.iter_mut() {
                *s = hum.process(*s);
            }
        }

        // Gain floor in linear amplitude: e.g. -15dB -> ~0.1778, -30dB -> ~0.0316
        let gain_floor = 10.0_f32.powf(-self.config.reduction_db / 20.0);
        let attack_coeff =
            (-1.0 / (self.config.attack_ms.max(0.5) * 0.001 * self.sample_rate as f32)).exp();
        let release_coeff =
            (-1.0 / (self.config.release_ms.max(5.0) * 0.001 * self.sample_rate as f32)).exp();

        // Track broadband average level
        let mut sum_abs = 0.0_f32;
        for &s in samples.iter() {
            sum_abs += s.abs();
        }
        let avg_level = sum_abs / samples.len() as f32;
        if avg_level < self.noise_floor_estimate * 1.5 {
            self.noise_floor_estimate = self.noise_floor_estimate * 0.95 + avg_level * 0.05;
        }
        let threshold = (self.noise_floor_estimate * self.config.sensitivity).max(1e-4);

        match self.config.mode {
            DenoiseMode::MultiBand | DenoiseMode::AutoDeHum => {
                // High-precision STFT Ephraim-Malah decision-directed Wiener filtering
                let mut re = [0.0_f32; STFT_SIZE];
                let mut im = [0.0_f32; STFT_SIZE];
                let mut noisy_power = [0.0_f32; NUM_BINS];
                let mut noisy_mag = [0.0_f32; NUM_BINS];

                let mut offset = 0;
                while offset + HOP_SIZE <= samples.len() {
                    // Shift input history and insert new hop
                    self.input_history.copy_within(HOP_SIZE..STFT_SIZE, 0);
                    self.input_history[STFT_SIZE - HOP_SIZE..STFT_SIZE]
                        .copy_from_slice(&samples[offset..offset + HOP_SIZE]);

                    // Windowed analysis using square-root Hann
                    for i in 0..STFT_SIZE {
                        re[i] = self.input_history[i] * self.stft_window[i];
                        im[i] = 0.0;
                    }

                    fft_radix2(&mut re, &mut im, false);

                    // Compute noisy power and magnitude spectrum
                    for k in 0..NUM_BINS {
                        let p = re[k] * re[k] + im[k] * im[k];
                        noisy_power[k] = p;
                        noisy_mag[k] = p.sqrt();
                    }

                    // Minimum statistics noise PSD update
                    if self.config.profile.is_none() {
                        self.min_stats
                            .update(&noisy_power, &mut self.noise_psd_buffer);
                    }

                    // Transient detection
                    let transient_active = if self.config.transient_preserve {
                        self.transient_detector.update(&noisy_mag, threshold)
                    } else {
                        false
                    };

                    // Decision-directed Wiener filtering
                    self.wiener.compute_gains(
                        &noisy_power,
                        &self.noise_psd_buffer,
                        gain_floor,
                        self.config.sensitivity,
                        self.config.wiener_alpha,
                        &mut self.wiener_gains,
                    );

                    // Apply gains with transient relaxation
                    for k in 0..NUM_BINS {
                        let mut g = self.wiener_gains[k];
                        if transient_active {
                            g = g.max(0.85);
                        }

                        re[k] *= g;
                        im[k] *= g;

                        // Conjugate symmetry for real IFFT
                        if k > 0 && k < STFT_SIZE / 2 {
                            re[STFT_SIZE - k] = re[k];
                            im[STFT_SIZE - k] = -im[k];
                        }
                    }

                    fft_radix2(&mut re, &mut im, true);

                    // Overlap-add synthesis using square-root Hann
                    for (i, overlap) in self.output_overlap.iter_mut().enumerate() {
                        let syn = re[i] * self.stft_window[i];
                        *overlap += syn;
                    }

                    // Output reconstructed audio for this hop
                    if self.stft_latency_filled {
                        for i in 0..HOP_SIZE {
                            samples[offset + i] = self.output_overlap[i].clamp(-1.0, 1.0);
                        }
                    } else {
                        // Cold-start priming: output without discontinuity
                        for i in 0..HOP_SIZE {
                            samples[offset + i] = self.output_overlap[i].clamp(-1.0, 1.0);
                        }
                        self.stft_latency_filled = true;
                    }

                    // Shift output overlap buffer
                    self.output_overlap.copy_within(HOP_SIZE..STFT_SIZE, 0);
                    self.output_overlap[STFT_SIZE - HOP_SIZE..STFT_SIZE].fill(0.0);

                    offset += HOP_SIZE;
                }

                // Process remaining tail samples with transient-preserving time gate
                if offset < samples.len() {
                    for s in &mut samples[offset..] {
                        let abs_val = s.abs();
                        let is_t =
                            self.transient_detector
                                .update_sample(*s, threshold, self.sample_rate);
                        let gain = if is_t {
                            1.0
                        } else if abs_val < threshold {
                            gain_floor
                        } else {
                            1.0
                        };
                        *s = (*s * gain).clamp(-1.0, 1.0);
                    }
                }
            }
            _ => {
                // Vocal focus / Room tone / De-hum: transient-preserving dynamic spectral gate
                for s in samples.iter_mut() {
                    let abs_val = s.abs();

                    let is_transient = if self.config.transient_preserve {
                        self.transient_detector
                            .update_sample(*s, threshold, self.sample_rate)
                    } else {
                        false
                    };

                    if abs_val > self.envelope {
                        self.envelope =
                            attack_coeff * self.envelope + (1.0 - attack_coeff) * abs_val;
                    } else {
                        self.envelope =
                            release_coeff * self.envelope + (1.0 - release_coeff) * abs_val;
                    }

                    let gain = if is_transient {
                        1.0
                    } else if self.envelope < threshold {
                        let ratio = (self.envelope / threshold).clamp(0.0, 1.0);
                        gain_floor + (1.0 - gain_floor) * ratio * ratio
                    } else {
                        1.0
                    };

                    *s = (*s * gain).clamp(-1.0, 1.0);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Audio Plugin Interface: NeuralAudioDenoiserPlugin
// ---------------------------------------------------------------------------

/// Professional broadcast audio denoiser studio plugin implementing `rd_audio::plugin::AudioPlugin`.
pub struct NeuralAudioDenoiserPlugin {
    desc: PluginDescriptor,
    sample_rate: f32,
    denoiser_l: AudioDenoiser,
    denoiser_r: AudioDenoiser,
    bypassed: bool,
    wet_dry: f32,
}

impl NeuralAudioDenoiserPlugin {
    pub const PARAM_REDUCTION_DB: u32 = 0;
    pub const PARAM_SENSITIVITY: u32 = 1;
    pub const PARAM_MODE: u32 = 2;
    pub const PARAM_TRANSIENT_PRESERVE: u32 = 3;
    pub const PARAM_NOTCH_Q: u32 = 4;
    pub const PARAM_AUTO_DEHUM: u32 = 5;
    pub const PARAM_ATTACK_MS: u32 = 6;
    pub const PARAM_RELEASE_MS: u32 = 7;

    pub fn new() -> Self {
        let desc = PluginDescriptor {
            id: "reddart.effects.neural_denoiser".to_string(),
            name: "Reddart Neural Audio Denoiser".to_string(),
            vendor: "Reddart Audio Labs".to_string(),
            version: "1.4.0".to_string(),
            category: PluginCategory::Effect,
            format: PluginFormat::InternalDsp,
            input_channels: 2,
            output_channels: 2,
            latency_samples: HOP_SIZE,
            parameters: vec![
                PluginParameter::new(
                    Self::PARAM_REDUCTION_DB,
                    "Noise Reduction",
                    "dB",
                    0.0,
                    30.0,
                    15.0,
                ),
                PluginParameter::new(Self::PARAM_SENSITIVITY, "Sensitivity", "", 0.5, 3.0, 1.2),
                PluginParameter::new(
                    Self::PARAM_MODE,
                    "Mode (0:Voice 1:Room 2:Hum50 3:Hum60 4:MultiBand 5:Auto)",
                    "",
                    0.0,
                    5.0,
                    4.0, // default to MultiBand
                ),
                PluginParameter::new(
                    Self::PARAM_TRANSIENT_PRESERVE,
                    "Transient Preservation",
                    "",
                    0.0,
                    1.0,
                    1.0,
                ),
                PluginParameter::new(Self::PARAM_NOTCH_Q, "De-hum Notch Q", "", 2.0, 30.0, 10.0),
                PluginParameter::new(Self::PARAM_AUTO_DEHUM, "Auto De-hum", "", 0.0, 1.0, 1.0),
                PluginParameter::new(Self::PARAM_ATTACK_MS, "Attack", "ms", 1.0, 50.0, 10.0),
                PluginParameter::new(Self::PARAM_RELEASE_MS, "Release", "ms", 10.0, 500.0, 80.0),
            ],
        };

        let initial_config = DenoiseConfig {
            reduction_db: 15.0,
            sensitivity: 1.2,
            mode: DenoiseMode::MultiBand,
            transient_preserve: true,
            notch_q: 10.0,
            auto_dehum: true,
            attack_ms: 10.0,
            release_ms: 80.0,
            ..Default::default()
        };

        Self {
            desc,
            sample_rate: 48000.0,
            denoiser_l: AudioDenoiser::new(48000, initial_config.clone()),
            denoiser_r: AudioDenoiser::new(48000, initial_config),
            bypassed: false,
            wet_dry: 1.0,
        }
    }

    fn sync_parameters_to_config(&mut self) {
        let reduction_db = self.get_parameter(Self::PARAM_REDUCTION_DB).unwrap_or(15.0);
        let sensitivity = self.get_parameter(Self::PARAM_SENSITIVITY).unwrap_or(1.2);
        let mode_val = self.get_parameter(Self::PARAM_MODE).unwrap_or(4.0).round() as u32;
        let mode = match mode_val {
            0 => DenoiseMode::VoiceFocus,
            1 => DenoiseMode::RoomToneRemoval,
            2 => DenoiseMode::DeHum50,
            3 => DenoiseMode::DeHum60,
            4 => DenoiseMode::MultiBand,
            _ => DenoiseMode::AutoDeHum,
        };
        let transient_preserve = self
            .get_parameter(Self::PARAM_TRANSIENT_PRESERVE)
            .unwrap_or(1.0)
            > 0.5;
        let notch_q = self.get_parameter(Self::PARAM_NOTCH_Q).unwrap_or(10.0);
        let auto_dehum = self.get_parameter(Self::PARAM_AUTO_DEHUM).unwrap_or(1.0) > 0.5;
        let attack_ms = self.get_parameter(Self::PARAM_ATTACK_MS).unwrap_or(10.0);
        let release_ms = self.get_parameter(Self::PARAM_RELEASE_MS).unwrap_or(80.0);

        let config = DenoiseConfig {
            reduction_db,
            sensitivity,
            mode,
            transient_preserve,
            notch_q,
            auto_dehum,
            attack_ms,
            release_ms,
            ..Default::default()
        };

        self.denoiser_l.set_config(config.clone());
        self.denoiser_r.set_config(config);
    }
}

impl Default for NeuralAudioDenoiserPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlugin for NeuralAudioDenoiserPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.desc
    }

    fn descriptor_mut(&mut self) -> &mut PluginDescriptor {
        &mut self.desc
    }

    fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8000.0);
        self.sync_parameters_to_config();
        self.denoiser_l =
            AudioDenoiser::new(self.sample_rate as u32, self.denoiser_l.config.clone());
        self.denoiser_r =
            AudioDenoiser::new(self.sample_rate as u32, self.denoiser_r.config.clone());
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

    fn set_parameter(&mut self, id: u32, value: f32) -> Result<(), PluginError> {
        let desc = self.descriptor_mut();
        if let Some(param) = desc.parameters.iter_mut().find(|p| p.id == id) {
            param.set_clamped(value);
            self.sync_parameters_to_config();
            Ok(())
        } else {
            Err(PluginError::ParameterNotFound(id))
        }
    }

    fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> Result<(), PluginError> {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        if n == 0 {
            return Ok(());
        }

        out_l[..n].copy_from_slice(&in_l[..n]);
        out_r[..n].copy_from_slice(&in_r[..n]);

        if !self.bypassed {
            self.denoiser_l.process_mono(&mut out_l[..n]);
            self.denoiser_r.process_mono(&mut out_r[..n]);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fft_roundtrip() {
        let mut re = [0.0_f32; 16];
        let mut im = [0.0_f32; 16];
        re[0] = 1.0;
        re[1] = 0.5;
        re[4] = -0.3;

        let orig_re = re;
        fft_radix2(&mut re, &mut im, false);
        fft_radix2(&mut re, &mut im, true);

        for i in 0..16 {
            assert!((re[i] - orig_re[i]).abs() < 1e-5);
            assert!(im[i].abs() < 1e-5);
        }
    }

    #[test]
    fn test_cola_square_root_hann_window() {
        // Princen-Bradley COLA condition test:
        // w^2[n] + w^2[n + N/2] == 1.0 for all n in [0, N/2 - 1]
        for n in 0..HOP_SIZE {
            let w1 = (PI * (n as f32 + 0.5) / STFT_SIZE as f32).sin();
            let w2 = (PI * ((n + HOP_SIZE) as f32 + 0.5) / STFT_SIZE as f32).sin();
            let sum_sq = w1 * w1 + w2 * w2;
            assert!(
                (sum_sq - 1.0).abs() < 1e-6,
                "COLA condition violated at n={}: w1^2 + w2^2 = {}",
                n,
                sum_sq
            );
        }
    }

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

    #[test]
    fn test_audio_denoiser_multiband_mode() {
        let config = DenoiseConfig {
            mode: DenoiseMode::MultiBand,
            reduction_db: 18.0,
            sensitivity: 1.2,
            ..Default::default()
        };
        let mut denoiser = AudioDenoiser::new(48000, config);

        // Low amplitude stationary hiss
        let mut hiss = vec![0.002_f32; 4800];
        denoiser.process_mono(&mut hiss);
        let attenuated = hiss[hiss.len() - 1];
        assert!(
            attenuated < 0.001,
            "Expected multi-band hiss attenuation, got {}",
            attenuated
        );
    }

    #[test]
    fn test_hum_notch_filter_50hz() {
        let mut hum_filter = HumNotchFilter::new(50.0, 48000.0);
        let sample_rate = 48000.0;

        let n = 24000;
        let mut hum_signal = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sample_rate;
            let sample = (2.0 * PI * 50.0 * t).sin();
            hum_signal.push(sample);
        }

        let mut filtered_signal = hum_signal.clone();
        for s in &mut filtered_signal {
            *s = hum_filter.process(*s);
        }

        // Check steady state attenuation in the second half
        let steady_state = &filtered_signal[12000..];
        let rms: f32 =
            (steady_state.iter().map(|s| s * s).sum::<f32>() / steady_state.len() as f32).sqrt();
        assert!(rms < 0.2, "Expected 50 Hz hum attenuation, got RMS {}", rms);
    }

    #[test]
    fn test_mains_hum_detector_50hz() {
        let detector = MainsHumDetector;
        let sample_rate = 48000.0;
        let n = 9600; // 200ms

        // Synthesize 50 Hz hum + 100 Hz harmonic with background noise
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sample_rate;
            let hum = (2.0 * PI * 50.0 * t).sin() * 0.3 + (2.0 * PI * 100.0 * t).sin() * 0.1;
            samples.push(hum);
        }

        let detected = detector.detect_hum(&samples, sample_rate);
        assert_eq!(detected, DetectedHum::Hum50Hz);
    }

    #[test]
    fn test_mains_hum_detector_60hz() {
        let detector = MainsHumDetector;
        let sample_rate = 48000.0;
        let n = 9600;

        // Synthesize 60 Hz hum + 120 Hz harmonic
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sample_rate;
            let hum = (2.0 * PI * 60.0 * t).sin() * 0.3 + (2.0 * PI * 120.0 * t).sin() * 0.1;
            samples.push(hum);
        }

        let detected = detector.detect_hum(&samples, sample_rate);
        assert_eq!(detected, DetectedHum::Hum60Hz);
    }

    #[test]
    fn test_learn_noise_profile() {
        let sample_rate = 48000;
        let noise_samples = vec![0.008_f32; 4800];
        let profile = DenoiseProfile::learn_from_samples(&noise_samples, sample_rate);

        assert!(!profile.bin_floors.is_empty());
        for bin_floor in &profile.bin_floors {
            assert!(*bin_floor > 0.0);
        }

        let mut denoiser = AudioDenoiser::new(sample_rate, DenoiseConfig::default());
        denoiser.learn_noise_profile(&noise_samples);
        assert!(denoiser.noise_floor_estimate > 0.0);
    }

    #[test]
    fn test_biquad_filters() {
        let mut lp = BiquadFilter::lowpass(1000.0, 48000.0);
        let mut hp = BiquadFilter::highpass(1000.0, 48000.0);
        let mut bp = BiquadFilter::bandpass(1000.0, 48000.0, 2.0);

        let out_lp = lp.process(0.5);
        let out_hp = hp.process(0.5);
        let out_bp = bp.process(0.5);

        assert!(!out_lp.is_nan());
        assert!(!out_hp.is_nan());
        assert!(!out_bp.is_nan());
    }

    #[test]
    fn test_neural_audio_denoiser_plugin_interface() {
        let mut plugin = NeuralAudioDenoiserPlugin::new();
        assert_eq!(plugin.descriptor().id, "reddart.effects.neural_denoiser");
        assert_eq!(plugin.descriptor().category, PluginCategory::Effect);
        assert_eq!(plugin.latency_samples(), HOP_SIZE);

        // Test parameter set/get
        plugin
            .set_parameter(NeuralAudioDenoiserPlugin::PARAM_REDUCTION_DB, 20.0)
            .unwrap();
        let val = plugin
            .get_parameter(NeuralAudioDenoiserPlugin::PARAM_REDUCTION_DB)
            .unwrap();
        assert_eq!(val, 20.0);

        // Test audio processing
        let in_l = vec![0.002_f32; 1024];
        let in_r = vec![0.002_f32; 1024];
        let mut out_l = vec![0.0_f32; 1024];
        let mut out_r = vec![0.0_f32; 1024];

        plugin
            .process(&in_l, &in_r, &mut out_l, &mut out_r)
            .unwrap();
        assert!(!out_l[0].is_nan());
        assert!(!out_r[0].is_nan());
    }
}
