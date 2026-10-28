//! Template engine & Beat Synchronization.
//!
//! Section W2.4 of the Technical Proposal:
//! JSON template specification: defines clip slots, effect parameters,
//! timing, and music cues. Instantiates full projects automatically.
//! Independent onset/beat detector (clean BSD-compatible algorithm, no GPL).

use anyhow::Result;
use rd_core::{Clip, ClipKind, DurationUs, Project, TimestampUs, TrackKind};
use serde::{Deserialize, Serialize};

/// Slot placeholder in a video template where user media is injected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSlot {
    pub slot_id: String,
    pub track_index: usize,
    pub start_us: TimestampUs,
    pub duration_us: DurationUs,
    pub effect_id: Option<String>,
}

/// Music cue definition with rhythm beat markers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicCue {
    pub audio_file: String,
    pub beats_us: Vec<TimestampUs>,
}

/// Declarative template specification matching template JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub aspect_width: u32,
    pub aspect_height: u32,
    pub duration_us: DurationUs,
    pub slots: Vec<TemplateSlot>,
    pub music_cue: Option<MusicCue>,
}

impl TemplateSpec {
    /// Instantiate a template into a full Project by injecting user media paths into the slots.
    pub fn instantiate(&self, media_paths: &[String]) -> Result<Project> {
        let mut project = Project::new(&self.name);
        project.resolution.width = self.aspect_width;
        project.resolution.height = self.aspect_height;

        let v_track = project.timeline.add_track(TrackKind::Video, "Video Main");

        for (idx, slot) in self.slots.iter().enumerate() {
            let media_path = media_paths
                .get(idx)
                .cloned()
                .unwrap_or_else(|| "placeholder.mp4".into());

            let mut clip = Clip::new_video(media_path, slot.start_us, 0, slot.duration_us);

            if let Some(eff_id) = &slot.effect_id {
                clip.effects.push(rd_core::ClipEffect {
                    effect_id: eff_id.clone(),
                    parameters: std::collections::HashMap::new(),
                    enabled: true,
                });
            }

            if let Some(track) = project.timeline.track_mut(v_track) {
                track.add_clip(clip);
            }
        }

        if let Some(music) = &self.music_cue {
            let a_track = project.timeline.add_track(TrackKind::Audio, "Music");
            let audio_clip = Clip {
                id: rd_core::ClipId::new(),
                kind: ClipKind::Audio(rd_core::AudioClip {
                    source_path: music.audio_file.clone(),
                    volume: 1.0,
                }),
                position_us: 0,
                source_in_us: 0,
                source_out_us: self.duration_us,
                speed: 1.0,
                opacity: 1.0,
                transform: rd_core::Transform2D::default(),
                effects: Vec::new(),
            };
            if let Some(track) = project.timeline.track_mut(a_track) {
                track.add_clip(audio_clip);
            }
        }

        Ok(project)
    }

    /// Curated starter template collection.
    pub fn starter_templates() -> Vec<Self> {
        vec![
            Self {
                id: "tiktok_reels_beat_sync".into(),
                name: "Reels / TikTok Beat Cut".into(),
                description: "Dynamic 9:16 vertical edit synced to rhythmic music cuts with directional wipes.".into(),
                aspect_width: 1080,
                aspect_height: 1920,
                duration_us: 10_000_000,
                slots: vec![
                    TemplateSlot { slot_id: "clip_1".into(), track_index: 0, start_us: 0, duration_us: 2_000_000, effect_id: None },
                    TemplateSlot { slot_id: "clip_2".into(), track_index: 0, start_us: 2_000_000, duration_us: 2_000_000, effect_id: Some("directional_wipe".into()) },
                    TemplateSlot { slot_id: "clip_3".into(), track_index: 0, start_us: 4_000_000, duration_us: 2_000_000, effect_id: None },
                    TemplateSlot { slot_id: "clip_4".into(), track_index: 0, start_us: 6_000_000, duration_us: 2_000_000, effect_id: Some("directional_wipe".into()) },
                    TemplateSlot { slot_id: "clip_5".into(), track_index: 0, start_us: 8_000_000, duration_us: 2_000_000, effect_id: None },
                ],
                music_cue: Some(MusicCue {
                    audio_file: "beat_track_upbeat.mp3".into(),
                    beats_us: vec![0, 2_000_000, 4_000_000, 6_000_000, 8_000_000],
                }),
            },
            Self {
                id: "cinematic_vlog_widescreen".into(),
                name: "Cinematic Vlog 16:9".into(),
                description: "Widescreen landscape vlog edit with warm sunlight grading and smooth dissolves.".into(),
                aspect_width: 1920,
                aspect_height: 1080,
                duration_us: 12_000_000,
                slots: vec![
                    TemplateSlot { slot_id: "intro".into(), track_index: 0, start_us: 0, duration_us: 4_000_000, effect_id: Some("brightness_contrast".into()) },
                    TemplateSlot { slot_id: "main_scene".into(), track_index: 0, start_us: 4_000_000, duration_us: 4_000_000, effect_id: Some("cross_dissolve".into()) },
                    TemplateSlot { slot_id: "outro".into(), track_index: 0, start_us: 8_000_000, duration_us: 4_000_000, effect_id: Some("cross_dissolve".into()) },
                ],
                music_cue: Some(MusicCue {
                    audio_file: "ambient_acoustic.mp3".into(),
                    beats_us: vec![0, 4_000_000, 8_000_000],
                }),
            },
            Self {
                id: "social_square_highlight".into(),
                name: "Social Square 1:1".into(),
                description: "Square 1:1 post with center focus shape mask and high contrast punch.".into(),
                aspect_width: 1080,
                aspect_height: 1080,
                duration_us: 8_000_000,
                slots: vec![
                    TemplateSlot { slot_id: "feature_1".into(), track_index: 0, start_us: 0, duration_us: 4_000_000, effect_id: Some("mask_shape".into()) },
                    TemplateSlot { slot_id: "feature_2".into(), track_index: 0, start_us: 4_000_000, duration_us: 4_000_000, effect_id: None },
                ],
                music_cue: None,
            },
        ]
    }
}

/// Simple energy-based audio onset detector (Beat Detector) without external GPL dependencies.
pub struct BeatDetector {
    sample_rate: u32,
    window_size: usize,
}

impl BeatDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            window_size: 1024,
        }
    }

    /// Detect rhythmic beat onsets from mono/stereo floating point PCM audio samples.
    /// Returns vector of timestamps in microseconds where beats occur.
    pub fn detect_onsets(&self, samples: &[f32], threshold_ratio: f32) -> Vec<TimestampUs> {
        let mut onsets = Vec::new();
        if samples.len() < self.window_size {
            return onsets;
        }

        let mut energies = Vec::new();
        let step = self.window_size / 2;

        let mut idx = 0;
        while idx + self.window_size <= samples.len() {
            let window = &samples[idx..idx + self.window_size];
            let energy: f32 = window.iter().map(|s| s * s).sum();
            energies.push((idx, energy));
            idx += step;
        }

        let mean_energy: f32 = energies.iter().map(|(_, e)| *e).sum::<f32>() / energies.len() as f32;
        let threshold = mean_energy * threshold_ratio;

        let mut last_beat_sample = 0;
        let min_sample_distance = (self.sample_rate as usize) / 4; // Max 4 beats/sec (240 BPM limit)

        for (sample_idx, energy) in energies {
            if energy > threshold && sample_idx.saturating_sub(last_beat_sample) > min_sample_distance {
                let timestamp_us = (sample_idx as i64 * 1_000_000) / self.sample_rate as i64;
                onsets.push(timestamp_us);
                last_beat_sample = sample_idx;
            }
        }

        onsets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_instantiation() {
        let template = TemplateSpec {
            id: "fast_cut_v1".into(),
            name: "Fast Cut Template".into(),
            description: "High energy fast cuts".into(),
            aspect_width: 1080,
            aspect_height: 1920,
            duration_us: 4_000_000,
            slots: vec![
                TemplateSlot {
                    slot_id: "s1".into(),
                    track_index: 0,
                    start_us: 0,
                    duration_us: 2_000_000,
                    effect_id: Some("brightness_contrast".into()),
                },
                TemplateSlot {
                    slot_id: "s2".into(),
                    track_index: 0,
                    start_us: 2_000_000,
                    duration_us: 2_000_000,
                    effect_id: None,
                },
            ],
            music_cue: Some(MusicCue {
                audio_file: "beat.wav".into(),
                beats_us: vec![0, 1_000_000, 2_000_000, 3_000_000],
            }),
        };

        let media = vec!["beach.mp4".into(), "mountain.mp4".into()];
        let project = template.instantiate(&media).expect("Instantiated project");

        assert_eq!(project.resolution.width, 1080);
        assert_eq!(project.resolution.height, 1920);
        assert_eq!(project.timeline.track_count(), 2); // 1 video + 1 audio
        assert_eq!(project.timeline.total_clip_count(), 3); // 2 video + 1 audio
    }

    #[test]
    fn test_starter_templates() {
        let templates = TemplateSpec::starter_templates();
        assert_eq!(templates.len(), 3);

        for t in &templates {
            let media = vec!["video1.mp4".into(), "video2.mp4".into(), "video3.mp4".into(), "video4.mp4".into(), "video5.mp4".into()];
            let project = t.instantiate(&media).expect("instantiate starter template");
            assert!(project.timeline.total_clip_count() >= 2);
            assert!(project.timeline.duration_us() > 0);
        }
    }

    #[test]
    fn test_beat_detector() {
        let detector = BeatDetector::new(48_000);
        let mut samples = vec![0.0_f32; 48_000 * 2]; // 2 seconds of audio

        // Create pulses at 0.5s, 1.0s, 1.5s
        for &sec in &[0.5, 1.0, 1.5] {
            let start = (sec * 48_000.0) as usize;
            for i in 0..500 {
                samples[start + i] = 0.9;
            }
        }

        let onsets = detector.detect_onsets(&samples, 1.5);
        assert_eq!(onsets.len(), 3);
        // Onsets must match closely with 0.5s, 1.0s, 1.5s
        assert!((onsets[0] - 500_000).abs() < 50_000);
        assert!((onsets[1] - 1_000_000).abs() < 50_000);
        assert!((onsets[2] - 1_500_000).abs() < 50_000);
    }
}
