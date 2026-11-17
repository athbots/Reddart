//! Auto captions generator with word-level timestamp alignment.
//!
//! Section W3.2 of the Technical Proposal:
//! Quantized on-device speech-to-text pipeline (Whisper class model).
//! Generates time-aligned subtitle TextClips directly onto a dedicated text track.
//! Supports exporting and importing industry standard SubRip (.srt) and WebVTT (.vtt) files.

use anyhow::{Context, Result, bail};
use rd_core::{Clip, ClipKind, Timeline, TimestampUs, TrackId, TrackKind};
use serde::{Deserialize, Serialize};

/// A single word with start and end timestamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_us: TimestampUs,
    pub end_us: TimestampUs,
}

/// A speech transcription segment containing words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionSegment {
    pub text: String,
    pub start_us: TimestampUs,
    pub end_us: TimestampUs,
    pub words: Vec<WordTiming>,
}

/// Caption generator styling and typography settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionStyle {
    pub font_family: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub position_x: f32, // 0.5 = center
    pub position_y: f32, // 0.85 = lower third
    pub tracking: f32,
    pub leading: f32,
    pub outline_color: [f32; 4],
    pub outline_width: f32,
    pub drop_shadow: bool,
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub uppercase: bool,
    pub max_chars_per_line: usize,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_family: "Inter".into(),
            font_size: 42.0,
            color: [1.0, 1.0, 1.0, 1.0], // White
            position_x: 0.5,
            position_y: 0.85,
            tracking: 0.0,
            leading: 1.35,
            outline_color: [0.0, 0.0, 0.0, 1.0],
            outline_width: 0.12,
            drop_shadow: true,
            shadow_color: [0.0, 0.0, 0.0, 0.8],
            shadow_offset: [0.003, 0.003],
            uppercase: false,
            max_chars_per_line: 42,
        }
    }
}

impl CaptionStyle {
    /// Create a new CaptionStyle with specified font family and size.
    pub fn new(font_family: impl Into<String>, font_size: f32) -> Self {
        Self {
            font_family: font_family.into(),
            font_size,
            ..Default::default()
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
        let text_content = if style.uppercase {
            seg.text.to_uppercase()
        } else {
            seg.text.clone()
        };

        let mut clip = Clip::new_text(&text_content, seg.start_us, duration_us);
        if let ClipKind::Text(ref mut tclip) = clip.kind {
            tclip.font_family = style.font_family.clone();
            tclip.font_size = style.font_size;
            tclip.color = style.color;
            tclip.position = [style.position_x, style.position_y];
            tclip.tracking = style.tracking;
            tclip.leading = style.leading;
            tclip.outline_color = style.outline_color;
            tclip.outline_width = style.outline_width;
            tclip.drop_shadow = style.drop_shadow;
            tclip.shadow_color = style.shadow_color;
            tclip.shadow_offset = style.shadow_offset;
        }

        clips.push(clip);
    }

    (track_id, clips)
}

/// Insert caption segments onto an active timeline text track, creating the track if needed.
pub fn apply_captions_to_timeline(
    timeline: &mut Timeline,
    segments: &[CaptionSegment],
    style: &CaptionStyle,
    track_name: &str,
) -> TrackId {
    let (_, clips) = generate_caption_track(segments, style, track_name);

    let track_id = if let Some(existing) = timeline
        .tracks()
        .iter()
        .find(|t| t.kind == TrackKind::Text && t.name == track_name)
    {
        existing.id
    } else {
        timeline.add_track(TrackKind::Text, track_name)
    };

    if let Some(track) = timeline.track_mut(track_id) {
        for clip in clips {
            track.add_clip(clip);
        }
    }

    track_id
}

/// Export subtitle clips from a specific timeline track as SubRip (.srt) format.
pub fn export_timeline_subtitles_srt(timeline: &Timeline, track_id: TrackId) -> Result<String> {
    let track = timeline
        .track(track_id)
        .context("Subtitle track ID not found on timeline")?;

    let mut segments = Vec::new();
    for clip in track.clips() {
        if let ClipKind::Text(ref tclip) = clip.kind {
            segments.push(CaptionSegment {
                text: tclip.text.clone(),
                start_us: clip.position_us,
                end_us: clip.position_us + clip.duration_us(),
                words: Vec::new(),
            });
        }
    }
    segments.sort_by_key(|s| s.start_us);
    Ok(export_srt(&segments))
}

/// Export subtitle clips from a specific timeline track as WebVTT (.vtt) format.
pub fn export_timeline_subtitles_vtt(timeline: &Timeline, track_id: TrackId) -> Result<String> {
    let track = timeline
        .track(track_id)
        .context("Subtitle track ID not found on timeline")?;

    let mut segments = Vec::new();
    for clip in track.clips() {
        if let ClipKind::Text(ref tclip) = clip.kind {
            segments.push(CaptionSegment {
                text: tclip.text.clone(),
                start_us: clip.position_us,
                end_us: clip.position_us + clip.duration_us(),
                words: Vec::new(),
            });
        }
    }
    segments.sort_by_key(|s| s.start_us);
    Ok(export_vtt(&segments))
}

/// Strip HTML tags (e.g. `<b>`, `<i>`, `<font ...>`, `<v ...>`) and normalize whitespace in subtitle text.
pub fn sanitize_subtitle_text(raw: &str) -> String {
    let mut clean = String::with_capacity(raw.len());
    let mut in_tag = false;

    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => clean.push(c),
            _ => {}
        }
    }

    let words: Vec<&str> = clean.split_whitespace().collect();
    words.join(" ")
}

/// Severity level of subtitle validation findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptionValidationSeverity {
    Warning,
    Error,
}

/// A validation finding on a subtitle segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionValidationIssue {
    pub segment_index: usize,
    pub severity: CaptionValidationSeverity,
    pub message: String,
}

/// Validate subtitle segments against readability limits, reading speed (CPS), and timeline overlaps.
pub fn validate_caption_segments(
    segments: &[CaptionSegment],
    max_cps: f32,
) -> Vec<CaptionValidationIssue> {
    let mut issues = Vec::new();

    for (idx, seg) in segments.iter().enumerate() {
        let duration_s = (seg.end_us - seg.start_us) as f32 / 1_000_000.0;
        if seg.start_us >= seg.end_us || duration_s <= 0.0 {
            issues.push(CaptionValidationIssue {
                segment_index: idx,
                severity: CaptionValidationSeverity::Error,
                message: format!(
                    "Invalid timecode: start ({}) >= end ({})",
                    format_timestamp_srt(seg.start_us),
                    format_timestamp_srt(seg.end_us)
                ),
            });
            continue;
        }

        if duration_s < 0.3 {
            issues.push(CaptionValidationIssue {
                segment_index: idx,
                severity: CaptionValidationSeverity::Warning,
                message: format!("Segment duration too brief: {:.2}s (< 0.3s)", duration_s),
            });
        } else if duration_s > 7.0 {
            issues.push(CaptionValidationIssue {
                segment_index: idx,
                severity: CaptionValidationSeverity::Warning,
                message: format!("Segment duration too long: {:.2}s (> 7.0s)", duration_s),
            });
        }

        let char_count = seg.text.trim().chars().count();
        if duration_s > 0.0 {
            let cps = char_count as f32 / duration_s;
            if cps > max_cps {
                issues.push(CaptionValidationIssue {
                    segment_index: idx,
                    severity: CaptionValidationSeverity::Warning,
                    message: format!(
                        "Reading speed exceeds threshold: {:.1} CPS (limit: {:.1} CPS)",
                        cps, max_cps
                    ),
                });
            }
        }

        // Check for overlaps with following segment
        if idx + 1 < segments.len() {
            let next = &segments[idx + 1];
            if seg.end_us > next.start_us {
                issues.push(CaptionValidationIssue {
                    segment_index: idx,
                    severity: CaptionValidationSeverity::Error,
                    message: format!(
                        "Segment overlaps with subsequent segment: end {} > next start {}",
                        format_timestamp_srt(seg.end_us),
                        format_timestamp_srt(next.start_us)
                    ),
                });
            }
        }
    }

    issues
}

/// Automatically adjust segment end timestamps so that adjacent segments have at least `min_gap_us` spacing.
pub fn resolve_caption_overlaps(segments: &mut [CaptionSegment], min_gap_us: TimestampUs) {
    if segments.len() <= 1 {
        return;
    }

    for i in 0..segments.len() - 1 {
        let next_start = segments[i + 1].start_us;
        let allowed_end = next_start.saturating_sub(min_gap_us);
        if segments[i].end_us > allowed_end && allowed_end > segments[i].start_us {
            segments[i].end_us = allowed_end;
        }
    }
}

/// Format a microsecond timestamp as SubRip SRT format: `HH:MM:SS,mmm`
pub fn format_timestamp_srt(us: TimestampUs) -> String {
    let total_ms = (us.max(0) / 1_000) as u64;
    let ms = total_ms % 1_000;
    let total_s = total_ms / 1_000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

/// Format a microsecond timestamp as WebVTT format: `HH:MM:SS.mmm`
pub fn format_timestamp_vtt(us: TimestampUs) -> String {
    let total_ms = (us.max(0) / 1_000) as u64;
    let ms = total_ms % 1_000;
    let total_s = total_ms / 1_000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

/// Parse a SubRip timestamp string (`HH:MM:SS,mmm`) into microseconds.
pub fn parse_timestamp_srt(s: &str) -> Option<TimestampUs> {
    let parts: Vec<&str> = s.trim().split(&[':', ','][..]).collect();
    if parts.len() != 4 {
        return None;
    }

    let h: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let s: i64 = parts[2].parse().ok()?;
    let ms: i64 = parts[3].parse().ok()?;

    let total_ms = (h * 3600 + m * 60 + s) * 1000 + ms;
    Some(total_ms * 1_000)
}

/// Parse a WebVTT timestamp string (`HH:MM:SS.mmm` or `MM:SS.mmm`) into microseconds.
pub fn parse_timestamp_vtt(s: &str) -> Option<TimestampUs> {
    let parts: Vec<&str> = s.trim().split(&[':', '.'][..]).collect();
    if parts.len() == 4 {
        let h: i64 = parts[0].parse().ok()?;
        let m: i64 = parts[1].parse().ok()?;
        let s: i64 = parts[2].parse().ok()?;
        let ms: i64 = parts[3].parse().ok()?;
        let total_ms = (h * 3600 + m * 60 + s) * 1000 + ms;
        Some(total_ms * 1_000)
    } else if parts.len() == 3 {
        let m: i64 = parts[0].parse().ok()?;
        let s: i64 = parts[1].parse().ok()?;
        let ms: i64 = parts[2].parse().ok()?;
        let total_ms = (m * 60 + s) * 1000 + ms;
        Some(total_ms * 1_000)
    } else {
        None
    }
}

/// Export caption segments to SubRip (.srt) subtitle format.
pub fn export_srt(segments: &[CaptionSegment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let index = i + 1;
        let start = format_timestamp_srt(seg.start_us);
        let end = format_timestamp_srt(seg.end_us);

        out.push_str(&format!(
            "{index}\n{start} --> {end}\n{}\n\n",
            seg.text.trim()
        ));
    }
    out
}

/// Export caption segments to WebVTT (.vtt) format.
pub fn export_vtt(segments: &[CaptionSegment]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (i, seg) in segments.iter().enumerate() {
        let start = format_timestamp_vtt(seg.start_us);
        let end = format_timestamp_vtt(seg.end_us);

        out.push_str(&format!(
            "{}\n{start} --> {end}\n{}\n\n",
            i + 1,
            seg.text.trim()
        ));
    }
    out
}

/// Parse a SubRip (.srt) formatted subtitle string into structured `CaptionSegment` items.
pub fn parse_srt(content: &str) -> Result<Vec<CaptionSegment>> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let blocks: Vec<&str> = normalized.split("\n\n").collect();
    let mut segments = Vec::new();

    for block in blocks {
        let lines: Vec<&str> = block
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.len() < 2 {
            continue;
        }

        // Determine if first line is index number or timecode
        let (time_line_idx, _idx_str) = if lines[0].contains("-->") {
            (0, None)
        } else {
            (1, Some(lines[0]))
        };

        if time_line_idx >= lines.len() {
            continue;
        }

        let time_line = lines[time_line_idx];
        let parts: Vec<&str> = time_line.split("-->").collect();
        if parts.len() != 2 {
            continue;
        }

        let start_us = parse_timestamp_srt(parts[0])
            .with_context(|| format!("Invalid SRT start timestamp: {}", parts[0]))?;
        let end_us = parse_timestamp_srt(parts[1])
            .with_context(|| format!("Invalid SRT end timestamp: {}", parts[1]))?;

        let text_lines = &lines[time_line_idx + 1..];
        let text = sanitize_subtitle_text(&text_lines.join(" "));

        segments.push(CaptionSegment {
            text,
            start_us,
            end_us,
            words: Vec::new(),
        });
    }

    Ok(segments)
}

/// Parse a WebVTT (.vtt) formatted subtitle string into structured `CaptionSegment` items.
pub fn parse_vtt(content: &str) -> Result<Vec<CaptionSegment>> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized.lines().map(|l| l.trim());

    // Validate WEBVTT header
    let first = lines.next().unwrap_or("").trim();
    if !first.starts_with("WEBVTT") {
        bail!("Missing WEBVTT header");
    }

    let mut segments = Vec::new();
    let blocks: Vec<&str> = normalized.split("\n\n").collect();

    for block in blocks {
        let b_lines: Vec<&str> = block
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if b_lines.is_empty() {
            continue;
        }

        let mut time_line_opt = None;
        let mut text_start = 0;

        for (idx, line) in b_lines.iter().enumerate() {
            if line.contains("-->") {
                time_line_opt = Some(*line);
                text_start = idx + 1;
                break;
            }
        }

        if let Some(time_line) = time_line_opt {
            let parts: Vec<&str> = time_line.split("-->").collect();
            if parts.len() >= 2 {
                let start_part = parts[0].trim();
                // End part may contain VTT cue settings (e.g. `align:center`)
                let end_part = parts[1].split_whitespace().next().unwrap_or("");

                if let (Some(start_us), Some(end_us)) = (
                    parse_timestamp_vtt(start_part),
                    parse_timestamp_vtt(end_part),
                ) {
                    let text = sanitize_subtitle_text(&b_lines[text_start..].join(" "));
                    segments.push(CaptionSegment {
                        text,
                        start_us,
                        end_us,
                        words: Vec::new(),
                    });
                }
            }
        }
    }

    Ok(segments)
}

/// Wrap long subtitle segments into shorter lines based on maximum word count and maximum duration.
pub fn wrap_segments(
    segments: &[CaptionSegment],
    max_words_per_segment: usize,
    max_duration_us: TimestampUs,
) -> Vec<CaptionSegment> {
    if max_words_per_segment == 0 {
        return segments.to_vec();
    }

    let mut wrapped = Vec::new();

    for seg in segments {
        if seg.words.is_empty() {
            // No word timings: split text naively
            let words: Vec<&str> = seg.text.split_whitespace().collect();
            if words.len() <= max_words_per_segment {
                wrapped.push(seg.clone());
            } else {
                let chunk_count = words.len().div_ceil(max_words_per_segment);
                let duration_chunk = (seg.end_us - seg.start_us) / chunk_count as i64;

                for (c_idx, chunk) in words.chunks(max_words_per_segment).enumerate() {
                    let start = seg.start_us + (c_idx as i64 * duration_chunk);
                    let end = if c_idx == chunk_count - 1 {
                        seg.end_us
                    } else {
                        start + duration_chunk
                    };
                    wrapped.push(CaptionSegment {
                        text: chunk.join(" "),
                        start_us: start,
                        end_us: end,
                        words: Vec::new(),
                    });
                }
            }
        } else {
            // Split respecting word timestamps
            let mut cur_words: Vec<WordTiming> = Vec::new();
            let mut cur_start = seg.words[0].start_us;

            for word in &seg.words {
                let would_exceed_duration = word.end_us - cur_start > max_duration_us;
                let would_exceed_words = cur_words.len() >= max_words_per_segment;

                if !cur_words.is_empty() && (would_exceed_duration || would_exceed_words) {
                    let last_end = cur_words.last().unwrap().end_us;
                    let text = cur_words
                        .iter()
                        .map(|w: &WordTiming| w.word.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");

                    wrapped.push(CaptionSegment {
                        text,
                        start_us: cur_start,
                        end_us: last_end,
                        words: std::mem::take(&mut cur_words),
                    });

                    cur_start = word.start_us;
                }

                cur_words.push(word.clone());
            }

            if !cur_words.is_empty() {
                let last_end = cur_words.last().unwrap().end_us;
                let text = cur_words
                    .iter()
                    .map(|w| w.word.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                wrapped.push(CaptionSegment {
                    text,
                    start_us: cur_start,
                    end_us: last_end,
                    words: cur_words,
                });
            }
        }
    }

    wrapped
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

    #[test]
    fn test_timestamp_formatting_and_parsing_srt() {
        let us = 3_723_456_000; // 1h 2m 3s 456ms
        let formatted = format_timestamp_srt(us);
        assert_eq!(formatted, "01:02:03,456");

        let parsed = parse_timestamp_srt(&formatted).expect("Failed to parse SRT timestamp");
        assert_eq!(parsed, us);
    }

    #[test]
    fn test_timestamp_formatting_and_parsing_vtt() {
        let us = 3_723_456_000; // 1h 2m 3s 456ms
        let formatted = format_timestamp_vtt(us);
        assert_eq!(formatted, "01:02:03.456");

        let parsed = parse_timestamp_vtt(&formatted).expect("Failed to parse VTT timestamp");
        assert_eq!(parsed, us);

        // Also test MM:SS.mmm parsing
        let short_parsed = parse_timestamp_vtt("02:03.456").unwrap();
        assert_eq!(short_parsed, (2 * 60 + 3) * 1_000_000 + 456_000);
    }

    #[test]
    fn test_srt_roundtrip() {
        let segments = vec![
            CaptionSegment {
                text: "First subtitle line".into(),
                start_us: 1_000_000,
                end_us: 3_000_000,
                words: vec![],
            },
            CaptionSegment {
                text: "Second subtitle line".into(),
                start_us: 3_500_000,
                end_us: 5_000_000,
                words: vec![],
            },
        ];

        let srt_text = export_srt(&segments);
        assert!(srt_text.contains("00:00:01,000 --> 00:00:03,000"));
        assert!(srt_text.contains("First subtitle line"));

        let parsed = parse_srt(&srt_text).expect("Failed to parse SRT export");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].text, "First subtitle line");
        assert_eq!(parsed[0].start_us, 1_000_000);
        assert_eq!(parsed[0].end_us, 3_000_000);
        assert_eq!(parsed[1].text, "Second subtitle line");
        assert_eq!(parsed[1].start_us, 3_500_000);
        assert_eq!(parsed[1].end_us, 5_000_000);
    }

    #[test]
    fn test_vtt_roundtrip() {
        let segments = vec![
            CaptionSegment {
                text: "Welcome to Reddart".into(),
                start_us: 500_000,
                end_us: 2_000_000,
                words: vec![],
            },
            CaptionSegment {
                text: "GPU-accelerated desktop editor".into(),
                start_us: 2_500_000,
                end_us: 5_000_000,
                words: vec![],
            },
        ];

        let vtt_text = export_vtt(&segments);
        assert!(vtt_text.starts_with("WEBVTT"));
        assert!(vtt_text.contains("00:00:00.500 --> 00:00:02.000"));

        let parsed = parse_vtt(&vtt_text).expect("Failed to parse VTT export");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].text, "Welcome to Reddart");
        assert_eq!(parsed[0].start_us, 500_000);
        assert_eq!(parsed[0].end_us, 2_000_000);
    }

    #[test]
    fn test_wrap_segments_with_word_timings() {
        let words = vec![
            WordTiming {
                word: "One".into(),
                start_us: 0,
                end_us: 200_000,
            },
            WordTiming {
                word: "Two".into(),
                start_us: 250_000,
                end_us: 450_000,
            },
            WordTiming {
                word: "Three".into(),
                start_us: 500_000,
                end_us: 700_000,
            },
            WordTiming {
                word: "Four".into(),
                start_us: 750_000,
                end_us: 950_000,
            },
        ];

        let segment = CaptionSegment {
            text: "One Two Three Four".into(),
            start_us: 0,
            end_us: 950_000,
            words,
        };

        // Wrap into chunks of max 2 words
        let wrapped = wrap_segments(&[segment], 2, 5_000_000);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0].text, "One Two");
        assert_eq!(wrapped[0].start_us, 0);
        assert_eq!(wrapped[0].end_us, 450_000);
        assert_eq!(wrapped[1].text, "Three Four");
        assert_eq!(wrapped[1].start_us, 500_000);
        assert_eq!(wrapped[1].end_us, 950_000);
    }

    #[test]
    fn test_timeline_subtitles_apply_and_export() {
        let mut timeline = Timeline::new();
        let segments = vec![
            CaptionSegment {
                text: "Cinematic 4K grading".into(),
                start_us: 1_000_000,
                end_us: 3_000_000,
                words: vec![],
            },
            CaptionSegment {
                text: "Real-time AI captions".into(),
                start_us: 3_500_000,
                end_us: 5_500_000,
                words: vec![],
            },
        ];

        let style = CaptionStyle {
            font_family: "Roboto".into(),
            font_size: 44.0,
            color: [1.0, 1.0, 1.0, 1.0],
            position_x: 0.5,
            position_y: 0.85,
            tracking: 0.05,
            leading: 1.4,
            outline_color: [0.0, 0.0, 0.0, 1.0],
            outline_width: 0.15,
            drop_shadow: true,
            shadow_color: [0.0, 0.0, 0.0, 0.75],
            shadow_offset: [0.004, 0.004],
            uppercase: true,
            max_chars_per_line: 40,
        };

        let track_id =
            apply_captions_to_timeline(&mut timeline, &segments, &style, "Subtitles Track");
        assert_eq!(timeline.tracks().len(), 1);

        let track = timeline.track(track_id).unwrap();
        assert_eq!(track.clips().len(), 2);
        if let ClipKind::Text(ref tc) = track.clips()[0].kind {
            assert_eq!(tc.text, "CINEMATIC 4K GRADING");
            assert_eq!(tc.font_family, "Roboto");
            assert_eq!(tc.outline_width, 0.15);
            assert!(tc.drop_shadow);
        } else {
            panic!("Expected ClipKind::Text");
        }

        // Export roundtrip back to SRT and VTT
        let srt_out = export_timeline_subtitles_srt(&timeline, track_id).unwrap();
        assert!(srt_out.contains("00:00:01,000 --> 00:00:03,000"));
        assert!(srt_out.contains("CINEMATIC 4K GRADING"));

        let vtt_out = export_timeline_subtitles_vtt(&timeline, track_id).unwrap();
        assert!(vtt_out.starts_with("WEBVTT"));
        assert!(vtt_out.contains("REAL-TIME AI CAPTIONS"));
    }

    #[test]
    fn test_sanitize_subtitle_text() {
        let dirty = "  <i>Hello</i>, <font color=\"#ff0000\"><b>world</b></font>!  ";
        let clean = sanitize_subtitle_text(dirty);
        assert_eq!(clean, "Hello, world!");
    }

    #[test]
    fn test_caption_validation_and_overlap_resolution() {
        let mut segments = vec![
            CaptionSegment {
                text:
                    "Rapid high reading speed text that exceeds twenty characters per second easily"
                        .into(),
                start_us: 1_000_000,
                end_us: 2_000_000, // 1.0s for ~78 chars -> 78 CPS!
                words: vec![],
            },
            CaptionSegment {
                text: "Overlapping segment".into(),
                start_us: 1_800_000, // Starts before segment 0 ends!
                end_us: 3_000_000,
                words: vec![],
            },
        ];

        let issues = validate_caption_segments(&segments, 20.0);
        assert!(!issues.is_empty());
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

        // Resolve overlaps
        resolve_caption_overlaps(&mut segments, 50_000);
        assert!(segments[0].end_us <= segments[1].start_us - 50_000);
    }
}
