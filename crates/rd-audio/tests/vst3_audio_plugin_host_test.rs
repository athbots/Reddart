//! Integration tests for VST3 / AU Audio Plugin Hosting and DSP insert chains.

use rd_audio::{
    AudioPlugin, AudioPluginInsertChain, BrickwallLimiterPlugin, ParametricEqPlugin,
    PluginCategory, PluginFormat, PluginScanner, StudioCompressorPlugin,
};
use std::f32::consts::PI;

/// Helper to generate a continuous sine wave test tone.
fn generate_sine_wave(
    freq_hz: f32,
    sample_rate: f32,
    num_samples: usize,
    amplitude: f32,
) -> Vec<f32> {
    let mut buffer = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate;
        buffer.push((2.0 * PI * freq_hz * t).sin() * amplitude);
    }
    buffer
}

#[test]
fn test_studio_compressor_dynamics_reduction() {
    let mut compressor = StudioCompressorPlugin::new();
    assert_eq!(compressor.descriptor().category, PluginCategory::Dynamics);
    assert_eq!(compressor.descriptor().format, PluginFormat::InternalDsp);

    // Configure compressor: Threshold = -18 dBFS, Ratio = 4:1, Attack = 5ms, Release = 50ms
    compressor
        .set_parameter(StudioCompressorPlugin::PARAM_THRESHOLD_DB, -18.0)
        .expect("Set threshold");
    compressor
        .set_parameter(StudioCompressorPlugin::PARAM_RATIO, 4.0)
        .expect("Set ratio");
    compressor
        .set_parameter(StudioCompressorPlugin::PARAM_ATTACK_MS, 5.0)
        .expect("Set attack");
    compressor
        .set_parameter(StudioCompressorPlugin::PARAM_RELEASE_MS, 50.0)
        .expect("Set release");
    compressor
        .set_parameter(StudioCompressorPlugin::PARAM_MAKEUP_GAIN_DB, 0.0)
        .expect("Set makeup");

    let num_samples = 4800; // 100 ms at 48 kHz
    let hot_sine = generate_sine_wave(1000.0, 48000.0, num_samples, 1.0); // 0 dBFS peak

    let mut out_l = vec![0.0; num_samples];
    let mut out_r = vec![0.0; num_samples];

    compressor
        .process(&hot_sine, &hot_sine, &mut out_l, &mut out_r)
        .expect("Process audio");

    // Once attack time settles (second half of buffer), output peak must be compressed
    let in_peak = hot_sine[2400..]
        .iter()
        .fold(0.0f32, |acc, &s| acc.max(s.abs()));
    let out_peak = out_l[2400..]
        .iter()
        .fold(0.0f32, |acc, &s| acc.max(s.abs()));

    assert_eq!(in_peak, 1.0);
    assert!(
        out_peak < 0.6,
        "Compressed output peak {} must be significantly below input peak 1.0",
        out_peak
    );
    assert!(
        compressor.gain_reduction_db() > 4.0,
        "Gain reduction must be active (measured: {} dB)",
        compressor.gain_reduction_db()
    );

    // Quiet audio below threshold (-30 dBFS ~ 0.0316) should experience near zero gain reduction
    let quiet_sine = generate_sine_wave(1000.0, 48000.0, num_samples, 0.02);
    compressor.reset(48000.0);
    compressor
        .process(&quiet_sine, &quiet_sine, &mut out_l, &mut out_r)
        .expect("Process quiet audio");

    let quiet_out_peak = out_l[2400..]
        .iter()
        .fold(0.0f32, |acc, &s| acc.max(s.abs()));
    let ratio = quiet_out_peak / 0.02;
    assert!(
        (ratio - 1.0).abs() < 0.05,
        "Quiet signals below threshold must pass with unity gain (ratio: {})",
        ratio
    );
}

#[test]
fn test_parametric_eq_frequency_filtering() {
    let mut eq = ParametricEqPlugin::new();
    assert_eq!(eq.descriptor().category, PluginCategory::Equalizer);

    // Boost Low-Mid Bell at 1000 Hz by +12 dB with Q = 2.0
    eq.set_parameter(ParametricEqPlugin::PARAM_BELL2_FREQ, 1000.0)
        .expect("Set freq");
    eq.set_parameter(ParametricEqPlugin::PARAM_BELL2_GAIN_DB, 12.0)
        .expect("Set boost");
    eq.set_parameter(ParametricEqPlugin::PARAM_BELL2_Q, 2.0)
        .expect("Set Q");

    let num_samples = 4800;
    let input_1k = generate_sine_wave(1000.0, 48000.0, num_samples, 0.1);

    let mut out_l = vec![0.0; num_samples];
    let mut out_r = vec![0.0; num_samples];

    eq.process(&input_1k, &input_1k, &mut out_l, &mut out_r)
        .expect("EQ process");

    let in_peak = input_1k[2400..]
        .iter()
        .fold(0.0f32, |acc, &s| acc.max(s.abs()));
    let out_peak = out_l[2400..]
        .iter()
        .fold(0.0f32, |acc, &s| acc.max(s.abs()));

    // +12 dB is approximately a factor of ~3.98x
    let measured_gain = out_peak / in_peak;
    assert!(
        (measured_gain - 3.98).abs() < 0.5,
        "Measured gain at 1kHz {} must be near expected 12dB boost (~3.98x)",
        measured_gain
    );
}

#[test]
fn test_brickwall_limiter_strict_ceiling() {
    let mut limiter = BrickwallLimiterPlugin::new();
    assert_eq!(limiter.descriptor().category, PluginCategory::Limiter);
    assert_eq!(limiter.latency_samples(), 48); // 1ms lookahead

    // Set ceiling to -1.0 dBFS (~0.8912)
    limiter
        .set_parameter(BrickwallLimiterPlugin::PARAM_CEILING_DBFS, -1.0)
        .expect("Set ceiling");

    // Hot test signal peaking at 2.0 (+6 dBFS)
    let num_samples = 4800;
    let hot_signal = generate_sine_wave(440.0, 48000.0, num_samples, 2.0);

    let mut out_l = vec![0.0; num_samples];
    let mut out_r = vec![0.0; num_samples];

    limiter
        .process(&hot_signal, &hot_signal, &mut out_l, &mut out_r)
        .expect("Limiter process");

    let ceiling_linear = 10.0f32.powf(-1.0 / 20.0); // ~0.89125

    // Verify every single sample in the output does not exceed the ceiling + epsilon
    for (i, &s) in out_l.iter().enumerate() {
        assert!(
            s.abs() <= ceiling_linear + 0.001,
            "Sample {} value {} exceeded ceiling {}",
            i,
            s.abs(),
            ceiling_linear
        );
    }
}

#[test]
fn test_plugin_insert_chain_routing_and_latency() {
    let mut chain = AudioPluginInsertChain::new();
    assert!(chain.is_empty());

    chain.add_plugin(Box::new(StudioCompressorPlugin::new()));
    chain.add_plugin(Box::new(ParametricEqPlugin::new()));
    chain.add_plugin(Box::new(BrickwallLimiterPlugin::new()));

    assert_eq!(chain.len(), 3);
    // Total latency: 0 (comp) + 0 (eq) + 48 (limiter lookahead) = 48
    assert_eq!(chain.total_latency_samples(), 48);

    // Process interleaved stereo buffer (512 frames = 1024 samples)
    let num_frames = 512;
    let mut interleaved = vec![0.5f32; num_frames * 2];

    chain
        .process_interleaved(&mut interleaved, num_frames)
        .expect("Process chain");

    // Output must remain valid and bounded
    for &sample in &interleaved {
        assert!(sample.is_finite());
        assert!(sample.abs() <= 1.0);
    }
}

#[test]
fn test_plugin_bypass_and_wet_dry_mix() {
    let mut comp = StudioCompressorPlugin::new();
    comp.set_parameter(StudioCompressorPlugin::PARAM_THRESHOLD_DB, -30.0)
        .unwrap();
    comp.set_parameter(StudioCompressorPlugin::PARAM_RATIO, 10.0)
        .unwrap();

    let num_frames = 256;
    let original = vec![0.8f32; num_frames * 2];

    // 1. When bypassed, output must be bit-for-bit identical to input
    comp.set_bypassed(true);
    let mut buffer = original.clone();
    comp.process_interleaved(&mut buffer, num_frames).unwrap();
    assert_eq!(
        buffer, original,
        "Bypassed plugin must pass bit-for-bit identical audio"
    );

    // 2. When wet/dry mix is 0.0 (100% dry), output must also be identical to input
    comp.set_bypassed(false);
    comp.set_wet_dry_mix(0.0);
    let mut buffer = original.clone();
    comp.process_interleaved(&mut buffer, num_frames).unwrap();
    assert_eq!(
        buffer, original,
        "0% wet mix must pass bit-for-bit identical audio"
    );

    // 3. When wet/dry mix is 1.0 (100% wet), audio is fully compressed
    comp.set_wet_dry_mix(1.0);
    let mut buffer_wet = original.clone();
    comp.process_interleaved(&mut buffer_wet, num_frames)
        .unwrap();
    assert_ne!(buffer_wet, original);
}

#[test]
fn test_parameter_automation_and_state_serialization() {
    let mut comp = StudioCompressorPlugin::new();

    // Modify default parameters
    comp.set_parameter(StudioCompressorPlugin::PARAM_THRESHOLD_DB, -24.5)
        .unwrap();
    comp.set_parameter(StudioCompressorPlugin::PARAM_RATIO, 8.0)
        .unwrap();
    comp.set_parameter(StudioCompressorPlugin::PARAM_ATTACK_MS, 2.5)
        .unwrap();

    // Serialize state
    let state_bytes = comp.save_state().expect("Save state");
    assert!(!state_bytes.is_empty());

    // Create a new instance and restore state
    let mut comp_restored = StudioCompressorPlugin::new();
    assert_ne!(
        comp_restored
            .get_parameter(StudioCompressorPlugin::PARAM_THRESHOLD_DB)
            .unwrap(),
        -24.5
    );

    comp_restored.load_state(&state_bytes).expect("Load state");
    assert_eq!(
        comp_restored
            .get_parameter(StudioCompressorPlugin::PARAM_THRESHOLD_DB)
            .unwrap(),
        -24.5
    );
    assert_eq!(
        comp_restored
            .get_parameter(StudioCompressorPlugin::PARAM_RATIO)
            .unwrap(),
        8.0
    );
    assert_eq!(
        comp_restored
            .get_parameter(StudioCompressorPlugin::PARAM_ATTACK_MS)
            .unwrap(),
        2.5
    );
}

#[test]
fn test_plugin_scanner_standard_directories() {
    let dirs = PluginScanner::standard_plugin_directories();
    assert!(
        !dirs.is_empty(),
        "Plugin scanner must provide standard OS plugin directories"
    );

    #[cfg(target_os = "windows")]
    {
        let has_common_vst3 = dirs.iter().any(|d| {
            d.to_string_lossy().contains("Common Files") && d.to_string_lossy().contains("VST3")
        });
        assert!(
            has_common_vst3,
            "Must include Windows Common Files VST3 path"
        );
    }
}
