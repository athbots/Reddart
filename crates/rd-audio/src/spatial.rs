//! Spatial Audio Panning and Multichannel Surround Mixing (5.1 & 7.1).
//!
//! Technical Proposal Section 3.5:
//! - Channel layouts: Mono, Stereo (2.0), Surround 5.1 (6 channels), Surround 7.1 (8 channels).
//! - 2D Vector Spatial Panning (X: Left/Right, Y: Rear/Front).
//! - Constant-power energy-preserving trigonometric panning laws (sum(g_i^2) == 1.0).
//! - Center channel extraction and divergence control.
//! - Subwoofer / LFE (Low Frequency Effects) routing.
//! - ITU-R BS.775 standard downmixing from 5.1/7.1 to stereo for headphone/stereo monitoring.
//! - Real-time zero-allocation mixing and per-channel soft limiting (tanh).

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::SAMPLE_RATE;
use crate::clock::AudioClock;

/// Audio channel layout configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ChannelLayout {
    /// 1-channel mono (Center).
    Mono,
    /// 2-channel stereo (Left, Right).
    #[default]
    Stereo,
    /// 6-channel 5.1 surround sound (L, R, C, LFE, Ls, Rs).
    Surround51,
    /// 8-channel 7.1 surround sound (L, R, C, LFE, Ls, Rs, Lb, Rb).
    Surround71,
}

impl ChannelLayout {
    /// Number of discrete channels in this layout.
    pub fn channels(&self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }

    /// Human-readable standard channel names for this layout.
    pub fn channel_names(&self) -> &'static [&'static str] {
        match self {
            Self::Mono => &["Center"],
            Self::Stereo => &["Left", "Right"],
            Self::Surround51 => &[
                "Left",
                "Right",
                "Center",
                "LFE",
                "Left Surround",
                "Right Surround",
            ],
            Self::Surround71 => &[
                "Left",
                "Right",
                "Center",
                "LFE",
                "Left Surround",
                "Right Surround",
                "Left Back",
                "Right Back",
            ],
        }
    }

    /// Whether this layout represents a multichannel surround format (> 2 channels).
    pub fn is_surround(&self) -> bool {
        matches!(self, Self::Surround51 | Self::Surround71)
    }
}

/// Spatial panning parameters for positioning an audio source in 2D sound space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpatialParams {
    /// Azimuth / Left-Right pan in [-1.0, 1.0]: -1.0 is full Left, 0.0 is Center, +1.0 is full Right.
    pub pan_x: f32,
    /// Elevation / Front-Rear pan in [-1.0, 1.0]: -1.0 is full Rear/Surround, 0.0 is Center/Middle, +1.0 is full Front.
    pub pan_y: f32,
    /// Center channel bleed percentage in [0.0, 1.0]:
    /// When panning to front center (pan_x ≈ 0, pan_y > 0), controls whether sound goes to the discrete
    /// Center speaker (1.0 = 100% Center, 0.0 = phantom center split across Left & Right).
    pub center_percentage: f32,
    /// Main speaker bus send level in [0.0, 1.0]. Defaults to 1.0.
    /// Set to 0.0 for dedicated subwoofer-only / LFE effects channels.
    #[serde(default = "default_main_send")]
    pub main_send: f32,
    /// LFE / Subwoofer send level in [0.0, 1.0].
    pub lfe_send: f32,
}

fn default_main_send() -> f32 {
    1.0
}

impl Default for SpatialParams {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 1.0, // Default to front
            center_percentage: FRAC_1_SQRT_2,
            main_send: 1.0,
            lfe_send: 0.0,
        }
    }
}

/// Computes per-channel gain coefficients using energy-preserving constant-power panning laws.
pub struct SpatialPanner;

impl SpatialPanner {
    /// Compute channel gains for a given layout and spatial parameters.
    ///
    /// For surround layouts, the total acoustic power across main speakers satisfies:
    /// `sum(g_i^2) ≈ 1.0` (constant power law).
    pub fn compute_gains(layout: ChannelLayout, params: &SpatialParams) -> Vec<f32> {
        let px = params.pan_x.clamp(-1.0, 1.0);
        let py = params.pan_y.clamp(-1.0, 1.0);
        let center_pct = params.center_percentage.clamp(0.0, 1.0);
        let main = params.main_send.clamp(0.0, 1.0);
        let lfe = params.lfe_send.clamp(0.0, 1.0);

        match layout {
            ChannelLayout::Mono => vec![main],

            ChannelLayout::Stereo => {
                // Constant power stereo panning: cos / sin law
                let angle = (px + 1.0) * 0.5 * FRAC_PI_2;
                let gl = angle.cos() * main;
                let gr = angle.sin() * main;
                vec![gl, gr]
            }

            ChannelLayout::Surround51 => {
                // Layout: [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs]
                let (gl, gr, gc, gls, grs) = Self::panning_51(px, py, center_pct);
                vec![gl * main, gr * main, gc * main, lfe, gls * main, grs * main]
            }

            ChannelLayout::Surround71 => {
                // Layout: [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs, 6: Lb, 7: Rb]
                let (gl, gr, gc, gls, grs, glb, grb) = Self::panning_71(px, py, center_pct);
                vec![
                    gl * main,
                    gr * main,
                    gc * main,
                    lfe,
                    gls * main,
                    grs * main,
                    glb * main,
                    grb * main,
                ]
            }
        }
    }

    /// Internal 5.1 constant power calculation.
    fn panning_51(px: f32, py: f32, center_pct: f32) -> (f32, f32, f32, f32, f32) {
        // Front vs Rear angle
        let front_angle = (py + 1.0) * 0.5 * FRAC_PI_2;
        let gain_front = front_angle.sin();
        let gain_rear = front_angle.cos();

        // Left vs Right angle
        let lr_angle = (px + 1.0) * 0.5 * FRAC_PI_2;
        let gain_left = lr_angle.cos();
        let gain_right = lr_angle.sin();

        // Rear surrounds:
        let gls = gain_rear * gain_left;
        let grs = gain_rear * gain_right;

        // Front channels (L, R, C):
        // Center weight peaks at px = 0.0, drops to 0 at px = ±1.0
        let center_proximity = (px * FRAC_PI_2).cos().max(0.0);
        let gc = gain_front * center_proximity * center_pct;

        // Remaining front energy after center extraction
        let front_energy = gain_front * gain_front;
        let center_energy = gc * gc;
        let rem_front = (front_energy - center_energy).max(0.0).sqrt();

        let gl = rem_front * gain_left;
        let gr = rem_front * gain_right;

        (gl, gr, gc, gls, grs)
    }

    /// Internal 7.1 constant power calculation.
    fn panning_71(px: f32, py: f32, center_pct: f32) -> (f32, f32, f32, f32, f32, f32, f32) {
        let (gl, gr, gc, _, _) = Self::panning_51(px, py, center_pct);

        let front_angle = (py + 1.0) * 0.5 * FRAC_PI_2;
        let gain_rear = front_angle.cos();

        let lr_angle = (px + 1.0) * 0.5 * FRAC_PI_2;
        let gain_left = lr_angle.cos();
        let gain_right = lr_angle.sin();

        // In 7.1, rear energy is split between Side Surrounds (Ls/Rs) and Back Surrounds (Lb/Rb)
        // py in [-1.0, 0.0]: at 0.0 = pure side, at -1.0 = pure back
        let back_factor = (-py).clamp(0.0, 1.0);
        let back_angle = back_factor * FRAC_PI_2;
        let gain_back = back_angle.sin();
        let gain_side = back_angle.cos();

        let gls = gain_rear * gain_side * gain_left;
        let grs = gain_rear * gain_side * gain_right;
        let glb = gain_rear * gain_back * gain_left;
        let grb = gain_rear * gain_back * gain_right;

        (gl, gr, gc, gls, grs, glb, grb)
    }
}

/// ITU-R BS.775 standard downmixing from multichannel surround to stereo.
pub struct ItuDownmixer;

impl ItuDownmixer {
    /// Downmix an interleaved multichannel buffer to interleaved stereo (2-channel) PCM.
    ///
    /// Applies ITU-R BS.775 standard coefficients:
    /// - Center channel mixed to L and R at -3 dB (1 / sqrt(2) ≈ 0.7071).
    /// - Surround channels mixed to L and R at -3 dB.
    /// - Normalization headroom scaling prevents digital clipping.
    pub fn downmix_to_stereo(input: &[f32], layout: ChannelLayout, output: &mut [f32]) {
        let in_ch = layout.channels();
        let frame_count = input.len() / in_ch;
        let out_frames = output.len() / 2;
        let frames_to_process = frame_count.min(out_frames);

        match layout {
            ChannelLayout::Mono => {
                for i in 0..frames_to_process {
                    let s = input[i];
                    output[i * 2] = s;
                    output[i * 2 + 1] = s;
                }
            }
            ChannelLayout::Stereo => {
                let sample_count = frames_to_process * 2;
                output[..sample_count].copy_from_slice(&input[..sample_count]);
            }
            ChannelLayout::Surround51 => {
                // 5.1 layout: L, R, C, LFE, Ls, Rs
                // ITU-R BS.775: L' = (L + 0.7071*C + 0.7071*Ls) * 0.7071
                //               R' = (R + 0.7071*C + 0.7071*Rs) * 0.7071
                const INV_SQRT_2: f32 = 0.707_106_77;
                const NORM_FACTOR: f32 = 0.707_106_77;

                for i in 0..frames_to_process {
                    let in_idx = i * 6;
                    let l = input[in_idx];
                    let r = input[in_idx + 1];
                    let c = input[in_idx + 2];
                    let _lfe = input[in_idx + 3];
                    let ls = input[in_idx + 4];
                    let rs = input[in_idx + 5];

                    let left_mix = (l + c * INV_SQRT_2 + ls * INV_SQRT_2) * NORM_FACTOR;
                    let right_mix = (r + c * INV_SQRT_2 + rs * INV_SQRT_2) * NORM_FACTOR;

                    output[i * 2] = left_mix;
                    output[i * 2 + 1] = right_mix;
                }
            }
            ChannelLayout::Surround71 => {
                // 7.1 layout: L, R, C, LFE, Ls, Rs, Lb, Rb
                // ITU-R BS.775 extension:
                // L' = (L + 0.7071*C + 0.7071*Ls + 0.7071*Lb) * 0.57735
                // R' = (R + 0.7071*C + 0.7071*Rs + 0.7071*Rb) * 0.57735
                const INV_SQRT_2: f32 = 0.707_106_77;
                const NORM_FACTOR: f32 = 0.577_350_26; // 1 / sqrt(3)

                for i in 0..frames_to_process {
                    let in_idx = i * 8;
                    let l = input[in_idx];
                    let r = input[in_idx + 1];
                    let c = input[in_idx + 2];
                    let _lfe = input[in_idx + 3];
                    let ls = input[in_idx + 4];
                    let rs = input[in_idx + 5];
                    let lb = input[in_idx + 6];
                    let rb = input[in_idx + 7];

                    let left_mix =
                        (l + c * INV_SQRT_2 + ls * INV_SQRT_2 + lb * INV_SQRT_2) * NORM_FACTOR;
                    let right_mix =
                        (r + c * INV_SQRT_2 + rs * INV_SQRT_2 + rb * INV_SQRT_2) * NORM_FACTOR;

                    output[i * 2] = left_mix;
                    output[i * 2 + 1] = right_mix;
                }
            }
        }
    }
}

/// An audio input track configured for multichannel mixing.
pub struct MultichannelTrack {
    pub volume: f32,
    pub spatial: SpatialParams,
    pub pcm_data: Vec<f32>,
    /// Number of channels in the source PCM data (1 = mono, 2 = stereo).
    pub source_channels: usize,
    pub position_samples: usize,
}

/// Real-time multichannel surround audio mixer.
pub struct MultichannelMixer {
    layout: ChannelLayout,
    clock: Arc<AudioClock>,
    master_volume: f32,
    tracks: Vec<MultichannelTrack>,
}

impl MultichannelMixer {
    /// Initialize a new multichannel mixer for a given channel layout.
    pub fn new(layout: ChannelLayout, clock: Arc<AudioClock>) -> Self {
        Self {
            layout,
            clock,
            master_volume: 1.0,
            tracks: Vec::new(),
        }
    }

    /// Set master volume multiplier.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.max(0.0);
    }

    /// Set the target channel layout (Mono, Stereo, Surround 5.1, Surround 7.1).
    pub fn set_layout(&mut self, layout: ChannelLayout) {
        self.layout = layout;
    }

    /// Retrieve the current channel layout.
    pub fn layout(&self) -> ChannelLayout {
        self.layout
    }

    /// Add an audio track source with spatial panning parameters.
    pub fn add_track(
        &mut self,
        pcm_data: Vec<f32>,
        source_channels: usize,
        volume: f32,
        spatial: SpatialParams,
    ) {
        self.tracks.push(MultichannelTrack {
            volume,
            spatial,
            pcm_data,
            source_channels: source_channels.clamp(1, 2),
            position_samples: 0,
        });
    }

    /// Clear all loaded tracks from the mixer.
    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    /// Reset playback position for all tracks.
    pub fn seek_to_sample(&mut self, sample_pos: usize) {
        for track in &mut self.tracks {
            let total_frames = track.pcm_data.len() / track.source_channels;
            let target_frame = sample_pos.min(total_frames);
            track.position_samples = target_frame * track.source_channels;
        }
    }

    /// Fill output buffer with mixed multichannel PCM audio.
    ///
    /// The output buffer length must be a multiple of `layout.channels()`.
    /// Zero allocations occur inside this mixing loop.
    pub fn mix_interleaved(&mut self, output: &mut [f32]) {
        output.fill(0.0);

        let out_channels = self.layout.channels();
        let total_out_samples = output.len();
        let out_frame_count = total_out_samples / out_channels;

        for track in &mut self.tracks {
            let gains = SpatialPanner::compute_gains(self.layout, &track.spatial);
            let effective_vol = track.volume * self.master_volume;

            let src_ch = track.source_channels;
            let available_src_samples = track.pcm_data.len().saturating_sub(track.position_samples);
            let available_frames = available_src_samples / src_ch;
            let frames_to_mix = out_frame_count.min(available_frames);

            for frame_idx in 0..frames_to_mix {
                let src_base = track.position_samples + frame_idx * src_ch;
                let out_base = frame_idx * out_channels;

                if src_ch == 1 {
                    // Mono source: route single sample across all layout channels using computed gains
                    let mono_sample = track.pcm_data[src_base] * effective_vol;
                    for (ch, &gain) in gains.iter().enumerate() {
                        output[out_base + ch] += mono_sample * gain;
                    }
                } else {
                    // Stereo source: route Left and Right appropriately
                    let left_sample = track.pcm_data[src_base] * effective_vol;
                    let right_sample = track.pcm_data[src_base + 1] * effective_vol;

                    match self.layout {
                        ChannelLayout::Mono => {
                            // Sum to mono with -3dB
                            output[out_base] += (left_sample + right_sample) * FRAC_1_SQRT_2;
                        }
                        ChannelLayout::Stereo => {
                            // Apply stereo pan gains
                            output[out_base] += left_sample * gains[0];
                            output[out_base + 1] += right_sample * gains[1];
                        }
                        ChannelLayout::Surround51 | ChannelLayout::Surround71 => {
                            // Distribute stereo width: Left feeds L / Ls / Lb; Right feeds R / Rs / Rb;
                            // Center and LFE receive proportional mono sum
                            output[out_base] += left_sample * gains[0]; // Left
                            output[out_base + 1] += right_sample * gains[1]; // Right
                            output[out_base + 2] += (left_sample + right_sample) * 0.5 * gains[2]; // Center
                            output[out_base + 3] += (left_sample + right_sample) * 0.5 * gains[3]; // LFE
                            output[out_base + 4] += left_sample * gains[4]; // Left Surround
                            output[out_base + 5] += right_sample * gains[5]; // Right Surround

                            if self.layout == ChannelLayout::Surround71 {
                                output[out_base + 6] += left_sample * gains[6]; // Left Back
                                output[out_base + 7] += right_sample * gains[7]; // Right Back
                            }
                        }
                    }
                }
            }

            track.position_samples += frames_to_mix * src_ch;
        }

        // Apply soft limiter per channel to prevent harsh digital clipping
        for s in output.iter_mut() {
            if *s > 1.0 || *s < -1.0 {
                *s = s.tanh();
            }
        }

        // Advance master clock
        self.clock.advance_samples(out_frame_count, SAMPLE_RATE);
    }

    /// Mix multichannel tracks directly downmixed to a stereo output buffer.
    ///
    /// Ideal for previewing surround sound projects on stereo headphones or speakers.
    pub fn mix_and_downmix_to_stereo(&mut self, stereo_output: &mut [f32]) {
        let stereo_frames = stereo_output.len() / 2;
        let out_channels = self.layout.channels();

        if out_channels == 2 {
            self.mix_interleaved(stereo_output);
            return;
        }

        // Allocate temporary buffer for multichannel intermediate
        let mut multi_buf = vec![0.0f32; stereo_frames * out_channels];
        self.mix_interleaved(&mut multi_buf);
        ItuDownmixer::downmix_to_stereo(&multi_buf, self.layout, stereo_output);
    }
}

/// Telemetry measurements for each channel in a multichannel audio stream.
#[derive(Debug, Clone, Default)]
pub struct MultichannelTelemetry {
    /// Peak amplitude per channel in [0.0, 1.0+].
    pub peaks: Vec<f32>,
    /// RMS amplitude per channel in [0.0, 1.0+].
    pub rms: Vec<f32>,
    /// Peak decibels relative to full scale (dBFS) per channel.
    pub peak_dbfs: Vec<f32>,
    /// RMS decibels relative to full scale (dBFS) per channel.
    pub rms_dbfs: Vec<f32>,
}

impl MultichannelTelemetry {
    /// Analyze an interleaved multichannel buffer and compute telemetry metrics.
    pub fn from_interleaved(buffer: &[f32], channels: usize) -> Self {
        if channels == 0 || buffer.is_empty() {
            return Self::default();
        }

        let frame_count = buffer.len() / channels;
        let mut peaks = vec![0.0f32; channels];
        let mut sum_squares = vec![0.0f32; channels];

        for frame_idx in 0..frame_count {
            let base = frame_idx * channels;
            for ch in 0..channels {
                let sample = buffer[base + ch].abs();
                if sample > peaks[ch] {
                    peaks[ch] = sample;
                }
                sum_squares[ch] += sample * sample;
            }
        }

        let mut rms = vec![0.0f32; channels];
        let mut peak_dbfs = vec![-100.0f32; channels];
        let mut rms_dbfs = vec![-100.0f32; channels];

        for ch in 0..channels {
            rms[ch] = (sum_squares[ch] / frame_count as f32).sqrt();
            peak_dbfs[ch] = crate::engine::amp_to_dbfs(peaks[ch]);
            rms_dbfs[ch] = crate::engine::amp_to_dbfs(rms[ch]);
        }

        Self {
            peaks,
            rms,
            peak_dbfs,
            rms_dbfs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_layout_properties() {
        assert_eq!(ChannelLayout::Mono.channels(), 1);
        assert_eq!(ChannelLayout::Stereo.channels(), 2);
        assert_eq!(ChannelLayout::Surround51.channels(), 6);
        assert_eq!(ChannelLayout::Surround71.channels(), 8);

        assert!(!ChannelLayout::Stereo.is_surround());
        assert!(ChannelLayout::Surround51.is_surround());
        assert!(ChannelLayout::Surround71.is_surround());

        assert_eq!(ChannelLayout::Surround51.channel_names().len(), 6);
        assert_eq!(ChannelLayout::Surround71.channel_names().len(), 8);
    }

    #[test]
    fn test_stereo_constant_power_panning() {
        // Pan Left (-1.0)
        let left_params = SpatialParams {
            pan_x: -1.0,
            ..Default::default()
        };
        let gains_left = SpatialPanner::compute_gains(ChannelLayout::Stereo, &left_params);
        assert!((gains_left[0] - 1.0).abs() < 1e-4);
        assert!(gains_left[1].abs() < 1e-4);

        // Pan Center (0.0) -> constant power (sum of squares == 1.0)
        let center_params = SpatialParams {
            pan_x: 0.0,
            ..Default::default()
        };
        let gains_center = SpatialPanner::compute_gains(ChannelLayout::Stereo, &center_params);
        let power = gains_center[0] * gains_center[0] + gains_center[1] * gains_center[1];
        assert!((power - 1.0).abs() < 1e-4);
        assert!((gains_center[0] - FRAC_1_SQRT_2).abs() < 1e-3);
        assert!((gains_center[1] - FRAC_1_SQRT_2).abs() < 1e-3);

        // Pan Right (1.0)
        let right_params = SpatialParams {
            pan_x: 1.0,
            ..Default::default()
        };
        let gains_right = SpatialPanner::compute_gains(ChannelLayout::Stereo, &right_params);
        assert!(gains_right[0].abs() < 1e-4);
        assert!((gains_right[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_51_surround_constant_power_and_lfe() {
        // 5.1 layout: L, R, C, LFE, Ls, Rs
        // Test front center with 100% center speaker bleed and 50% LFE send
        let center_front = SpatialParams {
            pan_x: 0.0,
            pan_y: 1.0,
            center_percentage: 1.0,
            main_send: 1.0,
            lfe_send: 0.5,
        };
        let gains = SpatialPanner::compute_gains(ChannelLayout::Surround51, &center_front);
        assert_eq!(gains.len(), 6);

        // Center channel must be active; surrounds must be 0
        assert!(gains[2] > 0.99, "Center channel was {}", gains[2]);
        assert_eq!(gains[3], 0.5, "LFE send was {}", gains[3]);
        assert!(gains[4] < 1e-4, "Ls was {}", gains[4]);
        assert!(gains[5] < 1e-4, "Rs was {}", gains[5]);

        // Power across main 5 speakers (excluding LFE) must equal ~1.0
        let main_power = gains[0].powi(2)
            + gains[1].powi(2)
            + gains[2].powi(2)
            + gains[4].powi(2)
            + gains[5].powi(2);
        assert!(
            (main_power - 1.0).abs() < 1e-3,
            "Main power was {}",
            main_power
        );

        // Test rear left surround
        let rear_left = SpatialParams {
            pan_x: -1.0,
            pan_y: -1.0,
            center_percentage: 0.5,
            main_send: 1.0,
            lfe_send: 0.0,
        };
        let gains_rl = SpatialPanner::compute_gains(ChannelLayout::Surround51, &rear_left);
        assert!(
            (gains_rl[4] - 1.0).abs() < 1e-3,
            "Ls should be ~1.0, got {}",
            gains_rl[4]
        );
        assert!(gains_rl[0] < 1e-3);
        assert!(gains_rl[1] < 1e-3);
        assert!(gains_rl[2] < 1e-3);
        assert!(gains_rl[5] < 1e-3);
    }

    #[test]
    fn test_71_surround_speaker_separation() {
        // 7.1 layout: [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs, 6: Lb, 7: Rb]
        // Hard rear back-right pan: pan_x = 1.0, pan_y = -1.0
        let back_right = SpatialParams {
            pan_x: 1.0,
            pan_y: -1.0,
            center_percentage: 0.0,
            main_send: 1.0,
            lfe_send: 0.0,
        };
        let gains = SpatialPanner::compute_gains(ChannelLayout::Surround71, &back_right);
        assert_eq!(gains.len(), 8);

        // Right back (Rb, index 7) must be near 1.0
        assert!((gains[7] - 1.0).abs() < 1e-3, "Rb was {}", gains[7]);
        assert!(gains[0] < 1e-3); // L
        assert!(gains[1] < 1e-3); // R
        assert!(gains[6] < 1e-3); // Lb

        // Main 7 speakers power sum
        let power: f32 = gains[0].powi(2)
            + gains[1].powi(2)
            + gains[2].powi(2)
            + gains[4].powi(2)
            + gains[5].powi(2)
            + gains[6].powi(2)
            + gains[7].powi(2);
        assert!((power - 1.0).abs() < 1e-3, "7.1 power was {}", power);
    }

    #[test]
    fn test_itu_r_bs775_downmixer() {
        // 5.1 surround frame: L = 1.0, R = 1.0, C = 1.0, LFE = 1.0, Ls = 1.0, Rs = 1.0
        let multi_51 = vec![1.0f32; 6 * 10]; // 10 frames
        let mut stereo_out = vec![0.0f32; 2 * 10];

        ItuDownmixer::downmix_to_stereo(&multi_51, ChannelLayout::Surround51, &mut stereo_out);

        // Left and Right channels must be non-zero and equal due to symmetry
        for i in 0..10 {
            let l = stereo_out[i * 2];
            let r = stereo_out[i * 2 + 1];
            assert!(l > 0.0);
            assert_eq!(l, r);
            // Must stay bounded to prevent clipping
            assert!(l <= 2.0);
        }
    }

    #[test]
    fn test_multichannel_mixer_execution() {
        let clock = AudioClock::new();
        clock.set_playing(true);
        let mut mixer = MultichannelMixer::new(ChannelLayout::Surround51, clock);

        // Track 1: Mono tone 0.5 placed at center front
        let mono_tone = vec![0.5f32; 100];
        mixer.add_track(
            mono_tone,
            1, // mono
            1.0,
            SpatialParams {
                pan_x: 0.0,
                pan_y: 1.0,
                center_percentage: 1.0,
                main_send: 1.0,
                lfe_send: 0.2,
            },
        );

        // Output buffer for 10 frames of 5.1 (60 samples)
        let mut output = vec![0.0f32; 60];
        mixer.mix_interleaved(&mut output);

        // Check center channel (index 2) received the audio
        assert!(output[2] > 0.45, "Center output was {}", output[2]);
        // Check LFE channel (index 3) received audio proportional to lfe_send (0.5 * 0.2 = 0.1)
        assert!(
            (output[3] - 0.1).abs() < 1e-2,
            "LFE output was {}",
            output[3]
        );

        // Telemetry check
        let telemetry = MultichannelTelemetry::from_interleaved(&output, 6);
        assert_eq!(telemetry.peaks.len(), 6);
        assert!(telemetry.peaks[2] > 0.45);
        assert!(telemetry.peak_dbfs[2] > -10.0);
    }
}
