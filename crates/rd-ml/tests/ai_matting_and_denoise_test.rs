//! End-to-end integration test for AI Portrait Matting & Neural Audio Denoising.

use rd_effects::{EffectRegistry, NeuralMatteParams, apply_neural_matte, evaluate_matte_alpha};
use rd_ml::{
    AudioDenoiser, CompositeBackground, DenoiseConfig, DenoiseMode, PortraitMattingConfig,
    PortraitMattingEngine, composite_segmented_frame, suppress_color_spill,
};

#[test]
fn test_end_to_end_neural_audio_denoiser() {
    let sample_rate = 48000;
    let n_samples = 48000; // 1 second of audio
    let mut audio_signal = vec![0.0_f32; n_samples];

    // 1. Add background noise: stationary room hiss (RMS ~ 0.005) + 50 Hz hum
    for (i, sample) in audio_signal.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        let hum = (2.0 * std::f32::consts::PI * 50.0 * t).sin() * 0.01;
        let hiss = (i as f32 * 0.123).sin() * 0.005;
        *sample = hum + hiss;
    }

    // 2. Add speech utterance between 0.3s and 0.7s (samples 14400..33600)
    for (i, sample) in audio_signal[14400..33600].iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        // Speech formant simulation: 300Hz fundamental + 1200Hz formant
        let speech = (2.0 * std::f32::consts::PI * 300.0 * t).sin() * 0.4
            + (2.0 * std::f32::consts::PI * 1200.0 * t).sin() * 0.2;
        *sample += speech;
    }

    // 3. Process with AudioDenoiser in VoiceFocus mode with 50Hz dehumming
    let config = DenoiseConfig {
        reduction_db: 15.0,
        sensitivity: 1.2,
        attack_ms: 5.0,
        release_ms: 50.0,
        mode: DenoiseMode::DeHum50,
        transient_preserve: true,
        profile: None,
    };
    let mut denoiser = AudioDenoiser::new(sample_rate, config);

    // Teach background noise from the first 0.2s of silence
    denoiser.learn_noise_profile(&audio_signal[0..9600]);

    let mut processed_audio = audio_signal.clone();
    denoiser.process_interleaved(&mut processed_audio);

    // Verify background silence interval (samples 4000..9000): significant attenuation
    let silence_slice = &processed_audio[4000..9000];
    let silence_rms: f32 =
        (silence_slice.iter().map(|s| s * s).sum::<f32>() / silence_slice.len() as f32).sqrt();
    assert!(
        silence_rms < 0.006,
        "Expected attenuated noise floor, got RMS {}",
        silence_rms
    );

    // Verify speech interval (samples 20000..30000): voice preserved
    let speech_slice = &processed_audio[20000..30000];
    let speech_rms: f32 =
        (speech_slice.iter().map(|s| s * s).sum::<f32>() / speech_slice.len() as f32).sqrt();
    assert!(
        speech_rms > 0.25,
        "Expected speech level preserved, got RMS {}",
        speech_rms
    );
}

#[test]
fn test_end_to_end_portrait_matting_and_despill_pipeline() {
    let w = 64u32;
    let h = 64u32;
    let n = (w * h) as usize;

    // Green screen background: R=20, G=210, B=30, A=255
    let mut frame = vec![0u8; n * 4];
    for chunk in frame.chunks_exact_mut(4) {
        chunk[0] = 20;
        chunk[1] = 210;
        chunk[2] = 30;
        chunk[3] = 255;
    }

    // Add centered portrait subject (oval head/torso) with warm skin and green fringe
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 - cx) / 16.0;
            let dy = (y as f32 - cy) / 22.0;
            if dx * dx + dy * dy <= 1.0 {
                let px = ((y * w + x) * 4) as usize;
                frame[px] = 215; // Red
                frame[px + 1] = 165; // Green (with slight spill)
                frame[px + 2] = 130; // Blue
                frame[px + 3] = 255;
            }
        }
    }

    // 1. Portrait matting engine segmentation
    let config = PortraitMattingConfig {
        threshold: 0.40,
        feather_radius: 2,
        edge_choke: 0,
        spill_suppression: 0.7,
        temporal_smooth: 0.0,
    };
    let mut engine = PortraitMattingEngine::new(config);
    let mask = engine.segment_frame(&frame, w, h);

    // Subject core (center) should have high alpha
    assert!(mask.get_alpha(32, 32) > 200, "Expected opaque center");
    // Outer green screen corners should be keyed out (alpha near 0)
    assert!(mask.get_alpha(2, 2) < 40, "Expected transparent corner");

    // 2. Despill verification
    let mut despilled_frame = frame.clone();
    suppress_color_spill(&mut despilled_frame, 0.75);

    // Green screen corner despill
    let corner_px = 0;
    assert!(despilled_frame[corner_px + 1] < frame[corner_px + 1]);

    // 3. Composite over new solid background (Dark Gray studio backdrop: [40, 40, 40, 255])
    let mut composited = vec![0u8; n * 4];
    composite_segmented_frame(
        &despilled_frame,
        &mask,
        CompositeBackground::SolidColor([40, 40, 40, 255]),
        &mut composited,
    );

    // Center pixel should display the warm subject
    let center_px = ((32 * w + 32) * 4) as usize;
    assert!(
        composited[center_px] > 180,
        "Expected warm skin in composite"
    );

    // Corner pixel should display the studio backdrop
    assert_eq!(composited[corner_px], 40);
    assert_eq!(composited[corner_px + 1], 40);
    assert_eq!(composited[corner_px + 2], 40);
}

#[test]
fn test_neural_matte_effect_registry_and_manifest() {
    let registry = EffectRegistry::new();
    let entry = registry
        .get("neural_matte")
        .expect("neural_matte must be registered");

    assert_eq!(entry.manifest.name, "AI Portrait Matte");
    assert_eq!(entry.manifest.parameters.len(), 6);

    let threshold_param = entry
        .manifest
        .parameters
        .iter()
        .find(|p| p.id == "threshold")
        .unwrap();
    assert_eq!(threshold_param.default, 0.45);

    let spill_param = entry
        .manifest
        .parameters
        .iter()
        .find(|p| p.id == "spill")
        .unwrap();
    assert_eq!(spill_param.default, 0.6);

    // Test CPU software evaluator
    let params = NeuralMatteParams::default();
    let alpha = evaluate_matte_alpha(0.5, 0.5, 0.85, 0.60, 0.50, &params);
    assert!(alpha > 0.8);

    let mut frame_buf = vec![128u8; 16 * 16 * 4];
    apply_neural_matte(&mut frame_buf, 16, 16, &params);
    assert_eq!(frame_buf.len(), 16 * 16 * 4);
}
