//! Timeline Audio Scheduler and Mixer: connects arbitrary project audio clips across
//! all enabled timeline tracks to the real-time CPAL audio engine.
//!
//! Technical Proposal Section 3.5:
//! - Pre-caches decoded 48 kHz stereo floating-point PCM audio in memory.
//! - Renders timeline track audio schedules into a continuous playback buffer.
//! - Accurately applies clip volume, speed multipliers, panning, keyframed envelopes,
//!   track mute toggles, and solo overrides.

use std::collections::HashMap;
use std::f32::consts::{FRAC_1_SQRT_2, SQRT_2};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use rd_core::{ClipKind, Project, TrackId};

use crate::decode::decode_audio_to_pcm;
use crate::engine::AudioEngine;
use crate::spatial::{ChannelLayout, ItuDownmixer, SpatialPanner, SpatialParams};

/// Scheduler that manages project timeline audio clips and synchronizes them
/// into the low-latency hardware audio engine.
#[derive(Default)]
pub struct TimelineAudioScheduler {
    pcm_cache: HashMap<PathBuf, Arc<Vec<f32>>>,
}

impl TimelineAudioScheduler {
    /// Create a new timeline audio scheduler with an empty cache.
    pub fn new() -> Self {
        Self {
            pcm_cache: HashMap::new(),
        }
    }

    /// Resolve a media path against project workspace folders.
    pub fn resolve_path(raw_path: &str) -> Option<PathBuf> {
        let p = Path::new(raw_path);
        if p.exists() {
            return Some(p.to_path_buf());
        }

        let candidate_dirs = [
            Path::new("ready_to_use/windows/samples"),
            Path::new("ready_to_use/windows"),
            Path::new("samples"),
            Path::new("."),
        ];

        for dir in candidate_dirs {
            let candidate = dir.join(raw_path);
            if candidate.exists() {
                return Some(candidate);
            }
            if let Some(file_name) = p.file_name() {
                let by_name = dir.join(file_name);
                if by_name.exists() {
                    return Some(by_name);
                }
                let in_audio = dir.join("audio").join(file_name);
                if in_audio.exists() {
                    return Some(in_audio);
                }
                let in_footage = dir.join("footage").join(file_name);
                if in_footage.exists() {
                    return Some(in_footage);
                }
            }
        }

        None
    }

    /// Synchronize the AudioEngine mixer with the current state of the project timeline.
    ///
    /// Evaluates all clips on enabled tracks, applying track mute and solo states,
    /// clip in/out boundaries, speed adjustments, panning, and keyframed volume envelopes.
    pub fn sync_timeline(
        &mut self,
        engine: &AudioEngine,
        project: &Project,
        solo_track: Option<TrackId>,
    ) -> Result<()> {
        let duration_us = project.timeline.duration_us();
        let duration_s = (duration_us as f64 / 1_000_000.0).max(0.1);
        let total_sample_frames = (duration_s * 48000.0).round() as usize;
        let mut master_pcm = vec![0.0f32; total_sample_frames * 2];

        for track in project.timeline.tracks() {
            // Track solo override takes priority; otherwise respect track.enabled
            if let Some(solo_id) = solo_track {
                if track.id != solo_id {
                    continue;
                }
            } else if !track.enabled {
                continue;
            }

            for clip in track.clips() {
                let audio_path = match &clip.kind {
                    ClipKind::Audio(aclip) => Self::resolve_path(&aclip.source_path),
                    ClipKind::Video(vclip) if vclip.has_audio => {
                        Self::resolve_path(&vclip.source_path)
                    }
                    _ => None,
                };

                let path = match audio_path {
                    Some(p) => p,
                    None => continue,
                };

                // Retrieve from memory cache or decode from file
                let pcm = match self.pcm_cache.get(&path) {
                    Some(cached) => cached.clone(),
                    None => match decode_audio_to_pcm(&path) {
                        Ok(decoded) => {
                            let arc = Arc::new(decoded);
                            self.pcm_cache.insert(path.clone(), arc.clone());
                            arc
                        }
                        Err(e) => {
                            log::warn!("Failed to decode audio for {}: {:?}", path.display(), e);
                            continue;
                        }
                    },
                };

                if pcm.is_empty() {
                    continue;
                }

                let clip_start_frame =
                    ((clip.position_us.max(0) as f64 / 1_000_000.0) * 48000.0).round() as usize;
                let source_start_frame =
                    ((clip.source_in_us.max(0) as f64 / 1_000_000.0) * 48000.0).round() as usize;
                let clip_duration_frames =
                    ((clip.duration_us() as f64 / 1_000_000.0) * 48000.0).round() as usize;
                let speed = clip.speed.abs().max(0.0001);

                for frame_i in 0..clip_duration_frames {
                    let dst_frame = clip_start_frame + frame_i;
                    if dst_frame >= total_sample_frames {
                        break;
                    }

                    let src_frame =
                        source_start_frame + ((frame_i as f64 * speed).round() as usize);
                    let src_idx = src_frame * 2;
                    if src_idx + 1 >= pcm.len() {
                        break;
                    }

                    let time_us =
                        clip.position_us + ((frame_i as f64 / 48000.0) * 1_000_000.0) as i64;
                    let vol = clip.effective_volume(time_us);
                    let pan = clip.pan.clamp(-1.0, 1.0);
                    let pan_l = ((1.0 - pan) * 0.5).clamp(0.0, 1.0);
                    let pan_r = ((1.0 + pan) * 0.5).clamp(0.0, 1.0);

                    let dst_idx = dst_frame * 2;
                    master_pcm[dst_idx] += pcm[src_idx] * vol * pan_l * 2.0;
                    master_pcm[dst_idx + 1] += pcm[src_idx + 1] * vol * pan_r * 2.0;
                }
            }
        }

        // Apply soft limiter to prevent clipping
        for s in &mut master_pcm {
            if *s > 1.0 || *s < -1.0 {
                *s = s.tanh();
            }
        }

        engine.set_timeline_pcm(master_pcm);
        Ok(())
    }

    /// Render the project timeline audio into a multichannel surround buffer (Mono, Stereo, 5.1, or 7.1)
    /// respecting each clip's 2D spatial positioning (`pan`, `pan_surround`, `lfe_send`).
    pub fn render_multichannel_pcm(
        &mut self,
        project: &Project,
        layout: ChannelLayout,
        solo_track: Option<TrackId>,
    ) -> Result<Vec<f32>> {
        let ch = layout.channels();
        let duration_us = project.timeline.duration_us();
        let duration_s = (duration_us as f64 / 1_000_000.0).max(0.1);
        let total_frames = (duration_s * 48000.0).round() as usize;
        let mut master_pcm = vec![0.0f32; total_frames * ch];

        for track in project.timeline.tracks() {
            if let Some(solo_id) = solo_track {
                if track.id != solo_id {
                    continue;
                }
            } else if !track.enabled {
                continue;
            }

            for clip in track.clips() {
                let audio_path = match &clip.kind {
                    ClipKind::Audio(aclip) => Self::resolve_path(&aclip.source_path),
                    ClipKind::Video(vclip) if vclip.has_audio => {
                        Self::resolve_path(&vclip.source_path)
                    }
                    _ => None,
                };

                let path = match audio_path {
                    Some(p) => p,
                    None => continue,
                };

                let pcm = match self.pcm_cache.get(&path) {
                    Some(cached) => cached.clone(),
                    None => match decode_audio_to_pcm(&path) {
                        Ok(decoded) => {
                            let arc = Arc::new(decoded);
                            self.pcm_cache.insert(path.clone(), arc.clone());
                            arc
                        }
                        Err(e) => {
                            log::warn!("Failed to decode audio for {}: {:?}", path.display(), e);
                            continue;
                        }
                    },
                };

                if pcm.is_empty() {
                    continue;
                }

                let clip_start_frame =
                    ((clip.position_us.max(0) as f64 / 1_000_000.0) * 48000.0).round() as usize;
                let source_start_frame =
                    ((clip.source_in_us.max(0) as f64 / 1_000_000.0) * 48000.0).round() as usize;
                let clip_duration_frames =
                    ((clip.duration_us() as f64 / 1_000_000.0) * 48000.0).round() as usize;
                let speed = clip.speed.abs().max(0.0001);

                for frame_i in 0..clip_duration_frames {
                    let dst_frame = clip_start_frame + frame_i;
                    if dst_frame >= total_frames {
                        break;
                    }

                    let src_frame =
                        source_start_frame + ((frame_i as f64 * speed).round() as usize);
                    let src_idx = src_frame * 2;
                    if src_idx + 1 >= pcm.len() {
                        break;
                    }

                    let time_us =
                        clip.position_us + ((frame_i as f64 / 48000.0) * 1_000_000.0) as i64;
                    let vol = clip.effective_volume(time_us);
                    let pan_x = clip.effective_pan(time_us);
                    let pan_y = clip.effective_pan_surround(time_us);
                    let lfe_send = clip.effective_lfe_send(time_us);

                    let spatial_params = SpatialParams {
                        pan_x,
                        pan_y,
                        center_percentage: FRAC_1_SQRT_2,
                        main_send: 1.0,
                        lfe_send,
                    };
                    let gains = SpatialPanner::compute_gains(layout, &spatial_params);

                    let left_s = pcm[src_idx] * vol;
                    let right_s = pcm[src_idx + 1] * vol;
                    let dst_base = dst_frame * ch;

                    match layout {
                        ChannelLayout::Mono => {
                            master_pcm[dst_base] += (left_s + right_s) * FRAC_1_SQRT_2;
                        }
                        ChannelLayout::Stereo => {
                            master_pcm[dst_base] += left_s * gains[0] * SQRT_2;
                            master_pcm[dst_base + 1] += right_s * gains[1] * SQRT_2;
                        }
                        ChannelLayout::Surround51 | ChannelLayout::Surround71 => {
                            master_pcm[dst_base] += left_s * gains[0]; // Left
                            master_pcm[dst_base + 1] += right_s * gains[1]; // Right
                            master_pcm[dst_base + 2] += (left_s + right_s) * 0.5 * gains[2]; // Center
                            master_pcm[dst_base + 3] += (left_s + right_s) * 0.5 * gains[3]; // LFE
                            master_pcm[dst_base + 4] += left_s * gains[4]; // Left Surround
                            master_pcm[dst_base + 5] += right_s * gains[5]; // Right Surround

                            if layout == ChannelLayout::Surround71 {
                                master_pcm[dst_base + 6] += left_s * gains[6]; // Left Back
                                master_pcm[dst_base + 7] += right_s * gains[7]; // Right Back
                            }
                        }
                    }
                }
            }
        }

        // Apply soft limiter to prevent clipping
        for s in &mut master_pcm {
            if *s > 1.0 || *s < -1.0 {
                *s = s.tanh();
            }
        }

        Ok(master_pcm)
    }

    /// Synchronize the project timeline into the stereo hardware engine by rendering
    /// multichannel surround audio and downmixing via ITU-R BS.775.
    pub fn sync_multichannel_timeline(
        &mut self,
        engine: &AudioEngine,
        project: &Project,
        layout: ChannelLayout,
        solo_track: Option<TrackId>,
    ) -> Result<()> {
        let multi_pcm = self.render_multichannel_pcm(project, layout, solo_track)?;
        if layout == ChannelLayout::Stereo {
            engine.set_timeline_pcm(multi_pcm);
        } else {
            let frame_count = multi_pcm.len() / layout.channels();
            let mut stereo_pcm = vec![0.0f32; frame_count * 2];
            ItuDownmixer::downmix_to_stereo(&multi_pcm, layout, &mut stereo_pcm);
            engine.set_timeline_pcm(stereo_pcm);
        }
        Ok(())
    }

    /// Clear cached PCM data to force reload from disk.
    pub fn clear_cache(&mut self) {
        self.pcm_cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::{Clip, TrackKind};

    #[test]
    fn test_timeline_audio_scheduler_mixes_clips() {
        let clock = crate::AudioClock::new();
        let engine = AudioEngine::new(clock).expect("Failed to create AudioEngine");
        let mut scheduler = TimelineAudioScheduler::new();

        let mut project = Project::new("Audio Mix Test");
        let track_id = project.timeline.add_track(TrackKind::Audio, "A1");

        let clip = Clip::new_audio(
            "ready_to_use/windows/samples/audio/soundtrack.wav",
            0,
            2_000_000,
        );

        let track = project.timeline.track_mut(track_id).unwrap();
        track.add_clip(clip);

        let res = scheduler.sync_timeline(&engine, &project, None);
        assert!(res.is_ok());
    }

    #[test]
    fn test_timeline_audio_scheduler_multichannel_51_and_71() {
        let clock = crate::AudioClock::new();
        let engine = AudioEngine::new(clock).expect("Failed to create AudioEngine");
        let mut scheduler = TimelineAudioScheduler::new();

        let mut project = Project::new("Surround Audio Test");
        let track_id = project
            .timeline
            .add_track(TrackKind::Audio, "Surround Voice");

        let mut clip = Clip::new_audio(
            "ready_to_use/windows/samples/audio/soundtrack.wav",
            0,
            2_000_000,
        );
        // Position at rear surround
        clip.pan = -0.5;
        clip.pan_surround = -1.0;
        clip.lfe_send = 0.3;

        let track = project.timeline.track_mut(track_id).unwrap();
        track.add_clip(clip);

        // Render 5.1 surround PCM
        let pcm_51 = scheduler.render_multichannel_pcm(&project, ChannelLayout::Surround51, None);
        assert!(pcm_51.is_ok());
        let buf_51 = pcm_51.unwrap();
        assert_eq!(buf_51.len() % 6, 0);

        // Render 7.1 surround PCM
        let pcm_71 = scheduler.render_multichannel_pcm(&project, ChannelLayout::Surround71, None);
        assert!(pcm_71.is_ok());
        let buf_71 = pcm_71.unwrap();
        assert_eq!(buf_71.len() % 8, 0);

        // Test sync_multichannel_timeline with automatic ITU downmixing
        let sync_res = scheduler.sync_multichannel_timeline(
            &engine,
            &project,
            ChannelLayout::Surround51,
            None,
        );
        assert!(sync_res.is_ok());
    }
}
