//! Integration tests for on-device Whisper speech-to-text, VAD, and subtitle tracks.

use rd_core::{ClipKind, TimestampUs};
use rd_ml::captions::{
    CaptionSegment, CaptionStyle, WordTiming, export_srt, export_vtt, generate_caption_track,
    parse_srt, parse_vtt, wrap_segments,
};
use rd_ml::whisper::{
    AudioPreprocessor, VoiceActivityDetector, WHISPER_SAMPLE_RATE, WhisperConfig, WhisperEngine,
    WhisperModelSize,
};

#[test]
fn test_end_to_end_whisper_vad_srt_and_vtt_pipeline() {
    // 1. Generate 3 seconds of synthetic 48 kHz stereo audio
    // 0.0s - 0.4s: silence
    // 0.4s - 1.2s: Speech 1
    // 1.2s - 1.8s: silence
    // 1.8s - 2.7s: Speech 2
    // 2.7s - 3.0s: silence
    let sample_rate = 48_000;
    let total_frames = sample_rate * 3;
    let mut pcm_48k_stereo = vec![0.0f32; total_frames * 2];

    for i in 0..total_frames {
        let in_speech1 = (sample_rate * 4 / 10..sample_rate * 12 / 10).contains(&i);
        let in_speech2 = (sample_rate * 18 / 10..sample_rate * 27 / 10).contains(&i);

        if in_speech1 || in_speech2 {
            let val = 0.7 * (i as f32 / 15.0).sin();
            pcm_48k_stereo[i * 2] = val;
            pcm_48k_stereo[i * 2 + 1] = val;
        }
    }

    // 2. Preprocess to 16 kHz mono
    let mono_16k = AudioPreprocessor::resample_to_16k_mono(&pcm_48k_stereo, 48_000, 2);
    assert_eq!(mono_16k.len(), WHISPER_SAMPLE_RATE as usize * 3);

    // 3. Run Voice Activity Detection
    let boundaries = VoiceActivityDetector::detect_speech(&mono_16k, 0.05, 200);
    assert_eq!(boundaries.len(), 2);
    assert!(boundaries[0].start_us >= 350_000 && boundaries[0].start_us <= 450_000);
    assert!(boundaries[1].start_us >= 1_750_000 && boundaries[1].start_us <= 1_850_000);

    // 4. Transcribe with WhisperEngine
    let config = WhisperConfig {
        model_size: WhisperModelSize::Tiny,
        word_timestamps: true,
        ..Default::default()
    };
    let engine = WhisperEngine::new(config);
    let segments = engine.transcribe_pcm(&pcm_48k_stereo, 48_000, 2).unwrap();
    assert_eq!(segments.len(), 2);
    assert!(!segments[0].words.is_empty());

    // 5. Export to SubRip (.srt) and verify roundtrip
    let srt_output = export_srt(&segments);
    assert!(srt_output.contains("1\n"));
    assert!(srt_output.contains("-->"));
    assert!(srt_output.contains("Speech segment 1"));

    let parsed_srt = parse_srt(&srt_output).expect("Failed to parse SRT");
    assert_eq!(parsed_srt.len(), 2);
    assert_eq!(parsed_srt[0].start_us, segments[0].start_us);
    assert_eq!(parsed_srt[0].end_us, segments[0].end_us);

    // 6. Export to WebVTT (.vtt) and verify roundtrip
    let vtt_output = export_vtt(&segments);
    assert!(vtt_output.starts_with("WEBVTT\n\n"));
    assert!(vtt_output.contains("Speech segment 2"));

    let parsed_vtt = parse_vtt(&vtt_output).expect("Failed to parse WebVTT");
    assert_eq!(parsed_vtt.len(), 2);
    assert_eq!(parsed_vtt[1].start_us, segments[1].start_us);
    assert_eq!(parsed_vtt[1].end_us, segments[1].end_us);
}

#[test]
fn test_whisper_subtitles_to_timeline_text_clips() {
    let segments = vec![
        CaptionSegment {
            text: "Reddart empowers creative freedom".into(),
            start_us: 1_000_000,
            end_us: 3_500_000,
            words: vec![
                WordTiming {
                    word: "Reddart".into(),
                    start_us: 1_000_000,
                    end_us: 1_500_000,
                },
                WordTiming {
                    word: "empowers".into(),
                    start_us: 1_550_000,
                    end_us: 2_100_000,
                },
                WordTiming {
                    word: "creative".into(),
                    start_us: 2_150_000,
                    end_us: 2_800_000,
                },
                WordTiming {
                    word: "freedom".into(),
                    start_us: 2_850_000,
                    end_us: 3_500_000,
                },
            ],
        },
        CaptionSegment {
            text: "Deterministic 60 FPS playback".into(),
            start_us: 4_000_000,
            end_us: 6_000_000,
            words: vec![],
        },
    ];

    let style = CaptionStyle {
        font_family: "Montserrat".into(),
        font_size: 48.0,
        color: [1.0, 0.9, 0.2, 1.0], // Yellow highlight
        position_y: 0.80,
    };

    let (track_id, clips) = generate_caption_track(&segments, &style, "Subtitles");
    assert_eq!(track_id.0, 100);
    assert_eq!(clips.len(), 2);

    // Verify Clip 1 text properties
    assert_eq!(clips[0].position_us, 1_000_000);
    assert_eq!(clips[0].duration_us(), 2_500_000);
    if let ClipKind::Text(ref text_clip) = clips[0].kind {
        assert_eq!(text_clip.text, "Reddart empowers creative freedom");
        assert_eq!(text_clip.font_family, "Montserrat");
        assert_eq!(text_clip.font_size, 48.0);
        assert_eq!(text_clip.color, [1.0, 0.9, 0.2, 1.0]);
        assert_eq!(text_clip.position, [0.5, 0.80]);
    } else {
        panic!("Expected ClipKind::Text");
    }

    // Verify Clip 2 text properties
    assert_eq!(clips[1].position_us, 4_000_000);
    assert_eq!(clips[1].duration_us(), 2_000_000);
    if let ClipKind::Text(ref text_clip) = clips[1].kind {
        assert_eq!(text_clip.text, "Deterministic 60 FPS playback");
    } else {
        panic!("Expected ClipKind::Text");
    }
}

#[test]
fn test_whisper_model_config_serialization() {
    let config = WhisperConfig {
        model_size: WhisperModelSize::Small,
        model_path: None,
        language: Some("es".into()),
        translate: true,
        word_timestamps: true,
        temperature: 0.2,
        vad_filter: true,
        vad_threshold: 0.02,
        min_speech_duration_ms: 300,
        max_segment_duration_s: 25.0,
    };

    let serialized = serde_json::to_string(&config).expect("Serialization failed");
    let deserialized: WhisperConfig =
        serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(deserialized.model_size, WhisperModelSize::Small);
    assert_eq!(deserialized.language.as_deref(), Some("es"));
    assert!(deserialized.translate);
    assert_eq!(deserialized.temperature, 0.2);
    assert_eq!(deserialized.min_speech_duration_ms, 300);
}

#[test]
fn test_wrap_captions_for_social_media_reels() {
    let sentence = CaptionSegment {
        text: "Make every frame count with zero latency editing".into(),
        start_us: 1_000_000,
        end_us: 5_000_000,
        words: vec![
            WordTiming {
                word: "Make".into(),
                start_us: 1_000_000,
                end_us: 1_400_000,
            },
            WordTiming {
                word: "every".into(),
                start_us: 1_450_000,
                end_us: 1_900_000,
            },
            WordTiming {
                word: "frame".into(),
                start_us: 1_950_000,
                end_us: 2_400_000,
            },
            WordTiming {
                word: "count".into(),
                start_us: 2_450_000,
                end_us: 2_900_000,
            },
            WordTiming {
                word: "with".into(),
                start_us: 2_950_000,
                end_us: 3_300_000,
            },
            WordTiming {
                word: "zero".into(),
                start_us: 3_350_000,
                end_us: 3_800_000,
            },
            WordTiming {
                word: "latency".into(),
                start_us: 3_850_000,
                end_us: 4_400_000,
            },
            WordTiming {
                word: "editing".into(),
                start_us: 4_450_000,
                end_us: 5_000_000,
            },
        ],
    };

    // Wrap into max 3 words per subtitle chunk
    let wrapped = wrap_segments(&[sentence], 3, 3_000_000 as TimestampUs);
    assert_eq!(wrapped.len(), 3);
    assert_eq!(wrapped[0].text, "Make every frame");
    assert_eq!(wrapped[0].start_us, 1_000_000);
    assert_eq!(wrapped[0].end_us, 2_400_000);

    assert_eq!(wrapped[1].text, "count with zero");
    assert_eq!(wrapped[1].start_us, 2_450_000);
    assert_eq!(wrapped[1].end_us, 3_800_000);

    assert_eq!(wrapped[2].text, "latency editing");
    assert_eq!(wrapped[2].start_us, 3_850_000);
    assert_eq!(wrapped[2].end_us, 5_000_000);
}
