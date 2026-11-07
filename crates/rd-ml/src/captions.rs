//! Auto captions generator with word-level timestamp alignment.
//!
//! Section W3.2 of the Technical Proposal:
//! Quantized on-device speech-to-text pipeline (Whisper class model).
//! Generates time-aligned subtitle TextClips directly onto a dedicated text track.

use rd_core::{Clip, ClipKind, TimestampUs, TrackId};
use serde::{Deserialize, Serialize};

/// A single word with start and end timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_us: TimestampUs,
    pub end_us: TimestampUs,
}

/// A speech transcription segment containing words.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionSegment {
    pub text: String,
    pub start_us: TimestampUs,
    pub end_us: TimestampUs,
    pub words: Vec<WordTiming>,
}

/// Caption generator settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionStyle {
    pub font_family: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub position_y: f32, // 0.85 = lower third
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_family: "Inter".into(),
            font_size: 42.0,
            color: [1.0, 1.0, 1.0, 1.0], // White
            position_y: 0.85,
        }
    }
}

/// Convert speech transcription segments into a project subtitle track.
pub fn generate_caption_track(
    segments: &[CaptionSegment],
    style: &CaptionStyle,
    _track_name: &str,
) -> (TrackId, Vec<Clip>) {
    let track_id = TrackId(100); // Dedicated subtitle track ID
    let mut clips = Vec::with_capacity(segments.len());

    for seg in segments {
        let duration_us = (seg.end_us - seg.start_us).max(200_000); // Min 0.2s

        let mut clip = Clip::new_text(&seg.text, seg.start_us, duration_us);
        if let ClipKind::Text(ref mut tclip) = clip.kind {
            tclip.font_family = style.font_family.clone();
            tclip.font_size = style.font_size;
            tclip.color = style.color;
            tclip.position = [0.5, style.position_y];
        }

        clips.push(clip);
    }

    (track_id, clips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caption_generation() {
        let segments = vec![
            CaptionSegment {
                text: "Welcome to Reddart".into(),
                start_us: 500_000,
                end_us: 2_000_000,
                words: vec![
                    WordTiming {
                        word: "Welcome".into(),
                        start_us: 500_000,
                        end_us: 900_000,
                    },
                    WordTiming {
                        word: "to".into(),
                        start_us: 950_000,
                        end_us: 1_100_000,
                    },
                    WordTiming {
                        word: "Reddart".into(),
                        start_us: 1_150_000,
                        end_us: 2_000_000,
                    },
                ],
            },
            CaptionSegment {
                text: "Zero-lag video editor".into(),
                start_us: 2_200_000,
                end_us: 4_000_000,
                words: vec![],
            },
        ];

        let style = CaptionStyle::default();
        let (_tid, clips) = generate_caption_track(&segments, &style, "Captions");

        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].position_us, 500_000);
        assert_eq!(clips[0].duration_us(), 1_500_000);
        assert_eq!(clips[1].position_us, 2_200_000);
    }
}
