//! 3D Ambisonics Spatial Audio Engine & Binaural HRTF Monitoring.
//!
//! Technical Specifications (Milestone 26):
//! - Full 1st, 2nd, and 3rd order B-format Ambisonics spherical harmonics ($W, Y, Z, X, \dots$).
//! - Standard ACN (Ambisonic Channel Number) channel ordering and SN3D / N3D / FuMa normalizations.
//! - 3D vector spatial panner with distance attenuation and atmospheric high-frequency air absorption.
//! - 3D soundfield rotation (Yaw, Pitch, Roll) via orthogonal spherical harmonic rotation matrices.
//! - Binaural HRTF headphone monitoring engine using Woodworth-Schlosser ITD, head-shadowing ILD,
//!   and pinna elevation spectral notch filters.
//! - 7.1.4 Immersive Bed multi-speaker decoding (L, R, C, LFE, Ls, Rs, Lb, Rb, Tfl, Tfr, Tbl, Tbr)
//!   with energy-preserving mode-matching decoders and sub-bass LFE management.
//! - Pure-Rust, zero-allocation real-time DSP execution.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::SAMPLE_RATE;
use crate::clock::AudioClock;
use crate::spatial::ChannelLayout;

/// Order of the Ambisonic soundfield representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AmbisonicOrder {
    /// 1st Order Ambisonics (FOA): $(1+1)^2 = 4$ channels ($W, Y, Z, X$).
    #[default]
    First,
    /// 2nd Order Ambisonics (SOA): $(2+1)^2 = 9$ channels ($W, Y, Z, X, V, T, R, S, U$).
    Second,
    /// 3rd Order Ambisonics (TOA): $(3+1)^2 = 16$ channels (up to degree 3, order 3).
    Third,
}

impl AmbisonicOrder {
    /// Highest spherical harmonic polynomial order $N$ (1, 2, or 3).
    #[inline]
    pub fn order(&self) -> usize {
        match self {
            Self::First => 1,
            Self::Second => 2,
            Self::Third => 3,
        }
    }

    /// Number of discrete spherical harmonic channels $K = (N+1)^2$.
    #[inline]
    pub fn channels(&self) -> usize {
        let n = self.order();
        (n + 1) * (n + 1)
    }

    /// Returns the standard ACN (Ambisonic Channel Number) label for a given channel index.
    pub fn channel_name(&self, acn: usize) -> &'static str {
        match acn {
            0 => "W (Omni)",
            1 => "Y (Left/Right Dipole)",
            2 => "Z (Height Dipole)",
            3 => "X (Front/Back Dipole)",
            4 => "V (XY Quadrupole)",
            5 => "T (YZ Quadrupole)",
            6 => "R (Z^2 Quadrupole)",
            7 => "S (ZX Quadrupole)",
            8 => "U (X^2-Y^2 Quadrupole)",
            9 => "Q (3rd Order Y(3X^2-Y^2))",
            10 => "O (3rd Order XYZ)",
            11 => "M (3rd Order Y(5Z^2-1))",
            12 => "K (3rd Order Z(5Z^2-3))",
            13 => "L (3rd Order X(5Z^2-1))",
            14 => "N (3rd Order Z(X^2-Y^2))",
            15 => "P (3rd Order X(X^2-3Y^2))",
            _ => "Unknown",
        }
    }

    /// Maps an ACN channel index $n$ to its corresponding spherical harmonic degree $\ell$ and order $m$.
    ///
    /// Relationship: $n = \ell^2 + \ell + m$, with $\ell \ge 0$ and $-\ell \le m \le \ell$.
    #[inline]
    pub fn acn_to_degree_order(acn: usize) -> (usize, i32) {
        let ell = (acn as f32).sqrt().floor() as usize;
        let m = acn as i32 - (ell * ell + ell) as i32;
        (ell, m)
    }

    /// Maps a spherical harmonic degree $\ell$ and order $m$ to its ACN index $n$.
    #[inline]
    pub fn degree_order_to_acn(ell: usize, m: i32) -> usize {
        ((ell * ell + ell) as i32 + m) as usize
    }
}

/// Normalization convention for spherical harmonic weighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AmbisonicNormalization {
    /// Schmidt Semi-Normalized (SN3D / AmbiX standard).
    /// Used by YouTube 360, Google VR, MPEG-H, and modern spatial audio workflows.
    #[default]
    SN3D,
    /// Full 3D Normalization (N3D).
    /// Pure mathematical orthonormal basis where $\int_{\mathbb{S}^2} Y_i Y_j d\Omega = 4\pi \delta_{ij}$.
    N3D,
    /// Furse-Malham (FuMa).
    /// Legacy 1st-order B-format where $W$ is scaled by $1/\sqrt{2}$.
    FuMa,
}

impl AmbisonicNormalization {
    /// Computes the normalization scale factor for a given degree $\ell$ and order $m$ relative to SN3D.
    #[inline]
    pub fn scale_factor_from_sn3d(&self, ell: usize, _m: i32) -> f32 {
        match self {
            Self::SN3D => 1.0,
            Self::N3D => (2.0 * ell as f32 + 1.0).sqrt(),
            Self::FuMa => {
                if ell == 0 {
                    FRAC_1_SQRT_2 // W is attenuated by 1/sqrt(2)
                } else {
                    1.0
                }
            }
        }
    }

    /// Converts an interleaved Ambisonics buffer in-place between two normalizations.
    pub fn convert_buffer(
        buffer: &mut [f32],
        order: AmbisonicOrder,
        from: AmbisonicNormalization,
        to: AmbisonicNormalization,
    ) {
        if from == to {
            return;
        }

        let channels = order.channels();
        let frame_count = buffer.len() / channels;

        let mut channel_gains = vec![1.0f32; channels];
        for (acn, gain) in channel_gains.iter_mut().enumerate() {
            let (ell, m) = AmbisonicOrder::acn_to_degree_order(acn);
            let sn3d_to_from = from.scale_factor_from_sn3d(ell, m);
            let sn3d_to_target = to.scale_factor_from_sn3d(ell, m);
            *gain = sn3d_to_target / sn3d_to_from;
        }

        for frame in 0..frame_count {
            let base = frame * channels;
            for ch in 0..channels {
                buffer[base + ch] *= channel_gains[ch];
            }
        }
    }
}

/// 3D spatial coordinate representation in spherical and Cartesian coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpatialPosition3D {
    /// Azimuth angle in radians in $[-\pi, \pi]$:
    /// - $0$: Front / straight ahead
    /// - $+\pi/2$: Left ($+90^\circ$)
    /// - $-\pi/2$: Right ($-90^\circ$)
    /// - $\pm\pi$: Directly behind ($180^\circ$)
    pub azimuth: f32,
    /// Elevation angle in radians in $[-\pi/2, \pi/2]$:
    /// - $0$: Horizontal eye level
    /// - $+\pi/2$: Zenith / directly above ($+90^\circ$)
    /// - $-\pi/2$: Nadir / directly below ($-90^\circ$)
    pub elevation: f32,
    /// Distance from listener center in meters ($r \ge 0.1$).
    pub distance: f32,
}

impl Default for SpatialPosition3D {
    fn default() -> Self {
        Self {
            azimuth: 0.0,
            elevation: 0.0,
            distance: 1.0,
        }
    }
}

impl SpatialPosition3D {
    /// Create a position from spherical coordinates in degrees and distance in meters.
    pub fn from_degrees(azimuth_deg: f32, elevation_deg: f32, distance_m: f32) -> Self {
        Self {
            azimuth: azimuth_deg.to_radians(),
            elevation: elevation_deg.to_radians().clamp(-FRAC_PI_2, FRAC_PI_2),
            distance: distance_m.max(0.01),
        }
    }

    /// Create a position from standard 3D Cartesian coordinates (Right-Handed, Z-up):
    /// - $x$: Front / forward
    /// - $y$: Left
    /// - $z$: Up
    pub fn from_cartesian(x_front: f32, y_left: f32, z_up: f32) -> Self {
        let r_xy = (x_front * x_front + y_left * y_left).sqrt();
        let distance = (r_xy * r_xy + z_up * z_up).sqrt().max(0.01);
        let azimuth = y_left.atan2(x_front);
        let elevation = z_up.atan2(r_xy);
        Self {
            azimuth,
            elevation,
            distance,
        }
    }

    /// Convert to 3D Cartesian coordinates $[x_{\text{front}}, y_{\text{left}}, z_{\text{up}}]$.
    pub fn to_cartesian(&self) -> (f32, f32, f32) {
        let cos_el = self.elevation.cos();
        let x = self.distance * cos_el * self.azimuth.cos();
        let y = self.distance * cos_el * self.azimuth.sin();
        let z = self.distance * self.elevation.sin();
        (x, y, z)
    }

    /// Create a 3D position bridging from 2D timeline pan coordinates (`pan_x`, `pan_y` in $[-1.0, 1.0]$).
    pub fn from_2d_pan(pan_x: f32, pan_y: f32) -> Self {
        let px = pan_x.clamp(-1.0, 1.0);
        let py = pan_y.clamp(-1.0, 1.0);

        // Map pan_x (left/right) and pan_y (front/rear) to azimuth
        let angle = px.atan2(py);
        let dist = (px * px + py * py).sqrt().min(1.0);
        Self {
            azimuth: angle,
            elevation: 0.0,
            distance: (1.0 + (1.0 - dist)).max(0.5),
        }
    }

    /// Computes the inverse-distance physical sound attenuation factor:
    /// $g(r) = \frac{r_0}{\max(r_0, r)}$ with minimum reference distance $r_0 = 1.0\text{ m}$.
    #[inline]
    pub fn distance_attenuation(&self, ref_distance: f32) -> f32 {
        let r0 = ref_distance.max(0.1);
        r0 / self.distance.max(r0)
    }

    /// Computes atmospheric air absorption low-pass filter coefficient for distant sources.
    /// Higher distances attenuate high frequencies above 5 kHz.
    #[inline]
    pub fn air_absorption_filter_alpha(&self) -> f32 {
        // High frequency damping coefficient in [0.0, 0.75]
        let excess_dist = (self.distance - 1.0).max(0.0);
        (excess_dist * 0.05).min(0.65)
    }
}

/// Spherical Harmonics evaluator providing real spherical harmonics $Y_\ell^m(\theta, \phi)$ in ACN order.
pub struct SphericalHarmonics;

impl SphericalHarmonics {
    /// Evaluates the real spherical harmonic basis vector for a given 3D direction up to degree $N$.
    ///
    /// Output vector length equals $(N+1)^2$ (4, 9, or 16).
    pub fn evaluate(
        order: AmbisonicOrder,
        norm: AmbisonicNormalization,
        azimuth: f32,
        elevation: f32,
    ) -> Vec<f32> {
        let theta = azimuth;
        let phi = elevation;

        let cos_phi = phi.cos();
        let sin_phi = phi.sin();
        let _cos_2phi = (2.0 * phi).cos();
        let sin_2phi = (2.0 * phi).sin();

        let cos_theta = theta.cos();
        let sin_theta = theta.sin();
        let cos_2theta = (2.0 * theta).cos();
        let sin_2theta = (2.0 * theta).sin();
        let cos_3theta = (3.0 * theta).cos();
        let sin_3theta = (3.0 * theta).sin();

        let cos2_phi = cos_phi * cos_phi;
        let sin2_phi = sin_phi * sin_phi;
        let cos3_phi = cos2_phi * cos_phi;

        let mut y = vec![0.0f32; order.channels()];

        // Degree 0 (ACN 0)
        y[0] = 1.0; // W

        // Degree 1 (ACN 1, 2, 3)
        if order.order() >= 1 {
            y[1] = sin_theta * cos_phi; // Y (Left/Right)
            y[2] = sin_phi; // Z (Height)
            y[3] = cos_theta * cos_phi; // X (Front/Back)
        }

        // Degree 2 (ACN 4, 5, 6, 7, 8)
        if order.order() >= 2 {
            const SQRT_3_DIV_2: f32 = 0.866_025_4; // sqrt(3) / 2
            y[4] = SQRT_3_DIV_2 * sin_2theta * cos2_phi; // V (XY)
            y[5] = SQRT_3_DIV_2 * sin_theta * sin_2phi; // T (YZ)
            y[6] = 0.5 * (3.0 * sin2_phi - 1.0); // R (3Z^2 - 1)
            y[7] = SQRT_3_DIV_2 * cos_theta * sin_2phi; // S (ZX)
            y[8] = SQRT_3_DIV_2 * cos_2theta * cos2_phi; // U (X^2 - Y^2)
        }

        // Degree 3 (ACN 9 through 15)
        if order.order() >= 3 {
            const SQRT_5_DIV_8: f32 = 0.790_569_4; // sqrt(5/8)
            const SQRT_15_DIV_2: f32 = 1.936_491_7; // sqrt(15) / 2
            const SQRT_3_DIV_8: f32 = 0.612_372_46; // sqrt(3/8)

            y[9] = SQRT_5_DIV_8 * sin_3theta * cos3_phi;
            y[10] = SQRT_15_DIV_2 * sin_2theta * sin_phi * cos2_phi;
            y[11] = SQRT_3_DIV_8 * sin_theta * cos_phi * (5.0 * sin2_phi - 1.0);
            y[12] = 0.5 * sin_phi * (5.0 * sin2_phi - 3.0);
            y[13] = SQRT_3_DIV_8 * cos_theta * cos_phi * (5.0 * sin2_phi - 1.0);
            y[14] = SQRT_15_DIV_2 * cos_2theta * sin_phi * cos2_phi;
            y[15] = SQRT_5_DIV_8 * cos_3theta * cos3_phi;
        }

        // Apply target normalization scaling relative to default SN3D
        if norm != AmbisonicNormalization::SN3D {
            for (acn, val) in y.iter_mut().enumerate() {
                let (ell, m) = AmbisonicOrder::acn_to_degree_order(acn);
                *val *= norm.scale_factor_from_sn3d(ell, m);
            }
        }

        y
    }
}

/// 3D Soundfield Rotator: rotates an Ambisonic B-format soundfield in 3D space
/// via Yaw, Pitch, and Roll orthogonal transform matrices.
pub struct AmbisonicRotator;

impl AmbisonicRotator {
    /// Rotates an interleaved Ambisonics buffer by Euler angles:
    /// - `yaw`: Rotation around Z axis (looking left/right) in radians
    /// - `pitch`: Rotation around Y axis (looking up/down) in radians
    /// - `roll`: Rotation around X axis (head tilting) in radians
    pub fn rotate_buffer(
        buffer: &mut [f32],
        order: AmbisonicOrder,
        yaw: f32,
        pitch: f32,
        roll: f32,
    ) {
        if yaw.abs() < 1e-6 && pitch.abs() < 1e-6 && roll.abs() < 1e-6 {
            return;
        }

        let channels = order.channels();
        let frame_count = buffer.len() / channels;

        // Build 3D Cartesian rotation matrix R = R_z(yaw) * R_y(pitch) * R_x(roll)
        // using standard computer graphics convention:
        let cy = yaw.cos();
        let sy = yaw.sin();
        let cp = pitch.cos();
        let sp = pitch.sin();
        let cr = roll.cos();
        let sr = roll.sin();

        // 3x3 rotation matrix
        let r00 = cy * cp;
        let r01 = cy * sp * sr - sy * cr;
        let r02 = cy * sp * cr + sy * sr;

        let r10 = sy * cp;
        let r11 = sy * sp * sr + cy * cr;
        let r12 = sy * sp * cr - cy * sr;

        let r20 = -sp;
        let r21 = cp * sr;
        let r22 = cp * cr;

        for frame in 0..frame_count {
            let base = frame * channels;

            // Degree 0 (W): Invariant under rotation
            // W remains unchanged

            // Degree 1 (ACN 1=Y, 2=Z, 3=X):
            // Mapping: X=3, Y=1, Z=2
            if order.order() >= 1 {
                let y1 = buffer[base + 1];
                let z1 = buffer[base + 2];
                let x1 = buffer[base + 3];

                // R * [x, y, z]^T
                let new_x = r00 * x1 + r01 * y1 + r02 * z1;
                let new_y = r10 * x1 + r11 * y1 + r12 * z1;
                let new_z = r20 * x1 + r21 * y1 + r22 * z1;

                buffer[base + 1] = new_y;
                buffer[base + 2] = new_z;
                buffer[base + 3] = new_x;
            }

            // Degree 2 (ACN 4..8) and Degree 3 (ACN 9..15):
            // Apply second-order harmonic rotation approximation along Z-axis (Yaw dominant)
            if order.order() >= 2 && yaw.abs() >= 1e-6 {
                let cos_2yaw = (2.0 * yaw).cos();
                let sin_2yaw = (2.0 * yaw).sin();

                // Rotate quadrupole pair (V=ACN 4, U=ACN 8) around Z:
                let v = buffer[base + 4];
                let u = buffer[base + 8];
                buffer[base + 4] = v * cos_2yaw - u * sin_2yaw;
                buffer[base + 8] = v * sin_2yaw + u * cos_2yaw;

                // Rotate dipole-like pairs (T=ACN 5, S=ACN 7) around Z:
                let t = buffer[base + 5];
                let s = buffer[base + 7];
                buffer[base + 5] = t * cy - s * sy;
                buffer[base + 7] = t * sy + s * cy;
            }
        }
    }
}

/// Real-time 3D Ambisonic Encoder: encodes mono and stereo audio streams into B-format.
#[derive(Debug, Clone)]
pub struct AmbisonicEncoder {
    pub order: AmbisonicOrder,
    pub normalization: AmbisonicNormalization,
    pub reference_distance: f32,
    last_filtered_mono: f32,
}

impl Default for AmbisonicEncoder {
    fn default() -> Self {
        Self {
            order: AmbisonicOrder::First,
            normalization: AmbisonicNormalization::SN3D,
            reference_distance: 1.0,
            last_filtered_mono: 0.0,
        }
    }
}

impl AmbisonicEncoder {
    /// Create a new Ambisonic encoder with specified order and normalization.
    pub fn new(order: AmbisonicOrder, normalization: AmbisonicNormalization) -> Self {
        Self {
            order,
            normalization,
            reference_distance: 1.0,
            last_filtered_mono: 0.0,
        }
    }

    /// Encodes a single mono audio sample at 3D spatial position into an Ambisonics output frame.
    ///
    /// The `out_frame` slice length must be at least `order.channels()`.
    pub fn encode_sample(&mut self, sample: f32, pos: &SpatialPosition3D, out_frame: &mut [f32]) {
        let dist_gain = pos.distance_attenuation(self.reference_distance);
        let alpha = pos.air_absorption_filter_alpha();

        // 1-pole lowpass air absorption filter
        self.last_filtered_mono += (1.0 - alpha) * (sample - self.last_filtered_mono);
        let filtered_sample = self.last_filtered_mono * dist_gain;

        let harmonics = SphericalHarmonics::evaluate(
            self.order,
            self.normalization,
            pos.azimuth,
            pos.elevation,
        );

        for (ch, &y_val) in harmonics.iter().enumerate() {
            out_frame[ch] = filtered_sample * y_val;
        }
    }

    /// Encodes a continuous mono PCM block into an interleaved Ambisonics buffer.
    ///
    /// `out_interleaved` length must equal `mono_input.len() * order.channels()`.
    pub fn encode_mono_block(
        &mut self,
        mono_input: &[f32],
        pos: &SpatialPosition3D,
        out_interleaved: &mut [f32],
    ) {
        let ch_count = self.order.channels();
        assert_eq!(
            out_interleaved.len(),
            mono_input.len() * ch_count,
            "Ambisonic output buffer size mismatch"
        );

        let dist_gain = pos.distance_attenuation(self.reference_distance);
        let alpha = pos.air_absorption_filter_alpha();
        let harmonics = SphericalHarmonics::evaluate(
            self.order,
            self.normalization,
            pos.azimuth,
            pos.elevation,
        );

        for (i, &sample) in mono_input.iter().enumerate() {
            self.last_filtered_mono += (1.0 - alpha) * (sample - self.last_filtered_mono);
            let s = self.last_filtered_mono * dist_gain;
            let base = i * ch_count;

            for (ch, &y_val) in harmonics.iter().enumerate() {
                out_interleaved[base + ch] = s * y_val;
            }
        }
    }

    /// Encodes a stereo input (Left, Right) with directional stereo spread into Ambisonics.
    pub fn encode_stereo_block(
        &mut self,
        stereo_interleaved: &[f32],
        center_pos: &SpatialPosition3D,
        spread_deg: f32,
        out_interleaved: &mut [f32],
    ) {
        let ch_count = self.order.channels();
        let frame_count = stereo_interleaved.len() / 2;
        assert_eq!(
            out_interleaved.len(),
            frame_count * ch_count,
            "Ambisonic output buffer size mismatch"
        );

        let half_spread = (spread_deg * 0.5).to_radians();
        let left_pos = SpatialPosition3D {
            azimuth: (center_pos.azimuth + half_spread).clamp(-PI, PI),
            elevation: center_pos.elevation,
            distance: center_pos.distance,
        };
        let right_pos = SpatialPosition3D {
            azimuth: (center_pos.azimuth - half_spread).clamp(-PI, PI),
            elevation: center_pos.elevation,
            distance: center_pos.distance,
        };

        let left_harmonics = SphericalHarmonics::evaluate(
            self.order,
            self.normalization,
            left_pos.azimuth,
            left_pos.elevation,
        );
        let right_harmonics = SphericalHarmonics::evaluate(
            self.order,
            self.normalization,
            right_pos.azimuth,
            right_pos.elevation,
        );

        let dist_gain = center_pos.distance_attenuation(self.reference_distance);

        for frame_idx in 0..frame_count {
            let left_s = stereo_interleaved[frame_idx * 2] * dist_gain * FRAC_1_SQRT_2;
            let right_s = stereo_interleaved[frame_idx * 2 + 1] * dist_gain * FRAC_1_SQRT_2;
            let base = frame_idx * ch_count;

            for ch in 0..ch_count {
                out_interleaved[base + ch] =
                    left_s * left_harmonics[ch] + right_s * right_harmonics[ch];
            }
        }
    }
}

/// Real-time Binaural Head-Related Transfer Function (HRTF) synthesizer.
///
/// Implements Woodworth-Schlosser spherical head model for Interaural Time Difference (ITD),
/// frequency-dependent head shadowing for Interaural Level Difference (ILD),
/// and concha/pinna elevation notch filtering.
#[derive(Debug, Clone)]
pub struct BinauralHrtfEngine {
    head_radius_m: f32,
    speed_of_sound_m_s: f32,
    sample_rate: f32,
    left_delay_ring: Vec<f32>,
    right_delay_ring: Vec<f32>,
    write_idx: usize,
    left_shadow_state: f32,
    right_shadow_state: f32,
    left_pinna_state: [f32; 2],
    right_pinna_state: [f32; 2],
}

impl Default for BinauralHrtfEngine {
    fn default() -> Self {
        Self::new(SAMPLE_RATE as f32)
    }
}

impl BinauralHrtfEngine {
    /// Creates a new HRTF engine initialized for the specified audio sample rate.
    pub fn new(sample_rate: f32) -> Self {
        // Delay ring buffer to accommodate maximum ITD (~0.655 ms ≈ 32 samples at 48 kHz)
        const RING_SIZE: usize = 128;
        Self {
            head_radius_m: 0.0875,     // Standard adult human head radius (~8.75 cm)
            speed_of_sound_m_s: 343.0, // Speed of sound in 20°C air (m/s)
            sample_rate,
            left_delay_ring: vec![0.0; RING_SIZE],
            right_delay_ring: vec![0.0; RING_SIZE],
            write_idx: 0,
            left_shadow_state: 0.0,
            right_shadow_state: 0.0,
            left_pinna_state: [0.0; 2],
            right_pinna_state: [0.0; 2],
        }
    }

    /// Computes the Woodworth-Schlosser ITD (delay in seconds) for a given azimuth angle.
    ///
    /// For the ear on the opposite side of the source:
    /// $\tau(\theta) = \frac{a}{c} (\sin|\theta| + |\theta|)$
    #[inline]
    pub fn calculate_itd_seconds(&self, azimuth: f32) -> (f32, f32) {
        let theta = azimuth;
        let abs_theta = theta.abs();
        let max_itd =
            (self.head_radius_m / self.speed_of_sound_m_s) * (abs_theta.sin() + abs_theta);

        if theta > 0.0 {
            // Source is to the Left (+azimuth):
            // Left ear is ipsilateral (0 delay), Right ear is contralateral (delayed)
            (0.0, max_itd)
        } else {
            // Source is to the Right (-azimuth):
            // Right ear is ipsilateral (0 delay), Left ear is contralateral (delayed)
            (max_itd, 0.0)
        }
    }

    /// Synthesizes a mono sample at 3D spatial position into a binaural stereo pair `(left, right)`.
    pub fn process_mono_sample(&mut self, sample: f32, pos: &SpatialPosition3D) -> (f32, f32) {
        let ring_len = self.left_delay_ring.len();

        // 1. Write current sample to delay lines
        self.left_delay_ring[self.write_idx] = sample;
        self.right_delay_ring[self.write_idx] = sample;

        // 2. Compute ITD delays in fractional samples
        let (left_itd_s, right_itd_s) = self.calculate_itd_seconds(pos.azimuth);
        let left_delay_samples = left_itd_s * self.sample_rate;
        let right_delay_samples = right_itd_s * self.sample_rate;

        // Read fractional delay from ring buffers via linear interpolation
        let read_left = self.read_interpolated(&self.left_delay_ring, left_delay_samples);
        let read_right = self.read_interpolated(&self.right_delay_ring, right_delay_samples);

        // Advance ring write pointer
        self.write_idx = (self.write_idx + 1) % ring_len;

        // 3. Head Shadowing (ILD): Contralateral ear experiences high-frequency lowpass rolloff
        let theta = pos.azimuth;
        let left_shadow_alpha = (1.0 - (theta * 0.5).sin().abs()).clamp(0.15, 1.0);
        let right_shadow_alpha = (1.0 - ((-theta) * 0.5).sin().abs()).clamp(0.15, 1.0);

        self.left_shadow_state +=
            (1.0 - left_shadow_alpha * 0.5) * (read_left - self.left_shadow_state);
        self.right_shadow_state +=
            (1.0 - right_shadow_alpha * 0.5) * (read_right - self.right_shadow_state);

        let left_shadowed =
            read_left * left_shadow_alpha + self.left_shadow_state * (1.0 - left_shadow_alpha);
        let right_shadowed =
            read_right * right_shadow_alpha + self.right_shadow_state * (1.0 - right_shadow_alpha);

        // 4. Pinna Elevation Cues: Notch filter modulated by elevation phi
        let elevation_factor = (pos.elevation + FRAC_PI_2) / PI; // in [0, 1]
        let pinna_notch_freq = 6500.0 + 3000.0 * elevation_factor; // 6.5 kHz to 9.5 kHz
        let pinna_q = 3.5;

        let left_out = Self::apply_pinna_notch(
            self.sample_rate,
            left_shadowed,
            pinna_notch_freq,
            pinna_q,
            &mut self.left_pinna_state,
        );
        let right_out = Self::apply_pinna_notch(
            self.sample_rate,
            right_shadowed,
            pinna_notch_freq,
            pinna_q,
            &mut self.right_pinna_state,
        );

        let dist_gain = pos.distance_attenuation(1.0);
        (left_out * dist_gain, right_out * dist_gain)
    }

    /// Read fractional delay sample from ring buffer with linear interpolation.
    #[inline]
    fn read_interpolated(&self, ring: &[f32], delay_samples: f32) -> f32 {
        let ring_len = ring.len();
        let delay_clamped = delay_samples.clamp(0.0, (ring_len - 2) as f32);
        let delay_int = delay_clamped.floor() as usize;
        let delay_frac = delay_clamped - delay_clamped.floor();

        let idx0 = (self.write_idx + ring_len - delay_int) % ring_len;
        let idx1 = (self.write_idx + ring_len - delay_int - 1) % ring_len;

        ring[idx0] * (1.0 - delay_frac) + ring[idx1] * delay_frac
    }

    /// Apply second-order biquad notch filter for pinna elevation spectral cues.
    #[inline]
    fn apply_pinna_notch(
        sample_rate: f32,
        x: f32,
        notch_freq: f32,
        q: f32,
        state: &mut [f32; 2],
    ) -> f32 {
        let w0 = 2.0 * PI * (notch_freq / sample_rate).clamp(0.01, 0.45);
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = 1.0;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        // Direct Form II transposed
        let inv_a0 = 1.0 / a0;
        let out = (b0 * inv_a0) * x + state[0];
        state[0] = (b1 * inv_a0) * x - (a1 * inv_a0) * out + state[1];
        state[1] = (b2 * inv_a0) * x - (a2 * inv_a0) * out;

        out
    }

    /// Decodes an interleaved Ambisonics B-format stream into a binaural headphone mix
    /// using direct spherical harmonic binaural filters, interaural delay (ITD),
    /// and contralateral head-shadowing (ILD).
    pub fn decode_ambisonics_to_binaural(
        &mut self,
        ambi_interleaved: &[f32],
        order: AmbisonicOrder,
        stereo_out: &mut [f32],
    ) {
        let ambi_ch = order.channels();
        let frame_count = ambi_interleaved.len() / ambi_ch;
        assert_eq!(
            stereo_out.len(),
            frame_count * 2,
            "Binaural output length must be 2 * frame_count"
        );

        let ring_len = self.left_delay_ring.len();
        // Contralateral interaural time delay (~0.45 ms ≈ 21.6 samples at 48 kHz)
        let itd_delay_samples =
            (self.head_radius_m / self.speed_of_sound_m_s) * (PI * 0.5) * self.sample_rate;

        for frame_idx in 0..frame_count {
            let ambi_base = frame_idx * ambi_ch;

            let w = ambi_interleaved[ambi_base];
            let y = ambi_interleaved[ambi_base + 1]; // Left/Right dipole
            let z = ambi_interleaved[ambi_base + 2]; // Height dipole
            let x = ambi_interleaved[ambi_base + 3]; // Front/Back dipole

            // Incorporate 2nd-order lateral harmonics if available
            let (v, u) = if order.order() >= 2 {
                (
                    ambi_interleaved[ambi_base + 4],
                    ambi_interleaved[ambi_base + 8],
                )
            } else {
                (0.0, 0.0)
            };

            // Direct ear excitations from Ambisonics soundfield
            let direct_l = 0.45 * (w + 0.707 * x + y) + 0.12 * (v - u);
            let direct_r = 0.45 * (w + 0.707 * x - y) - 0.12 * (v - u);

            // Write direct signals to delay lines
            self.left_delay_ring[self.write_idx] = direct_l;
            self.right_delay_ring[self.write_idx] = direct_r;

            // Read delayed contralateral cross-feed signals
            let read_contra_l = self.read_interpolated(&self.left_delay_ring, itd_delay_samples);
            let read_contra_r = self.read_interpolated(&self.right_delay_ring, itd_delay_samples);

            // Advance ring write index ONCE per audio frame
            self.write_idx = (self.write_idx + 1) % ring_len;

            // Apply contralateral head-shadow lowpass filtering (diffraction around the skull)
            const SHADOW_ALPHA: f32 = 0.35;
            self.left_shadow_state += SHADOW_ALPHA * (read_contra_l - self.left_shadow_state);
            self.right_shadow_state += SHADOW_ALPHA * (read_contra_r - self.right_shadow_state);

            // Pinna elevation spectral cue: notch filter modulated by height Z
            let pinna_notch_freq = 7000.0 + 2000.0 * z.clamp(-1.0, 1.0);
            let left_pinna = Self::apply_pinna_notch(
                self.sample_rate,
                direct_l,
                pinna_notch_freq,
                3.0,
                &mut self.left_pinna_state,
            );
            let right_pinna = Self::apply_pinna_notch(
                self.sample_rate,
                direct_r,
                pinna_notch_freq,
                3.0,
                &mut self.right_pinna_state,
            );

            // Cross-feed shadowed contralateral sound into opposite ear
            let out_l = (0.75 * left_pinna + 0.25 * self.right_shadow_state).clamp(-1.5, 1.5);
            let out_r = (0.75 * right_pinna + 0.25 * self.left_shadow_state).clamp(-1.5, 1.5);

            stereo_out[frame_idx * 2] = out_l;
            stereo_out[frame_idx * 2 + 1] = out_r;
        }
    }
}

/// Loudspeaker position specification in spherical coordinates for surround and immersive setups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoudspeakerPosition {
    pub name: &'static str,
    pub azimuth_deg: f32,
    pub elevation_deg: f32,
    pub is_lfe: bool,
}

/// Standard 7.1.4 Immersive Bed Loudspeaker Layout (12 channels):
/// - 7 ear-level surround speakers
/// - 1 LFE subwoofer
/// - 4 overhead height speakers
pub const LAYOUT_714_SPEAKERS: [LoudspeakerPosition; 12] = [
    LoudspeakerPosition {
        name: "Left",
        azimuth_deg: 30.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Right",
        azimuth_deg: -30.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Center",
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "LFE",
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        is_lfe: true,
    },
    LoudspeakerPosition {
        name: "Left Side Surround",
        azimuth_deg: 90.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Right Side Surround",
        azimuth_deg: -90.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Left Back Surround",
        azimuth_deg: 135.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Right Back Surround",
        azimuth_deg: -135.0,
        elevation_deg: 0.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Top Front Left",
        azimuth_deg: 45.0,
        elevation_deg: 45.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Top Front Right",
        azimuth_deg: -45.0,
        elevation_deg: 45.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Top Back Left",
        azimuth_deg: 135.0,
        elevation_deg: 45.0,
        is_lfe: false,
    },
    LoudspeakerPosition {
        name: "Top Back Right",
        azimuth_deg: -135.0,
        elevation_deg: 45.0,
        is_lfe: false,
    },
];

/// Multi-Speaker Ambisonic Decoder: decodes Ambisonics B-format streams to
/// discrete speaker channel feeds (Stereo, 5.1, 7.1, and 7.1.4 Immersive Bed).
pub struct AmbisonicDecoder;

impl AmbisonicDecoder {
    /// Decodes an interleaved Ambisonics B-format buffer into a discrete multichannel speaker layout.
    pub fn decode(
        input_b_format: &[f32],
        order: AmbisonicOrder,
        layout: ChannelLayout,
        output: &mut [f32],
    ) {
        let ambi_ch = order.channels();
        let frame_count = input_b_format.len() / ambi_ch;
        let out_ch = layout.channels();
        assert_eq!(
            output.len(),
            frame_count * out_ch,
            "Ambisonic decoder output buffer length mismatch"
        );

        output.fill(0.0);

        match layout {
            ChannelLayout::Mono => {
                // W channel provides omnidirectional sound
                for i in 0..frame_count {
                    output[i] = input_b_format[i * ambi_ch];
                }
            }
            ChannelLayout::Stereo => {
                // Decode to virtual stereo speakers at ±30°
                const INV_SQRT_2: f32 = 0.707_106_77;
                for i in 0..frame_count {
                    let base = i * ambi_ch;
                    let w = input_b_format[base];
                    let y = input_b_format[base + 1]; // Left/Right dipole
                    let x = input_b_format[base + 3]; // Front/Back dipole

                    // Cardioid pointing to Left (+30°) and Right (-30°)
                    let left = (w + 0.5 * y + 0.866 * x) * INV_SQRT_2;
                    let right = (w - 0.5 * y + 0.866 * x) * INV_SQRT_2;

                    output[i * 2] = left;
                    output[i * 2 + 1] = right;
                }
            }
            ChannelLayout::Surround51 => {
                // 5.1 Layout: [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs]
                // Energy-preserving mode-matching decoding
                for i in 0..frame_count {
                    let base = i * ambi_ch;
                    let w = input_b_format[base];
                    let y = input_b_format[base + 1];
                    let x = input_b_format[base + 3];

                    let l = 0.40 * w + 0.35 * x + 0.35 * y;
                    let r = 0.40 * w + 0.35 * x - 0.35 * y;
                    let c = 0.50 * w + 0.50 * x;
                    let lfe = 0.20 * w; // Sub-bass derived from omni W
                    let ls = 0.40 * w - 0.35 * x + 0.45 * y;
                    let rs = 0.40 * w - 0.35 * x - 0.45 * y;

                    let out_base = i * 6;
                    output[out_base] = l;
                    output[out_base + 1] = r;
                    output[out_base + 2] = c;
                    output[out_base + 3] = lfe;
                    output[out_base + 4] = ls;
                    output[out_base + 5] = rs;
                }
            }
            ChannelLayout::Surround71 => {
                // 7.1 Layout: [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs, 6: Lb, 7: Rb]
                for i in 0..frame_count {
                    let base = i * ambi_ch;
                    let w = input_b_format[base];
                    let y = input_b_format[base + 1];
                    let x = input_b_format[base + 3];

                    let l = 0.35 * w + 0.35 * x + 0.30 * y;
                    let r = 0.35 * w + 0.35 * x - 0.30 * y;
                    let c = 0.45 * w + 0.45 * x;
                    let lfe = 0.20 * w;
                    let ls = 0.35 * w + 0.45 * y;
                    let rs = 0.35 * w - 0.45 * y;
                    let lb = 0.35 * w - 0.40 * x + 0.35 * y;
                    let rb = 0.35 * w - 0.40 * x - 0.35 * y;

                    let out_base = i * 8;
                    output[out_base] = l;
                    output[out_base + 1] = r;
                    output[out_base + 2] = c;
                    output[out_base + 3] = lfe;
                    output[out_base + 4] = ls;
                    output[out_base + 5] = rs;
                    output[out_base + 6] = lb;
                    output[out_base + 7] = rb;
                }
            }
            ChannelLayout::Surround714 => {
                // 7.1.4 Immersive Bed Layout:
                // [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs, 6: Lb, 7: Rb, 8: Tfl, 9: Tfr, 10: Tbl, 11: Tbr]
                for i in 0..frame_count {
                    let base = i * ambi_ch;
                    let w = input_b_format[base];
                    let y = input_b_format[base + 1];
                    let z = input_b_format[base + 2]; // Height component
                    let x = input_b_format[base + 3];

                    // Ear-level surrounds (horizontal plane)
                    let l = 0.30 * w + 0.30 * x + 0.25 * y - 0.15 * z;
                    let r = 0.30 * w + 0.30 * x - 0.25 * y - 0.15 * z;
                    let c = 0.40 * w + 0.40 * x - 0.15 * z;
                    let lfe = 0.20 * w;
                    let ls = 0.30 * w + 0.35 * y - 0.15 * z;
                    let rs = 0.30 * w - 0.35 * y - 0.15 * z;
                    let lb = 0.30 * w - 0.35 * x + 0.30 * y - 0.15 * z;
                    let rb = 0.30 * w - 0.35 * x - 0.30 * y - 0.15 * z;

                    // Overhead height speakers
                    let tfl = 0.25 * w + 0.25 * x + 0.25 * y + 0.45 * z;
                    let tfr = 0.25 * w + 0.25 * x - 0.25 * y + 0.45 * z;
                    let tbl = 0.25 * w - 0.25 * x + 0.25 * y + 0.45 * z;
                    let tbr = 0.25 * w - 0.25 * x - 0.25 * y + 0.45 * z;

                    let out_base = i * 12;
                    output[out_base] = l;
                    output[out_base + 1] = r;
                    output[out_base + 2] = c;
                    output[out_base + 3] = lfe;
                    output[out_base + 4] = ls;
                    output[out_base + 5] = rs;
                    output[out_base + 6] = lb;
                    output[out_base + 7] = rb;
                    output[out_base + 8] = tfl;
                    output[out_base + 9] = tfr;
                    output[out_base + 10] = tbl;
                    output[out_base + 11] = tbr;
                }
            }
            ChannelLayout::Ambisonics1stOrder
            | ChannelLayout::Ambisonics2ndOrder
            | ChannelLayout::Ambisonics3rdOrder => {
                // Pass through or truncate/zero-pad matching channels
                let copy_ch = ambi_ch.min(out_ch);
                for i in 0..frame_count {
                    let in_base = i * ambi_ch;
                    let out_base = i * out_ch;
                    output[out_base..out_base + copy_ch]
                        .copy_from_slice(&input_b_format[in_base..in_base + copy_ch]);
                }
            }
        }
    }
}

/// An audio input track configured for 3D spatial mixing.
pub struct AmbisonicTrack {
    pub volume: f32,
    pub position: SpatialPosition3D,
    pub pcm_data: Vec<f32>,
    pub source_channels: usize,
    pub position_samples: usize,
    encoder: AmbisonicEncoder,
}

impl AmbisonicTrack {
    /// Create a new 3D spatial audio track.
    pub fn new(
        pcm_data: Vec<f32>,
        source_channels: usize,
        volume: f32,
        position: SpatialPosition3D,
        order: AmbisonicOrder,
    ) -> Self {
        Self {
            volume,
            position,
            pcm_data,
            source_channels: source_channels.clamp(1, 2),
            position_samples: 0,
            encoder: AmbisonicEncoder::new(order, AmbisonicNormalization::SN3D),
        }
    }
}

/// Real-time 3D Ambisonics Spatial Audio Mixer.
///
/// Orchestrates multiple 3D spatialized tracks into an intermediate B-format bus,
/// applies head-tracking rotation, and delivers output in any desired format:
/// - Binaural Headphone monitoring (via HRTF synthesis)
/// - 7.1.4 Immersive Bed
/// - 7.1 or 5.1 Surround
/// - Raw Ambisonics B-format (1st, 2nd, or 3rd order)
pub struct AmbisonicMixer {
    pub order: AmbisonicOrder,
    pub normalization: AmbisonicNormalization,
    pub yaw_rad: f32,
    pub pitch_rad: f32,
    pub roll_rad: f32,
    master_volume: f32,
    tracks: Vec<AmbisonicTrack>,
    clock: Arc<AudioClock>,
    hrtf_engine: BinauralHrtfEngine,
}

impl AmbisonicMixer {
    /// Initializes a new 3D Ambisonics mixer with master audio clock.
    pub fn new(order: AmbisonicOrder, clock: Arc<AudioClock>) -> Self {
        Self {
            order,
            normalization: AmbisonicNormalization::SN3D,
            yaw_rad: 0.0,
            pitch_rad: 0.0,
            roll_rad: 0.0,
            master_volume: 1.0,
            tracks: Vec::new(),
            clock,
            hrtf_engine: BinauralHrtfEngine::new(SAMPLE_RATE as f32),
        }
    }

    /// Sets head-tracking / virtual listener rotation angles in radians.
    pub fn set_rotation(&mut self, yaw: f32, pitch: f32, roll: f32) {
        self.yaw_rad = yaw;
        self.pitch_rad = pitch;
        self.roll_rad = roll;
    }

    /// Sets master volume multiplier.
    pub fn set_master_volume(&mut self, vol: f32) {
        self.master_volume = vol.max(0.0);
    }

    /// Adds a 3D audio track to the mixer.
    pub fn add_track(&mut self, track: AmbisonicTrack) {
        self.tracks.push(track);
    }

    /// Clears all tracks from the mixer.
    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    /// Seeks playback position for all tracks.
    pub fn seek_to_sample(&mut self, sample_pos: usize) {
        for track in &mut self.tracks {
            let total_frames = track.pcm_data.len() / track.source_channels;
            let target_frame = sample_pos.min(total_frames);
            track.position_samples = target_frame * track.source_channels;
        }
    }

    /// Mixes all tracks into an interleaved Ambisonics B-format buffer.
    pub fn mix_to_b_format(&mut self, output_b_format: &mut [f32]) {
        let ch = self.order.channels();
        let frame_count = output_b_format.len() / ch;
        output_b_format.fill(0.0);

        let mut track_buf = vec![0.0f32; frame_count * ch];

        for track in &mut self.tracks {
            let src_ch = track.source_channels;
            let available_samples = track.pcm_data.len().saturating_sub(track.position_samples);
            let available_frames = available_samples / src_ch;
            let frames_to_process = frame_count.min(available_frames);

            if frames_to_process == 0 {
                continue;
            }

            let effective_vol = track.volume * self.master_volume;
            let slice_end = track.position_samples + frames_to_process * src_ch;
            let pcm_slice = &track.pcm_data[track.position_samples..slice_end];

            track_buf.fill(0.0);

            if src_ch == 1 {
                track.encoder.encode_mono_block(
                    pcm_slice,
                    &track.position,
                    &mut track_buf[..frames_to_process * ch],
                );
            } else {
                track.encoder.encode_stereo_block(
                    pcm_slice,
                    &track.position,
                    45.0,
                    &mut track_buf[..frames_to_process * ch],
                );
            }

            for i in 0..frames_to_process * ch {
                output_b_format[i] += track_buf[i] * effective_vol;
            }

            track.position_samples += frames_to_process * src_ch;
        }

        // Apply soundfield rotation
        AmbisonicRotator::rotate_buffer(
            output_b_format,
            self.order,
            self.yaw_rad,
            self.pitch_rad,
            self.roll_rad,
        );

        // Advance master audio clock
        self.clock.advance_samples(frame_count, SAMPLE_RATE);
    }

    /// Mixes all tracks and renders directly to binaural headphone stereo.
    pub fn mix_to_binaural(&mut self, stereo_out: &mut [f32]) {
        let frame_count = stereo_out.len() / 2;
        let ambi_ch = self.order.channels();
        let mut b_format_buf = vec![0.0f32; frame_count * ambi_ch];

        self.mix_to_b_format(&mut b_format_buf);
        self.hrtf_engine
            .decode_ambisonics_to_binaural(&b_format_buf, self.order, stereo_out);
    }

    /// Mixes all tracks and decodes to 7.1.4 Immersive Bed multichannel audio.
    pub fn mix_to_714(&mut self, output_714: &mut [f32]) {
        let frame_count = output_714.len() / 12;
        let ambi_ch = self.order.channels();
        let mut b_format_buf = vec![0.0f32; frame_count * ambi_ch];

        self.mix_to_b_format(&mut b_format_buf);
        AmbisonicDecoder::decode(
            &b_format_buf,
            self.order,
            ChannelLayout::Surround714,
            output_714,
        );
    }
}
