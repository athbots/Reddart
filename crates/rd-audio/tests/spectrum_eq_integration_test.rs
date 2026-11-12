//! End-to-end integration tests for Real-Time Audio Spectrum Analyzer & Parametric EQ.

use rd_audio::eq::ParametricEqualizer;
use rd_audio::spectrum::{AudioSampleRing, SpectrumAnalyzer};
use std::f32::consts::PI;

#[test]
fn test_multiband_parametric_eq_serialization_roundtrip() {
    let mut eq = ParametricEqualizer::new(48000.0);
    eq.output_gain_db = -2.5;
    eq.set_band_params(0, 35.0, 0.0, 0.707, true); // Low Cut
    eq.set_band_params(1, 120.0, 4.5, 0.8, true); // Low Shelf +4.5 dB
    eq.set_band_params(2, 2400.0, -3.0, 2.5, true); // Presence Cut -3 dB
    eq.set_band_params(3, 12000.0, 6.0, 0.707, true); // Air Boost +6 dB
    eq.set_band_params(4, 19000.0, 0.0, 0.707, false); // High Cut bypassed

    let serialized = serde_json::to_string_pretty(&eq).expect("Failed to serialize EQ");
    let mut deserialized: ParametricEqualizer =
        serde_json::from_str(&serialized).expect("Failed to deserialize EQ");

    deserialized.sample_rate = 48000.0;
    deserialized.recompute_all();

    assert_eq!(deserialized.enabled, eq.enabled);
    assert_eq!(deserialized.output_gain_db, eq.output_gain_db);
    assert_eq!(deserialized.bands.len(), 5);
    assert_eq!(deserialized.bands[1].gain_db, 4.5);
    assert_eq!(deserialized.bands[2].frequency_hz, 2400.0);
    assert!(!deserialized.bands[4].enabled);

    // Verify identical composite curve
    let curve_orig = eq.compute_composite_curve(128);
    let curve_deser = deserialized.compute_composite_curve(128);
    for (a, b) in curve_orig.iter().zip(curve_deser.iter()) {
        assert!((a - b).abs() < 1e-4, "Curves deviate: {} vs {}", a, b);
    }
}

#[test]
fn test_audio_sample_ring_multithreaded_concurrency() {
    use std::sync::Arc;
    use std::thread;

    let ring = Arc::new(std::sync::Mutex::new(AudioSampleRing::new(2048)));

    let ring_producer = ring.clone();
    let producer = thread::spawn(move || {
        for chunk_idx in 0..100 {
            let mut frames = [0.0f32; 128];
            for (i, val) in frames.iter_mut().enumerate() {
                *val = (chunk_idx * 128 + i) as f32;
            }
            if let Ok(mut r) = ring_producer.lock() {
                r.push_interleaved(&frames, 2);
            }
            std::thread::yield_now();
        }
    });

    let ring_consumer = ring.clone();
    let consumer = thread::spawn(move || {
        let mut read_buf = [0.0f32; 256];
        for _ in 0..50 {
            if let Ok(r) = ring_consumer.lock() {
                r.read_latest(&mut read_buf);
            }
            std::thread::yield_now();
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}

#[test]
fn test_spectrum_analyzer_multitone_harmonic_detection() {
    let mut analyzer = SpectrumAnalyzer::new(2048, 48000.0, 128);

    // Synthesize two simultaneous pure tones:
    // Low tone: 100 Hz (ISO band index 7: 100 Hz)
    // High tone: 5000 Hz (ISO band index 24: 5000 Hz)
    let num_samples = 2048;
    let mut samples = vec![0.0f32; num_samples];
    for (i, s) in samples.iter_mut().enumerate() {
        let t = i as f32 / 48000.0;
        let low = (2.0 * PI * 100.0 * t).sin() * 0.5;
        let high = (2.0 * PI * 5000.0 * t).sin() * 0.5;
        *s = low + high;
    }

    analyzer.process_samples(&samples, 0.05);

    let bands = analyzer.bands();
    let band_100hz = bands[7];
    let band_5khz = bands[24];
    let band_1khz = bands[17]; // Silence between the two tones

    assert!(
        band_100hz > -15.0,
        "100 Hz harmonic should be detected strongly (got {:.1} dBFS)",
        band_100hz
    );
    assert!(
        band_5khz > -15.0,
        "5 kHz harmonic should be detected strongly (got {:.1} dBFS)",
        band_5khz
    );
    assert!(
        band_100hz > band_1khz + 25.0,
        "100 Hz peak ({:.1} dB) should be substantially higher than 1 kHz notch ({:.1} dB)",
        band_100hz,
        band_1khz
    );
    assert!(
        band_5khz > band_1khz + 25.0,
        "5 kHz peak ({:.1} dB) should be substantially higher than 1 kHz notch ({:.1} dB)",
        band_5khz,
        band_1khz
    );
}

#[test]
fn test_parametric_equalizer_frequency_response_accuracy() {
    let mut eq = ParametricEqualizer::new(48000.0);

    // Configure a known bell boost: +9 dB at 2500 Hz, Q = 2.0
    eq.set_band_params(2, 2500.0, 9.0, 2.0, true);

    let curve = eq.compute_composite_curve(256);

    // Map 2500 Hz to curve index
    let f_frac = SpectrumAnalyzer::freq_to_fraction(2500.0);
    let target_idx = (f_frac * 255.0).round() as usize;

    let response_at_target = curve[target_idx];
    assert!(
        (response_at_target - 9.0).abs() < 0.6,
        "Composite curve response at 2.5 kHz ({:.2} dB) deviates from +9 dB target",
        response_at_target
    );
}
