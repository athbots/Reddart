//! Fast Fourier Transform (FFT) and Real-Time Spectral Analyzer (RTA).
//!
//! Provides a pure-Rust, allocation-free Radix-2 Cooley-Tukey FFT implementation,
//! standard windowing functions (Hann, Hamming, Blackman-Harris), 31-band 1/3-octave
//! ISO frequency binning, logarithmic frequency curve interpolation, and broadcast
//! ballistics (instantaneous peak rise with exponential decay).

use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Standard 31-band 1/3-octave ISO center frequencies in Hertz (20 Hz to 20 kHz).
pub const ISO_31_FREQUENCIES: [f32; 31] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];

/// Window function types for spectral analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowType {
    /// Hann window: good general-purpose frequency resolution and moderate sidelobe suppression.
    #[default]
    Hann,
    /// Hamming window: optimized for first sidelobe cancellation.
    Hamming,
    /// Blackman-Harris 4-term window: exceptional sidelobe suppression (>92 dB) for mastering.
    BlackmanHarris,
    /// Rectangular window (no windowing).
    Rectangular,
}

/// Compute window function sample at index `n` for window size `N`.
#[inline]
pub fn window_sample(n: usize, n_total: usize, window: WindowType) -> f32 {
    if n_total <= 1 {
        return 1.0;
    }
    let n_f = n as f32;
    let n_tot_f = (n_total - 1) as f32;

    match window {
        WindowType::Rectangular => 1.0,
        WindowType::Hann => 0.5 * (1.0 - (2.0 * PI * n_f / n_tot_f).cos()),
        WindowType::Hamming => 0.54 - 0.46 * (2.0 * PI * n_f / n_tot_f).cos(),
        WindowType::BlackmanHarris => {
            const A0: f32 = 0.35875;
            const A1: f32 = 0.48829;
            const A2: f32 = 0.14128;
            const A3: f32 = 0.01168;
            let phi = 2.0 * PI * n_f / n_tot_f;
            A0 - A1 * phi.cos() + A2 * (2.0 * phi).cos() - A3 * (3.0 * phi).cos()
        }
    }
}

/// Precomputed Radix-2 Cooley-Tukey FFT Processor.
pub struct FftPlan {
    size: usize,
    log2_size: usize,
    bit_rev: Vec<usize>,
    twiddles_re: Vec<f32>,
    twiddles_im: Vec<f32>,
    window: Vec<f32>,
}

impl FftPlan {
    /// Create a new FFT plan for the specified size (must be power of two, e.g. 512, 1024, 2048).
    pub fn new(size: usize, window_type: WindowType) -> Self {
        assert!(size.is_power_of_two(), "FFT size must be a power of two");
        let log2_size = size.trailing_zeros() as usize;

        // Bit reversal table
        let mut bit_rev = vec![0; size];
        for (i, entry) in bit_rev.iter_mut().enumerate() {
            let mut rev = 0;
            for j in 0..log2_size {
                if (i & (1 << j)) != 0 {
                    rev |= 1 << (log2_size - 1 - j);
                }
            }
            *entry = rev;
        }

        // Twiddle factors: W_N^k = e^(-2pi*i*k / N)
        let half = size / 2;
        let mut twiddles_re = Vec::with_capacity(half);
        let mut twiddles_im = Vec::with_capacity(half);
        for k in 0..half {
            let angle = -2.0 * PI * (k as f32) / (size as f32);
            twiddles_re.push(angle.cos());
            twiddles_im.push(angle.sin());
        }

        // Window table
        let mut window = Vec::with_capacity(size);
        for n in 0..size {
            window.push(window_sample(n, size, window_type));
        }

        Self {
            size,
            log2_size,
            bit_rev,
            twiddles_re,
            twiddles_im,
            window,
        }
    }

    /// Size of the FFT plan.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Execute forward in-place real-to-complex FFT.
    ///
    /// Input `signal` must have length at least `self.size`.
    /// Outputs `out_re` and `out_im` must have length at least `self.size`.
    pub fn forward(&self, signal: &[f32], out_re: &mut [f32], out_im: &mut [f32]) {
        assert!(signal.len() >= self.size);
        assert!(out_re.len() >= self.size);
        assert!(out_im.len() >= self.size);

        // Apply window and bit-reversal permute
        for (i, &sig) in signal.iter().enumerate().take(self.size) {
            let rev = self.bit_rev[i];
            out_re[rev] = sig * self.window[i];
            out_im[rev] = 0.0;
        }

        // Cooley-Tukey Radix-2 DIT Butterflies
        for s in 1..=self.log2_size {
            let m = 1 << s;
            let half_m = m >> 1;
            let step = self.size >> s;

            let mut k = 0;
            while k < self.size {
                for j in 0..half_m {
                    let tw_idx = j * step;
                    let u_re = out_re[k + j];
                    let u_im = out_im[k + j];

                    let v_re = out_re[k + j + half_m];
                    let v_im = out_im[k + j + half_m];

                    let w_re = self.twiddles_re[tw_idx];
                    let w_im = self.twiddles_im[tw_idx];

                    // Complex multiply: t = v * w
                    let t_re = v_re * w_re - v_im * w_im;
                    let t_im = v_re * w_im + v_im * w_re;

                    out_re[k + j] = u_re + t_re;
                    out_im[k + j] = u_im + t_im;

                    out_re[k + j + half_m] = u_re - t_re;
                    out_im[k + j + half_m] = u_im - t_im;
                }
                k += m;
            }
        }
    }

    /// Compute power spectral density magnitude (in dBFS) for positive frequencies ($0 \dots N/2$).
    ///
    /// Outputs `magnitudes_dbfs` of length `self.size / 2 + 1`.
    pub fn compute_magnitudes_dbfs(
        &self,
        signal: &[f32],
        out_re: &mut [f32],
        out_im: &mut [f32],
        magnitudes_dbfs: &mut [f32],
    ) {
        let num_bins = self.size / 2 + 1;
        assert!(magnitudes_dbfs.len() >= num_bins);

        self.forward(signal, out_re, out_im);

        // Normalize factor: 2.0 / N (account for one-sided positive frequency spectrum folding)
        let norm = 2.0 / (self.size as f32);

        for k in 0..num_bins {
            let re = out_re[k] * norm;
            let im = out_im[k] * norm;
            let mag = (re * re + im * im).sqrt();
            let dbfs = if mag < 0.000_001 {
                -120.0
            } else {
                (20.0 * mag.log10()).clamp(-120.0, 12.0)
            };
            magnitudes_dbfs[k] = dbfs;
        }
    }
}

/// Circular audio sample buffer for lock-free ingestion from real-time audio threads.
pub struct AudioSampleRing {
    buffer: Vec<f32>,
    capacity: usize,
    write_idx: AtomicUsize,
}

impl AudioSampleRing {
    /// Create new ring buffer with specified sample capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            capacity,
            write_idx: AtomicUsize::new(0),
        }
    }

    /// Ingest interleaved multi-channel or mono audio, summing channels to mono into the ring buffer.
    pub fn push_interleaved(&mut self, samples: &[f32], channels: usize) {
        if channels == 0 || samples.is_empty() {
            return;
        }
        let inv_ch = 1.0 / (channels as f32);
        let mut curr = self.write_idx.load(Ordering::Relaxed);

        for frame in samples.chunks_exact(channels) {
            let mono: f32 = frame.iter().sum::<f32>() * inv_ch;
            self.buffer[curr] = mono;
            curr = (curr + 1) % self.capacity;
        }

        self.write_idx.store(curr, Ordering::Release);
    }

    /// Read the latest `count` samples in continuous chronological order.
    pub fn read_latest(&self, out: &mut [f32]) {
        let count = out.len().min(self.capacity);
        let curr = self.write_idx.load(Ordering::Acquire);

        // Start index `count` samples prior to current write cursor
        let start = if curr >= count {
            curr - count
        } else {
            self.capacity + curr - count
        };

        for (i, out_sample) in out.iter_mut().enumerate().take(count) {
            let idx = (start + i) % self.capacity;
            *out_sample = self.buffer[idx];
        }
    }
}

/// Real-Time Spectrum Analyzer (RTA) with 31-band ISO frequency aggregation,
/// smooth log-frequency curve interpolation, and peak ballistics.
pub struct SpectrumAnalyzer {
    plan: FftPlan,
    sample_rate: f32,
    re_scratch: Vec<f32>,
    im_scratch: Vec<f32>,
    raw_bins_dbfs: Vec<f32>,
    bands_current: [f32; 31],
    bands_peak_hold: [f32; 31],
    curve_points: usize,
    curve_cache: Vec<f32>,
    falloff_db_per_sec: f32,
}

impl SpectrumAnalyzer {
    /// Create new spectrum analyzer with given FFT size (e.g. 1024 or 2048) and sample rate (48000).
    pub fn new(fft_size: usize, sample_rate: f32, curve_points: usize) -> Self {
        let plan = FftPlan::new(fft_size, WindowType::Hann);
        let bins = fft_size / 2 + 1;

        Self {
            plan,
            sample_rate,
            re_scratch: vec![0.0; fft_size],
            im_scratch: vec![0.0; fft_size],
            raw_bins_dbfs: vec![-120.0; bins],
            bands_current: [-120.0; 31],
            bands_peak_hold: [-120.0; 31],
            curve_points: curve_points.max(32),
            curve_cache: vec![-120.0; curve_points.max(32)],
            falloff_db_per_sec: 30.0, // 30 dB/sec decay
        }
    }

    /// Process a block of time-domain audio samples and update the spectral analysis with delta time `dt_s`.
    pub fn process_samples(&mut self, samples: &[f32], dt_s: f32) {
        let fft_size = self.plan.size();
        if samples.len() < fft_size {
            return;
        }

        // 1. Compute raw FFT magnitude spectrum
        self.plan.compute_magnitudes_dbfs(
            &samples[samples.len() - fft_size..],
            &mut self.re_scratch,
            &mut self.im_scratch,
            &mut self.raw_bins_dbfs,
        );

        let bin_width = self.sample_rate / (fft_size as f32);
        let decay = (self.falloff_db_per_sec * dt_s).max(0.0);

        // 2. Map raw FFT bins into standard 31-band 1/3-octave ISO frequencies
        // 1/3-octave lower edge: f_lower = f_c * 2^(-1/6) ~= f_c * 0.8908987
        // 1/3-octave upper edge: f_upper = f_c * 2^(+1/6) ~= f_c * 1.1224620
        const FACTOR_LOWER: f32 = 0.890_898_7;
        const FACTOR_UPPER: f32 = 1.122_462;

        for (band_idx, &f_c) in ISO_31_FREQUENCIES.iter().enumerate() {
            let f_low = (f_c * FACTOR_LOWER).max(0.0);
            let f_high = (f_c * FACTOR_UPPER).min(self.sample_rate * 0.5);

            let k_low = (f_low / bin_width).floor() as usize;
            let k_high = (f_high / bin_width).ceil() as usize;

            let k_start = k_low.clamp(0, self.raw_bins_dbfs.len() - 1);
            let k_end = k_high.clamp(k_start, self.raw_bins_dbfs.len() - 1);

            // Maximum peak or energy average in the band
            let mut band_max_db = -120.0_f32;
            for k in k_start..=k_end {
                let db = self.raw_bins_dbfs[k];
                if db > band_max_db {
                    band_max_db = db;
                }
            }

            // Apply ballistics: instantaneous rise, exponential decay
            let prev = self.bands_current[band_idx];
            let new_val = if band_max_db > prev {
                band_max_db
            } else {
                (prev - decay).max(-120.0)
            };
            self.bands_current[band_idx] = new_val;

            // Peak hold ballistics
            let peak_prev = self.bands_peak_hold[band_idx];
            if new_val > peak_prev {
                self.bands_peak_hold[band_idx] = new_val;
            } else {
                self.bands_peak_hold[band_idx] = (peak_prev - decay * 0.3).max(-120.0);
            }
        }

        // 3. Interpolate smooth continuous logarithmic frequency curve
        // Frequency range: 20 Hz to 20,000 Hz logarithmically mapped across curve_points
        let f_min = 20.0_f32;
        let f_max = 20_000.0_f32;
        let log_ratio = (f_max / f_min).ln();

        for (i, val) in self.curve_cache.iter_mut().enumerate() {
            let frac = i as f32 / ((self.curve_points - 1) as f32);
            let f = f_min * (frac * log_ratio).exp();
            let k_f = f / bin_width;
            let k0 = (k_f.floor() as usize).min(self.raw_bins_dbfs.len() - 1);
            let k1 = (k0 + 1).min(self.raw_bins_dbfs.len() - 1);
            let alpha = k_f - (k0 as f32);

            let db0 = self.raw_bins_dbfs[k0];
            let db1 = self.raw_bins_dbfs[k1];
            let raw_db = db0 * (1.0 - alpha) + db1 * alpha;

            let prev = *val;
            *val = if raw_db > prev {
                raw_db
            } else {
                (prev - decay).max(-120.0)
            };
        }
    }

    /// Retrieve the current 31-band decibel levels (dBFS).
    pub fn bands(&self) -> &[f32; 31] {
        &self.bands_current
    }

    /// Retrieve the peak hold 31-band decibel levels (dBFS).
    pub fn bands_peak_hold(&self) -> &[f32; 31] {
        &self.bands_peak_hold
    }

    /// Retrieve the smooth continuous logarithmic frequency response curve (dBFS).
    pub fn curve(&self) -> &[f32] {
        &self.curve_cache
    }

    /// Convert logarithmic frequency fraction $x \in [0.0, 1.0]$ to Hertz ($20 \dots 20000$).
    pub fn fraction_to_freq(fraction: f32) -> f32 {
        const F_MIN: f32 = 20.0;
        const F_MAX: f32 = 20_000.0;
        let ratio = F_MAX / F_MIN;
        F_MIN * ratio.powf(fraction.clamp(0.0, 1.0))
    }

    /// Convert frequency in Hertz ($20 \dots 20000$) to logarithmic normalized fraction $x \in [0.0, 1.0]$.
    pub fn freq_to_fraction(freq_hz: f32) -> f32 {
        const F_MIN: f32 = 20.0;
        const F_MAX: f32 = 20_000.0;
        let f = freq_hz.clamp(F_MIN, F_MAX);
        (f / F_MIN).ln() / (F_MAX / F_MIN).ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fft_plan_orthogonality_and_sine_peak() {
        let size = 1024;
        let sample_rate = 48000.0;
        let plan = FftPlan::new(size, WindowType::Hann);

        // Generate pure sine wave at 1500 Hz: f_k = 1500 / (48000/1024) = 32 bins
        let target_freq = 1500.0;
        let mut signal = vec![0.0; size];
        for (i, s) in signal.iter_mut().enumerate() {
            *s = (2.0 * PI * target_freq * (i as f32) / sample_rate).sin();
        }

        let mut out_re = vec![0.0; size];
        let mut out_im = vec![0.0; size];
        let mut dbfs = vec![0.0; size / 2 + 1];

        plan.compute_magnitudes_dbfs(&signal, &mut out_re, &mut out_im, &mut dbfs);

        // Find peak frequency bin
        let mut max_bin = 0;
        let mut max_val = -200.0;
        for (k, &val) in dbfs.iter().enumerate() {
            if val > max_val {
                max_val = val;
                max_bin = k;
            }
        }

        let detected_freq = (max_bin as f32) * (sample_rate / (size as f32));
        assert!(
            (detected_freq - target_freq).abs() < 50.0,
            "Detected frequency {} Hz deviates from expected {} Hz",
            detected_freq,
            target_freq
        );
        assert!(
            max_val > -10.0,
            "Peak dBFS {} should be near 0 dBFS for full-scale sine",
            max_val
        );
    }

    #[test]
    fn test_frequency_fraction_roundtrip() {
        for &freq in &[20.0, 100.0, 1000.0, 5000.0, 20000.0] {
            let frac = SpectrumAnalyzer::freq_to_fraction(freq);
            assert!((0.0..=1.0).contains(&frac));
            let reconstructed = SpectrumAnalyzer::fraction_to_freq(frac);
            assert!(
                (reconstructed - freq).abs() < 0.05 * freq,
                "Reconstructed frequency {} deviates from {}",
                reconstructed,
                freq
            );
        }
    }

    #[test]
    fn test_spectrum_analyzer_iso_bands_response() {
        let mut analyzer = SpectrumAnalyzer::new(1024, 48000.0, 128);

        // Generate 1 kHz tone (band index 17 in ISO 31 bands: 1000 Hz)
        let mut samples = vec![0.0; 1024];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (2.0 * PI * 1000.0 * (i as f32) / 48000.0).sin();
        }

        analyzer.process_samples(&samples, 0.02);

        let bands = analyzer.bands();
        let band_1k = bands[17]; // 1000 Hz
        let band_63 = bands[5]; // 63 Hz
        let band_16k = bands[29]; // 16 kHz

        assert!(
            band_1k > -12.0,
            "1 kHz band level ({:.1} dB) should be prominent",
            band_1k
        );
        assert!(
            band_1k > band_63 + 30.0,
            "1 kHz band ({:.1} dB) should exceed 63 Hz band ({:.1} dB)",
            band_1k,
            band_63
        );
        assert!(
            band_1k > band_16k + 30.0,
            "1 kHz band ({:.1} dB) should exceed 16 kHz band ({:.1} dB)",
            band_1k,
            band_16k
        );
    }
}
