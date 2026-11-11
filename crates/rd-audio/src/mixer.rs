//! Audio mixer: real-time PCM mixing on a dedicated thread.
//!
//! Section 3.5 of the Technical Proposal:
//! Audio is mixed in stereo 32-bit floating point PCM at 48 kHz.
//! Zero allocations, locks, or system calls in the audio render callback.

use crate::clock::AudioClock;
use crate::{CHANNELS, SAMPLE_RATE};
use std::sync::Arc;

/// Audio track channel data for real-time mixing.
pub struct AudioTrackInput {
    pub volume: f32,
    pub pcm_data: Vec<f32>,
    pub position_samples: usize,
}

/// Real-time audio mixer.
pub struct AudioMixer {
    clock: Arc<AudioClock>,
    master_volume: f32,
    tracks: Vec<AudioTrackInput>,
}

impl AudioMixer {
    pub fn new(clock: Arc<AudioClock>) -> Self {
        Self {
            clock,
            master_volume: 1.0,
            tracks: Vec::new(),
        }
    }

    /// Add an audio track source.
    pub fn add_track(&mut self, pcm_data: Vec<f32>, volume: f32) {
        self.tracks.push(AudioTrackInput {
            volume,
            pcm_data,
            position_samples: 0,
        });
    }

    /// Clear all loaded tracks from the mixer.
    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    /// Set master volume multiplier (0.0 to 1.0+).
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.max(0.0);
    }

    /// Fill output buffer with mixed stereo PCM audio.
    /// This function is called from the high-priority real-time audio callback.
    /// MUST NOT allocate memory, acquire locks, or log.
    pub fn mix_interleaved(&mut self, output: &mut [f32]) {
        // Zero output buffer
        output.fill(0.0);

        let channels = CHANNELS as usize;
        let total_samples = output.len();
        let frame_count = total_samples / channels;

        // Mix each track into the output buffer
        for track in &mut self.tracks {
            let track_volume = track.volume * self.master_volume;
            let available = track.pcm_data.len().saturating_sub(track.position_samples);
            let samples_to_mix = total_samples.min(available);

            for (i, out_sample) in output.iter_mut().enumerate().take(samples_to_mix) {
                *out_sample += track.pcm_data[track.position_samples + i] * track_volume;
            }

            track.position_samples += samples_to_mix;
        }

        // Apply soft limiter to prevent harsh digital clipping
        for sample in output.iter_mut() {
            if *sample > 1.0 || *sample < -1.0 {
                // Smooth hyperbolic tangent soft knee
                *sample = sample.tanh();
            }
        }

        // Advance master audio clock
        self.clock.advance_samples(frame_count, SAMPLE_RATE);
    }

    /// Reset playback position for all tracks.
    pub fn seek_to_sample(&mut self, sample_pos: usize) {
        for track in &mut self.tracks {
            track.position_samples = sample_pos.min(track.pcm_data.len());
        }
    }

    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_mixer_summation() {
        let clock = AudioClock::new();
        clock.set_playing(true);
        let mut mixer = AudioMixer::new(clock.clone());

        // Track 1: 0.3 constant tone
        let track1 = vec![0.3; 100];
        // Track 2: 0.2 constant tone
        let track2 = vec![0.2; 100];

        mixer.add_track(track1, 1.0);
        mixer.add_track(track2, 1.0);

        let mut output = vec![0.0; 10];
        mixer.mix_interleaved(&mut output);

        for sample in &output {
            assert!((sample - 0.5).abs() < 1e-4);
        }
    }

    #[test]
    fn test_soft_limiter() {
        let clock = AudioClock::new();
        let mut mixer = AudioMixer::new(clock);

        // Very loud tracks that would normally clip (sum = 2.5 > 1.0)
        let track1 = vec![1.0; 20];
        let track2 = vec![1.5; 20];

        mixer.add_track(track1, 1.0);
        mixer.add_track(track2, 1.0);

        let mut output = vec![0.0; 20];
        mixer.mix_interleaved(&mut output);

        for sample in &output {
            // Must stay bounded within [-1.0, 1.0] thanks to soft limiter
            assert!(*sample <= 1.0);
            assert!(*sample >= -1.0);
        }
    }
}
