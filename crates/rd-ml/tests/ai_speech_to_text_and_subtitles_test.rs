//! Integration Test Suite: AI Neural Speech-to-Text, Voice Activity Detection & Subtitle Track Generator
//!
//! Validates:
//! 1. High-fidelity audio downmixing and 16,000 Hz resampling with peak headroom normalization.
//! 2. 80-channel Log-Mel spectrogram filterbank feature extraction with 512-point FFT.
//! 3. Enhanced Voice Activity Detection (VAD) with adaptive noise floor and 300ms hangover.
//! 4. Acoustic syllable & script forced alignment with word-level microsecond timestamps.
//! 5. Caption typography styling mapped to GPU SDF text clips (outlines, drop shadows, tracking).
//! 6. Timeline subtitle track injection and bidirectional SubRip (.srt) / WebVTT (.vtt) roundtrip.
//! 7. Subtitle quality validation (reading speed CPS, minimum/maximum duration, cue overlap resolution).
//! 8. HTML/WebVTT tag sanitization.
//! 9. Compatibility with vector GPU SDF text overlay rendering.

use rd_core::{ClipKind, Timeline};
use rd_effects::text::{SdfTypographyParams, render_sdf_text_overlay};
use rd_ml::captions::{
    CaptionSegment, CaptionStyle, CaptionValidationSeverity, apply_captions_to_timeline,
    export_timeline_subtitles_srt, export_timeline_subtitles_vtt, generate_caption_track,
    parse_srt, parse_vtt, resolve_caption_overlaps, sanitize_subtitle_text,
    validate_caption_segments,
};
use rd_ml::whisper::{
    AcousticAligner, AudioPreprocessor, LogMelSpectrogram, VoiceActivityDetector, WHISPER_N_MELS,
    WHISPER_SAMPLE_RATE, WhisperConfig, compute_log_mel_spectrogram,
};

#[test]
fn test_audio_downsampling_and_headroom_normalization() {
    // 48 kHz stereo audio: 2 seconds = 96,000 stereo pairs
    let sample_rate = 48_000;
    let frames = sample_rate * 2;
    let mut pcm_48k = Vec::with_capacity(frames * 2);

    for i in 0..frames {
        let val_l =
            1.4 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin();
        let val_r =
            1.2 * (2.0 * std::f32::consts::PI * 880.0 * i as f32 / sample_rate as f32).sin();
        pcm_48k.push(val_l);
        pcm_48k.push(val_r);
    }

    let mono_16k = AudioPreprocessor::resample_to_16k_mono(&pcm_48k, sample_rate as u32, 2);

    // 2 seconds at 16 kHz = 32,000 samples
    assert_eq!(mono_16k.len(), WHISPER_SAMPLE_RATE as usize * 2);

    // Check peak normalization: max amplitude clamped to <= 0.95
    let max_abs = mono_16k.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!(max_abs <= 0.9501);
    assert!(max_abs >= 0.90);
}

#[test]
fn test_log_mel_spectrogram_80_channels_extraction() {
    let sample_rate = WHISPER_SAMPLE_RATE;
    let duration_s = 1.5;
    let total_samples = (sample_rate as f32 * duration_s) as usize;

    let mut audio = vec![0.0f32; total_samples];
    // Mix 300 Hz and 2500 Hz tones
    for (i, sample) in audio.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        *sample = 0.4 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            + 0.3 * (2.0 * std::f32::consts::PI * 2500.0 * t).sin();
    }

    let spec: LogMelSpectrogram = compute_log_mel_spectrogram(&audio);

    assert_eq!(spec.n_mels, WHISPER_N_MELS);
    assert_eq!(spec.n_mels, 80);
    // (24000 - 400) / 160 + 1 = 148 frames
    assert!(spec.n_frames >= 140 && spec.n_frames <= 160);

    // Verify all Mel features are normalized in [-1.0, 1.0]
    for f in 0..spec.n_frames {
        let frame = spec.frame(f);
        assert_eq!(frame.len(), 80);
        for &val in frame {
            assert!(
                (-1.01..=1.01).contains(&val),
                "Log-Mel value out of range: {val}"
            );
        }
    }

    // Verify low-frequency tone (300 Hz) excites lower Mel bins more than higher bins
    let low_bin_energy: f32 = (0..15).map(|m| spec.get(m, 50)).sum::<f32>() / 15.0;
    let high_bin_energy: f32 = (65..80).map(|m| spec.get(m, 50)).sum::<f32>() / 15.0;
    assert!(
        low_bin_energy >= high_bin_energy,
        "Low-frequency energy {low_bin_energy} should exceed high-frequency energy {high_bin_energy}"
    );
}

#[test]
fn test_adaptive_voice_activity_detection_with_hangover() {
    let sample_rate = WHISPER_SAMPLE_RATE as usize;
    // 3.5 seconds total:
    // 0.0s - 0.5s: ambient noise (RMS ~0.005)
    // 0.5s - 1.4s: Spoken utterance 1
    // 1.4s - 2.1s: ambient noise
    // 2.1s - 3.0s: Spoken utterance 2
    // 3.0s - 3.5s: silence
    let total_samples = sample_rate * 35 / 10;
    let mut audio = vec![0.0f32; total_samples];

    // Ambient room tone
    for (i, sample) in audio.iter_mut().enumerate() {
        *sample = 0.006 * ((i * 17) as f32).sin();
    }

    // Spoken utterance 1: 0.5s to 1.4s
    for (idx, sample) in audio[sample_rate / 2..sample_rate * 14 / 10]
        .iter_mut()
        .enumerate()
    {
        let i = sample_rate / 2 + idx;
        *sample += 0.55 * (i as f32 / 12.0).sin();
    }

    // Spoken utterance 2: 2.1s to 3.0s
    for (idx, sample) in audio[sample_rate * 21 / 10..sample_rate * 30 / 10]
        .iter_mut()
        .enumerate()
    {
        let i = sample_rate * 21 / 10 + idx;
        *sample += 0.60 * (i as f32 / 14.0).sin();
    }

    let config = WhisperConfig {
        vad_filter: true,
        vad_threshold: 0.02,
        min_speech_duration_ms: 200,
        ..Default::default()
    };

    let boundaries = VoiceActivityDetector::detect_speech_enhanced(&audio, &config);
    assert_eq!(
        boundaries.len(),
        2,
        "Expected exactly 2 speech boundaries, found {}",
        boundaries.len()
    );

    // Utterance 1 checks (with lead-in and hangover)
    assert!(boundaries[0].start_us <= 500_000);
    assert!(boundaries[0].start_us >= 400_000); // 80ms lead-in
    assert!(boundaries[0].end_us >= 1_400_000); // 300ms hangover
    assert!(boundaries[0].end_us <= 1_800_000);

    // Utterance 2 checks
    assert!(boundaries[1].start_us <= 2_100_000);
    assert!(boundaries[1].start_us >= 2_000_000);
    assert!(boundaries[1].end_us >= 3_000_000);
    assert!(boundaries[1].end_us <= 3_400_000);
}

#[test]
fn test_acoustic_script_forced_alignment_and_syllable_timing() {
    let sample_rate = WHISPER_SAMPLE_RATE as usize;
    // 3 seconds audio: speech at [0.4s..1.2s] and [1.6s..2.8s]
    let mut audio = vec![0.0f32; sample_rate * 3];
    for (idx, sample) in audio[sample_rate * 4 / 10..sample_rate * 12 / 10]
        .iter_mut()
        .enumerate()
    {
        let i = sample_rate * 4 / 10 + idx;
        *sample = 0.65 * (i as f32 / 10.0).sin();
    }
    for (idx, sample) in audio[sample_rate * 16 / 10..sample_rate * 28 / 10]
        .iter_mut()
        .enumerate()
    {
        let i = sample_rate * 16 / 10 + idx;
        *sample = 0.70 * (i as f32 / 10.0).sin();
    }

    let script = "Welcome to Reddart. Zero latency creative video editing engine.";
    let config = WhisperConfig {
        script_prompt: Some(script.into()),
        max_words_per_segment: 5,
        word_timestamps: true,
        ..Default::default()
    };

    let segments = AcousticAligner::align_script_to_speech(&audio, script, &config);
    assert!(!segments.is_empty());

    // Verify word timing monotonic integrity
    for seg in &segments {
        assert!(seg.end_us > seg.start_us);
        for w in &seg.words {
            assert!(w.end_us >= w.start_us);
            assert!(w.start_us >= seg.start_us);
            assert!(w.end_us <= seg.end_us);
        }
    }

    // Verify script words are preserved across segments
    let reconstructed_text: String = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(reconstructed_text.contains("Welcome to Reddart"));
    assert!(reconstructed_text.contains("Zero latency"));
}

#[test]
fn test_subtitle_style_to_text_clip_typography_mapping() {
    let segments = vec![CaptionSegment {
        text: "GPU-accelerated typography".into(),
        start_us: 1_000_000,
        end_us: 3_500_000,
        words: vec![],
    }];

    let style = CaptionStyle {
        font_family: "Fira Code".into(),
        font_size: 46.0,
        color: [1.0, 0.95, 0.2, 1.0], // Golden yellow
        position_x: 0.5,
        position_y: 0.82,
        tracking: 0.08,
        leading: 1.45,
        outline_color: [0.05, 0.05, 0.05, 1.0],
        outline_width: 0.16,
        drop_shadow: true,
        shadow_color: [0.0, 0.0, 0.0, 0.85],
        shadow_offset: [0.005, 0.005],
        uppercase: true,
        max_chars_per_line: 38,
    };

    let (_tid, clips) = generate_caption_track(&segments, &style, "Subtitles");
    assert_eq!(clips.len(), 1);

    if let ClipKind::Text(ref tc) = clips[0].kind {
        assert_eq!(tc.text, "GPU-ACCELERATED TYPOGRAPHY");
        assert_eq!(tc.font_family, "Fira Code");
        assert_eq!(tc.font_size, 46.0);
        assert_eq!(tc.color, [1.0, 0.95, 0.2, 1.0]);
        assert_eq!(tc.position, [0.5, 0.82]);
        assert_eq!(tc.tracking, 0.08);
        assert_eq!(tc.leading, 1.45);
        assert_eq!(tc.outline_color, [0.05, 0.05, 0.05, 1.0]);
        assert_eq!(tc.outline_width, 0.16);
        assert!(tc.drop_shadow);
        assert_eq!(tc.shadow_color, [0.0, 0.0, 0.0, 0.85]);
        assert_eq!(tc.shadow_offset, [0.005, 0.005]);
    } else {
        panic!("Expected ClipKind::Text");
    }
}

#[test]
fn test_timeline_subtitle_track_injection_and_roundtrip_export() {
    let mut timeline = Timeline::new();
    let segments = vec![
        CaptionSegment {
            text: "Welcome to professional Reddart editing.".into(),
            start_us: 1_000_000,
            end_us: 3_000_000,
            words: vec![],
        },
        CaptionSegment {
            text: "Real-time timeline scrubbing with zero latency.".into(),
            start_us: 3_200_000,
            end_us: 6_000_000,
            words: vec![],
        },
    ];

    let style = CaptionStyle::default();
    let track_id = apply_captions_to_timeline(&mut timeline, &segments, &style, "Subtitles");

    assert_eq!(timeline.tracks().len(), 1);
    assert_eq!(timeline.tracks()[0].name, "Subtitles");

    // 1. Export timeline track to SubRip (.srt)
    let srt_export = export_timeline_subtitles_srt(&timeline, track_id).unwrap();
    assert!(srt_export.contains("1\n00:00:01,000 --> 00:00:03,000"));
    assert!(srt_export.contains("Welcome to professional Reddart editing."));
    assert!(srt_export.contains("2\n00:00:03,200 --> 00:00:06,000"));

    // 2. Parse back exported SRT and check parity
    let parsed_srt = parse_srt(&srt_export).expect("Failed to parse exported SRT");
    assert_eq!(parsed_srt.len(), 2);
    assert_eq!(parsed_srt[0].start_us, 1_000_000);
    assert_eq!(parsed_srt[0].end_us, 3_000_000);
    assert_eq!(
        parsed_srt[0].text,
        "Welcome to professional Reddart editing."
    );

    // 3. Export timeline track to WebVTT (.vtt)
    let vtt_export = export_timeline_subtitles_vtt(&timeline, track_id).unwrap();
    assert!(vtt_export.starts_with("WEBVTT\n\n"));
    assert!(vtt_export.contains("00:00:01.000 --> 00:00:03.000"));

    let parsed_vtt = parse_vtt(&vtt_export).expect("Failed to parse exported WebVTT");
    assert_eq!(parsed_vtt.len(), 2);
    assert_eq!(parsed_vtt[1].start_us, 3_200_000);
    assert_eq!(parsed_vtt[1].end_us, 6_000_000);
    assert_eq!(
        parsed_vtt[1].text,
        "Real-time timeline scrubbing with zero latency."
    );
}

#[test]
fn test_subtitle_reading_speed_cps_and_overlap_validation() {
    let mut segments = vec![
        CaptionSegment {
            text: "Short".into(),
            start_us: 1_000_000,
            end_us: 1_200_000, // 0.2s duration: too brief (< 0.3s)
            words: vec![],
        },
        CaptionSegment {
            text: "This caption sentence is exceptionally long and must be read at an impossibly high speed."
                .into(),
            start_us: 1_500_000,
            end_us: 2_500_000, // 1.0s for 89 chars -> 89 CPS!
            words: vec![],
        },
        CaptionSegment {
            text: "Overlapping following cue".into(),
            start_us: 2_300_000, // Starts at 2.3s, before 2.5s end of previous cue
            end_us: 4_000_000,
            words: vec![],
        },
        CaptionSegment {
            text: "Very long presentation duration that remains visible without changing for much too long on screen."
                .into(),
            start_us: 4_500_000,
            end_us: 13_000_000, // 8.5s duration (> 7.0s)
            words: vec![],
        },
    ];

    let issues = validate_caption_segments(&segments, 21.0);
    assert!(!issues.is_empty());

    // Check specific validation warnings and errors
    assert!(issues.iter().any(|i| i.message.contains("too brief")));
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("Reading speed exceeds threshold"))
    );
    assert!(
        issues
            .iter()
            .any(|i| i.severity == CaptionValidationSeverity::Error
                && i.message.contains("overlaps"))
    );
    assert!(issues.iter().any(|i| i.message.contains("too long")));

    // Resolve overlaps with 50 ms gap
    resolve_caption_overlaps(&mut segments, 50_000);
    assert!(segments[1].end_us <= segments[2].start_us - 50_000);
    assert_eq!(segments[1].end_us, 2_250_000);
}

#[test]
fn test_subtitle_html_sanitization() {
    let raw =
        "<font color=\"#ffeedd\"><b>Welcome</b></font> to <c.highlight>Reddart</c> <i>editor</i>!";
    let sanitized = sanitize_subtitle_text(raw);
    assert_eq!(sanitized, "Welcome to Reddart editor!");

    // Roundtrip through SRT parser should automatically sanitize text
    let srt_block =
        "1\n00:00:01,000 --> 00:00:03,000\n<v Speaker1><b>Action</b> movie subtitle</v>\n\n";
    let parsed = parse_srt(srt_block).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].text, "Action movie subtitle");
}

#[test]
fn test_sdf_text_rendering_compatibility_with_caption_style() {
    let width = 640;
    let height = 360;
    let mut canvas = vec![25u8; (width * height * 4) as usize]; // Dark gray background

    let style = CaptionStyle {
        font_family: "Inter".into(),
        font_size: 32.0,
        color: [1.0, 1.0, 1.0, 1.0], // White
        position_x: 0.5,
        position_y: 0.85,
        tracking: 0.0,
        leading: 1.35,
        outline_color: [0.0, 0.0, 0.0, 1.0], // Black outline
        outline_width: 0.15,
        drop_shadow: true,
        shadow_color: [0.0, 0.0, 0.0, 0.75],
        shadow_offset: [0.004, 0.004],
        uppercase: false,
        max_chars_per_line: 42,
    };

    let params = SdfTypographyParams {
        text: "Reddart AI Subtitles",
        pos_x: style.position_x,
        pos_y: style.position_y,
        font_size: style.font_size,
        tracking: style.tracking,
        leading: style.leading,
        alignment: rd_effects::text::TextAlignment::Center,
        color: style.color,
        outline_color: style.outline_color,
        outline_width: style.outline_width,
        drop_shadow: style.drop_shadow,
        shadow_color: style.shadow_color,
        shadow_offset: style.shadow_offset,
        smoothing: 0.04,
        shadow_smoothing: 0.08,
    };

    render_sdf_text_overlay(&mut canvas, width, height, &params);

    // Verify non-trivial pixel composition in the lower third
    let lower_third_start = ((height as f32 * 0.75) as u32 * width * 4) as usize;
    let lower_third_slice = &canvas[lower_third_start..];

    let has_white_text = lower_third_slice
        .chunks_exact(4)
        .any(|px| px[0] > 200 && px[1] > 200 && px[2] > 200);
    assert!(
        has_white_text,
        "Failed to rasterize white text glyphs in lower third"
    );

    let has_black_outline = lower_third_slice
        .chunks_exact(4)
        .any(|px| px[0] < 10 && px[1] < 10 && px[2] < 10 && px[3] > 100);
    assert!(
        has_black_outline,
        "Failed to rasterize black outline pixels"
    );
}
