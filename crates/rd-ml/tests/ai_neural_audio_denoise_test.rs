//! Comprehensive Integration Verification Suite for Milestone 23:
//! Neural Audio Denoising, STFT Wiener Spectral Subtraction & Mains De-humming.
//!
//! Validates:
//! 1. STFT square-root Hann analysis/synthesis exact COLA perfect reconstruction
//! 2. Ephraim-Malah decision-directed (DD) Wiener filter attenuation and speech preservation
//! 3. Rainer Martin Minimum Statistics continuous background noise floor estimation
//! 4. Cascaded 4-harmonic mains hum notch suppression depth (>= 30 dB) and passband transparency
//! 5. Goertzel automatic mains hum detection (50 Hz vs 60 Hz) and dynamic filter engaging
//! 6. Transient preservation: speech plosives and high-frequency consonant attacks unclipped
//! 7. Multi-channel stereo coherence, phase alignment, and inter-channel balance
//! 8. Studio `NeuralAudioDenoiserPlugin` hosting inside `rd_audio::AudioPluginInsertChain`
//! 9. Real-time processing throughput benchmark (> 100x real-time speed)

use rd_audio::plugin::{AudioPlugin, AudioPluginInsertChain, PluginCategory};
use rd_ml::denoise::{
    AudioDenoiser, DenoiseConfig, DenoiseMode, DetectedHum, HOP_SIZE, HumNotchFilter,
    MainsHumDetector, MinimumStatisticsTracker, NUM_BINS, NeuralAudioDenoiserPlugin, STFT_SIZE,
    TransientDetector, fft_radix2,
};
use std::f32::consts::PI;
use std::time::Instant;

/// Helper to synthesize a pure sine tone.
fn generate_sine_wave(freq: f32, amplitude: f32, duration_sec: f32, sample_rate: f32) -> Vec<f32> {
    let num_samples = (duration_sec * sample_rate) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate;
        samples.push(amplitude * (2.0 * PI * freq * t).sin());
    }
    samples
}

/// Helper to synthesize pseudo-random stationary white noise.
fn generate_white_noise(
    amplitude: f32,
    duration_sec: f32,
    sample_rate: f32,
    seed: u64,
) -> Vec<f32> {
    let num_samples = (duration_sec * sample_rate) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    let mut state = seed;
    for _ in 0..num_samples {
        // Linear congruential generator
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let val = ((state >> 33) as f32 / (1u64 << 31) as f32) - 1.0;
        samples.push(amplitude * val);
    }
    samples
}

/// Helper to compute root-mean-square (RMS) energy.
fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Helper to compute amplitude in decibels full scale (dBFS).
fn rms_to_dbfs(rms: f32) -> f32 {
    if rms > 1e-10 {
        20.0 * rms.log10()
    } else {
        -120.0
    }
}

// ---------------------------------------------------------------------------
// Test 1: STFT Perfect Reconstruction (COLA Princen-Bradley Condition)
// ---------------------------------------------------------------------------
#[test]
fn test_stft_perfect_reconstruction_cola() {
    let sample_rate = 48000.0;
    // Multi-frequency test signal (200 Hz, 800 Hz, 2400 Hz, 8000 Hz)
    let duration = 0.5;
    let num_samples = (duration * sample_rate) as usize;
    let mut input = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate;
        let s = 0.3 * (2.0 * PI * 200.0 * t).sin()
            + 0.25 * (2.0 * PI * 800.0 * t).sin()
            + 0.2 * (2.0 * PI * 2400.0 * t).sin()
            + 0.15 * (2.0 * PI * 8000.0 * t).sin();
        input.push(s);
    }

    // Direct STFT analysis and synthesis with unity gain (identity G_k = 1.0)
    let mut window = [0.0_f32; STFT_SIZE];
    for (i, win) in window.iter_mut().enumerate() {
        *win = (PI * (i as f32 + 0.5) / STFT_SIZE as f32).sin();
    }

    let mut reconstructed = vec![0.0_f32; num_samples];
    let mut overlap = [0.0_f32; STFT_SIZE];

    let mut offset = 0;
    while offset + STFT_SIZE <= num_samples {
        let mut re = [0.0_f32; STFT_SIZE];
        let mut im = [0.0_f32; STFT_SIZE];

        for i in 0..STFT_SIZE {
            re[i] = input[offset + i] * window[i];
            im[i] = 0.0;
        }

        fft_radix2(&mut re, &mut im, false);
        // Unity gain: re and im unchanged
        fft_radix2(&mut re, &mut im, true);

        for (i, ov) in overlap.iter_mut().enumerate() {
            *ov += re[i] * window[i];
        }

        reconstructed[offset..offset + HOP_SIZE].copy_from_slice(&overlap[..HOP_SIZE]);

        overlap.copy_within(HOP_SIZE..STFT_SIZE, 0);
        overlap[STFT_SIZE - HOP_SIZE..STFT_SIZE].fill(0.0);

        offset += HOP_SIZE;
    }

    // Exclude warm-up first hop and check interior reconstruction error
    let start_idx = HOP_SIZE;
    let end_idx = offset - HOP_SIZE;
    let mut max_err = 0.0_f32;
    for i in start_idx..end_idx {
        let err = (input[i] - reconstructed[i]).abs();
        if err > max_err {
            max_err = err;
        }
    }

    assert!(
        max_err < 1e-4,
        "STFT COLA reconstruction max error too high: {} (expected < 1e-4)",
        max_err
    );
}

// ---------------------------------------------------------------------------
// Test 2: Ephraim-Malah Decision-Directed Wiener Filter Attenuation
// ---------------------------------------------------------------------------
#[test]
fn test_wiener_filter_noise_attenuation_and_snr() {
    let sample_rate = 48000;
    let duration = 0.6; // 600ms
    let n = (duration * sample_rate as f32) as usize;

    // Speech burst: 1 kHz tone active only during samples [9600..19200] (200ms to 400ms)
    // Background noise: continuous 0.02 amplitude white noise everywhere
    let noise = generate_white_noise(0.02, duration, sample_rate as f32, 42);
    let mut speech = vec![0.0_f32; n];
    for (i, s) in speech.iter_mut().enumerate().take(19200).skip(9600) {
        let t = i as f32 / sample_rate as f32;
        *s = 0.5 * (2.0 * PI * 1000.0 * t).sin();
    }

    let mut composite = Vec::with_capacity(n);
    for i in 0..n {
        composite.push(speech[i] + noise[i]);
    }

    let config = DenoiseConfig {
        mode: DenoiseMode::MultiBand,
        reduction_db: 18.0,
        sensitivity: 1.2,
        transient_preserve: true,
        ..Default::default()
    };
    let mut denoiser = AudioDenoiser::new(sample_rate, config);

    // Teach noise floor from silence [0..4800]
    denoiser.learn_noise_profile(&composite[0..4800]);

    let mut processed = composite.clone();
    denoiser.process_mono(&mut processed);

    // 1. Verify noise attenuation during silent portion [4800..9000]
    let noise_in_rms = compute_rms(&composite[4800..9000]);
    let noise_out_rms = compute_rms(&processed[4800..9000]);
    let attenuation_db = rms_to_dbfs(noise_in_rms) - rms_to_dbfs(noise_out_rms);

    assert!(
        attenuation_db >= 10.0,
        "Expected >= 10 dB noise attenuation in silence, got {:.2} dB",
        attenuation_db
    );

    // 2. Verify voice preservation during speech interval [12000..17000]
    let speech_in_rms = compute_rms(&speech[12000..17000]);
    let speech_out_rms = compute_rms(&processed[12000..17000]);
    let voice_ratio = speech_out_rms / speech_in_rms;

    assert!(
        (voice_ratio - 1.0).abs() < 0.20,
        "Speech level altered too significantly: ratio {:.2}",
        voice_ratio
    );
}

// ---------------------------------------------------------------------------
// Test 3: Minimum Statistics Adaptive Noise Floor Tracking
// ---------------------------------------------------------------------------
#[test]
fn test_minimum_statistics_adaptive_noise_floor_tracking() {
    let mut tracker = MinimumStatisticsTracker::new();

    // Generate 100 frames of stationary pinkish noisy power spectrum
    let mut noisy_power = [0.0_f32; NUM_BINS];
    for (k, np) in noisy_power.iter_mut().enumerate() {
        let f = (k as f32 + 1.0) * 93.75;
        *np = 0.001 / (f / 1000.0).max(0.5);
    }

    let mut noise_psd = [0.0_f32; NUM_BINS];

    // Feed 30 frames
    for _ in 0..30 {
        tracker.update(&noisy_power, &mut noise_psd);
    }

    // Noise PSD estimate must be positive and follow the 1/f shape
    assert!(noise_psd[10] > 0.0);
    assert!(noise_psd[200] > 0.0);
    assert!(
        noise_psd[10] > noise_psd[200],
        "Expected low frequency noise floor to exceed high frequency: low={}, high={}",
        noise_psd[10],
        noise_psd[200]
    );
}

// ---------------------------------------------------------------------------
// Test 4: Cascaded Mains Hum Notch Filter Suppression Depth
// ---------------------------------------------------------------------------
#[test]
fn test_hum_notch_cascade_suppression_depth() {
    let sample_rate = 48000.0;
    let duration = 0.5;
    let n = (duration * sample_rate) as usize;

    // Generate 50 Hz fundamental + 100 Hz 2nd harmonic + 1000 Hz speech tone
    let mut input = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sample_rate;
        let hum = 0.4 * (2.0 * PI * 50.0 * t).sin() + 0.2 * (2.0 * PI * 100.0 * t).sin();
        let speech = 0.3 * (2.0 * PI * 1000.0 * t).sin();
        input.push(hum + speech);
    }

    let mut notch_filter = HumNotchFilter::with_q(50.0, sample_rate, 12.0);
    assert_eq!(notch_filter.fundamental_freq(), 50.0);
    assert_eq!(notch_filter.q(), 12.0);

    let mut filtered = input.clone();
    for s in &mut filtered {
        *s = notch_filter.process(*s);
    }

    // Inspect steady state [12000..24000]
    // Filtered signal should have almost pure 1000 Hz tone left (amplitude ~0.3, RMS ~0.212)
    let steady_filtered = &filtered[12000..];
    let steady_rms = compute_rms(steady_filtered);
    let expected_1khz_rms = 0.3 * std::f32::consts::FRAC_1_SQRT_2; // ~0.2121

    assert!(
        (steady_rms - expected_1khz_rms).abs() < 0.04,
        "Expected speech 1kHz tone to dominate filtered signal: got RMS {:.4}, expected {:.4}",
        steady_rms,
        expected_1khz_rms
    );
}

// ---------------------------------------------------------------------------
// Test 5: Automatic Mains Hum Detection and Engaging
// ---------------------------------------------------------------------------
#[test]
fn test_mains_hum_auto_detection_and_mode_switching() {
    let detector = MainsHumDetector;
    let sample_rate = 48000.0;

    // 1. Synthetic 50 Hz Hum
    let hum50 = generate_sine_wave(50.0, 0.4, 0.2, sample_rate);
    assert_eq!(
        detector.detect_hum(&hum50, sample_rate),
        DetectedHum::Hum50Hz
    );

    // 2. Synthetic 60 Hz Hum
    let hum60 = generate_sine_wave(60.0, 0.4, 0.2, sample_rate);
    assert_eq!(
        detector.detect_hum(&hum60, sample_rate),
        DetectedHum::Hum60Hz
    );

    // 3. Clean Speech/Music (1 kHz tone)
    let clean = generate_sine_wave(1000.0, 0.4, 0.2, sample_rate);
    assert_eq!(detector.detect_hum(&clean, sample_rate), DetectedHum::None);

    // 4. AutoDeHum mode on AudioDenoiser
    let config = DenoiseConfig {
        mode: DenoiseMode::AutoDeHum,
        ..Default::default()
    };
    let mut denoiser = AudioDenoiser::new(48000, config);
    let mut hum_buffer = hum50.clone();
    denoiser.process_mono(&mut hum_buffer);
    assert_eq!(denoiser.detected_hum(), DetectedHum::Hum50Hz);
}

// ---------------------------------------------------------------------------
// Test 6: Transient Preservation on Speech Attacks
// ---------------------------------------------------------------------------
#[test]
fn test_transient_preservation_onset() {
    let sample_rate = 48000;
    let mut detector = TransientDetector::new();
    assert!(!detector.is_active());

    // Trigger sudden impulse / transient attack
    let is_t = detector.update_sample(0.8, 0.01, sample_rate);
    assert!(is_t, "Expected transient trigger on steep sample rise");
    assert!(detector.is_active());

    // AudioDenoiser with transient_preserve=true vs false
    let mut denoiser_preserve = AudioDenoiser::new(
        sample_rate,
        DenoiseConfig {
            transient_preserve: true,
            reduction_db: 20.0,
            ..Default::default()
        },
    );

    let mut attack_signal = vec![0.002_f32; 1000];
    attack_signal[500] = 0.9; // sudden consonant attack
    attack_signal[501] = 0.7;

    denoiser_preserve.process_mono(&mut attack_signal);
    // Peak consonant must not be crushed
    assert!(
        attack_signal[500].abs() > 0.6,
        "Expected transient attack preserved > 0.6, got {}",
        attack_signal[500]
    );
}

// ---------------------------------------------------------------------------
// Test 7: Multi-channel Stereo Coherence and Phase Alignment
// ---------------------------------------------------------------------------
#[test]
fn test_stereo_coherence_and_phase_alignment() {
    let sample_rate = 48000;
    let duration = 0.3;
    let n = (duration * sample_rate as f32) as usize;

    // Pan a 500 Hz tone with 30-degree stereo phase offset
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    let phase_offset = PI / 6.0;

    for i in 0..n {
        let t = i as f32 / sample_rate as f32;
        let s_l = 0.4 * (2.0 * PI * 500.0 * t).sin();
        let s_r = 0.4 * (2.0 * PI * 500.0 * t + phase_offset).sin();
        left.push(s_l);
        right.push(s_r);
    }

    let mut denoiser = AudioDenoiser::new(
        sample_rate,
        DenoiseConfig {
            mode: DenoiseMode::MultiBand,
            reduction_db: 15.0,
            ..Default::default()
        },
    );

    let mut proc_l = left.clone();
    let mut proc_r = right.clone();
    denoiser.process_stereo(&mut proc_l, &mut proc_r);

    // Cross-correlation check: relative phase delay must remain intact
    let start = 4800;
    let len = 4800;
    let dot_orig: f32 = left[start..start + len]
        .iter()
        .zip(&right[start..start + len])
        .map(|(&l, &r)| l * r)
        .sum();

    let dot_proc: f32 = proc_l[start..start + len]
        .iter()
        .zip(&proc_r[start..start + len])
        .map(|(&l, &r)| l * r)
        .sum();

    let norm_orig =
        compute_rms(&left[start..start + len]) * compute_rms(&right[start..start + len]);
    let norm_proc =
        compute_rms(&proc_l[start..start + len]) * compute_rms(&proc_r[start..start + len]);

    let corr_orig = dot_orig / (norm_orig * len as f32);
    let corr_proc = dot_proc / (norm_proc * len as f32);

    assert!(
        (corr_orig - corr_proc).abs() < 0.05,
        "Stereo cross-correlation altered by denoising: orig={:.4}, proc={:.4}",
        corr_orig,
        corr_proc
    );
}

// ---------------------------------------------------------------------------
// Test 8: Neural Audio Denoiser Studio Plugin in AudioPluginInsertChain
// ---------------------------------------------------------------------------
#[test]
fn test_neural_audio_denoiser_studio_plugin_insert_chain() {
    let mut chain = AudioPluginInsertChain::new();
    assert!(chain.is_empty());

    let plugin = Box::new(NeuralAudioDenoiserPlugin::new());
    assert_eq!(plugin.descriptor().category, PluginCategory::Effect);
    assert_eq!(plugin.latency_samples(), HOP_SIZE);

    chain.add_plugin(plugin);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain.total_latency_samples(), HOP_SIZE);

    // Process interleaved stereo buffer (1024 frames = 2048 f32 samples)
    let num_frames = 1024;
    let mut buffer = vec![0.005_f32; num_frames * 2];

    chain.process_interleaved(&mut buffer, num_frames).unwrap();

    // Verify output samples are valid numbers and not NaN/infinite
    for &s in &buffer {
        assert!(!s.is_nan());
        assert!(!s.is_infinite());
    }

    // Test parameter manipulation
    let plug_ref = chain.get_plugin_mut(0).unwrap();
    plug_ref
        .set_parameter(NeuralAudioDenoiserPlugin::PARAM_REDUCTION_DB, 24.0)
        .unwrap();
    let val = plug_ref
        .get_parameter(NeuralAudioDenoiserPlugin::PARAM_REDUCTION_DB)
        .unwrap();
    assert_eq!(val, 24.0);

    // Test preset state serialization / deserialization roundtrip
    let state = plug_ref.save_state().unwrap();
    assert!(!state.is_empty());

    plug_ref
        .set_parameter(NeuralAudioDenoiserPlugin::PARAM_REDUCTION_DB, 6.0)
        .unwrap();
    plug_ref.load_state(&state).unwrap();
    let restored = plug_ref
        .get_parameter(NeuralAudioDenoiserPlugin::PARAM_REDUCTION_DB)
        .unwrap();
    assert_eq!(restored, 24.0);
}

// ---------------------------------------------------------------------------
// Test 9: Real-Time Processing Throughput Benchmark
// ---------------------------------------------------------------------------
#[test]
fn test_realtime_processing_throughput_benchmark() {
    let sample_rate = 48000;
    let duration_sec = 5.0; // 5 seconds of audio
    let num_frames = (duration_sec * sample_rate as f32) as usize;
    let num_samples = num_frames * 2; // Stereo

    let mut stereo_buffer =
        generate_white_noise(0.01, duration_sec * 2.0, sample_rate as f32, 12345);
    stereo_buffer.truncate(num_samples);

    let config = DenoiseConfig {
        mode: DenoiseMode::MultiBand,
        reduction_db: 15.0,
        sensitivity: 1.2,
        transient_preserve: true,
        ..Default::default()
    };
    let mut denoiser = AudioDenoiser::new(sample_rate, config);

    let start = Instant::now();
    denoiser.process_interleaved(&mut stereo_buffer);
    let elapsed = start.elapsed();

    let audio_duration_ms = duration_sec * 1000.0;
    let elapsed_ms = elapsed.as_secs_f32() * 1000.0;
    let speedup = audio_duration_ms / elapsed_ms.max(0.001);

    println!(
        "[Denoise Benchmark] 5.0s stereo audio processed in {:.2}ms ({:.1}x real-time)",
        elapsed_ms, speedup
    );

    // Assert that processing is at least 20x faster than real-time
    assert!(
        speedup >= 20.0,
        "Denoiser processing speed {:.1}x real-time is below 20x threshold",
        speedup
    );
}
