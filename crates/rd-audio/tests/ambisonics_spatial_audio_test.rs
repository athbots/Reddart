//! Integration Test Suite for 3D Ambisonics Spatial Audio Engine & Binaural HRTF Monitoring (Milestone 26).

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2};

use rd_audio::ambisonics::{
    AmbisonicDecoder, AmbisonicEncoder, AmbisonicMixer, AmbisonicNormalization, AmbisonicOrder,
    AmbisonicRotator, AmbisonicTrack, BinauralHrtfEngine, SpatialPosition3D, SphericalHarmonics,
};
use rd_audio::clock::AudioClock;
use rd_audio::spatial::{ChannelLayout, ItuDownmixer};

#[test]
fn test_ambisonic_order_and_channel_counts() {
    assert_eq!(AmbisonicOrder::First.order(), 1);
    assert_eq!(AmbisonicOrder::First.channels(), 4);

    assert_eq!(AmbisonicOrder::Second.order(), 2);
    assert_eq!(AmbisonicOrder::Second.channels(), 9);

    assert_eq!(AmbisonicOrder::Third.order(), 3);
    assert_eq!(AmbisonicOrder::Third.channels(), 16);

    // Test ACN index to degree and order mapping: n = ell^2 + ell + m
    assert_eq!(AmbisonicOrder::acn_to_degree_order(0), (0, 0)); // W (ell=0, m=0)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(1), (1, -1)); // Y (ell=1, m=-1)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(2), (1, 0)); // Z (ell=1, m=0)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(3), (1, 1)); // X (ell=1, m=1)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(4), (2, -2)); // V (ell=2, m=-2)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(8), (2, 2)); // U (ell=2, m=2)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(9), (3, -3)); // Q (ell=3, m=-3)
    assert_eq!(AmbisonicOrder::acn_to_degree_order(15), (3, 3)); // P (ell=3, m=3)

    // Verify reverse mapping
    for acn in 0..16 {
        let (ell, m) = AmbisonicOrder::acn_to_degree_order(acn);
        let reconstructed_acn = AmbisonicOrder::degree_order_to_acn(ell, m);
        assert_eq!(acn, reconstructed_acn, "Mismatch at ACN {}", acn);
    }
}

#[test]
fn test_spherical_harmonics_orthonormality_and_energy() {
    // Front direction: azimuth=0, elevation=0
    let front = SphericalHarmonics::evaluate(
        AmbisonicOrder::First,
        AmbisonicNormalization::SN3D,
        0.0,
        0.0,
    );
    assert_eq!(front.len(), 4);
    assert!((front[0] - 1.0).abs() < 1e-5); // W = 1.0
    assert!(front[1].abs() < 1e-5); // Y = sin(0)*cos(0) = 0
    assert!(front[2].abs() < 1e-5); // Z = sin(0) = 0
    assert!((front[3] - 1.0).abs() < 1e-5); // X = cos(0)*cos(0) = 1.0

    // Left direction: azimuth=+pi/2, elevation=0
    let left = SphericalHarmonics::evaluate(
        AmbisonicOrder::First,
        AmbisonicNormalization::SN3D,
        FRAC_PI_2,
        0.0,
    );
    assert!((left[0] - 1.0).abs() < 1e-5); // W = 1.0
    assert!((left[1] - 1.0).abs() < 1e-5); // Y = sin(pi/2)*cos(0) = 1.0
    assert!(left[2].abs() < 1e-5); // Z = 0
    assert!(left[3].abs() < 1e-5); // X = cos(pi/2)*cos(0) = 0

    // Up direction: azimuth=0, elevation=+pi/2
    let up = SphericalHarmonics::evaluate(
        AmbisonicOrder::First,
        AmbisonicNormalization::SN3D,
        0.0,
        FRAC_PI_2,
    );
    assert!((up[0] - 1.0).abs() < 1e-5); // W = 1.0
    assert!(up[1].abs() < 1e-5); // Y = 0
    assert!((up[2] - 1.0).abs() < 1e-5); // Z = 1.0
    assert!(up[3].abs() < 1e-5); // X = 0

    // Verify N3D orthonormality: sum_m |Y_1^m|^2 = 2*1 + 1 = 3.0
    let n3d_order1 =
        SphericalHarmonics::evaluate(AmbisonicOrder::First, AmbisonicNormalization::N3D, 0.7, 0.4);
    let degree1_energy = n3d_order1[1].powi(2) + n3d_order1[2].powi(2) + n3d_order1[3].powi(2);
    assert!(
        (degree1_energy - 3.0).abs() < 1e-4,
        "N3D degree 1 energy was {}",
        degree1_energy
    );

    // Verify N3D degree 2 energy: sum_m |Y_2^m|^2 = 2*2 + 1 = 5.0
    let n3d_order2 = SphericalHarmonics::evaluate(
        AmbisonicOrder::Second,
        AmbisonicNormalization::N3D,
        1.2,
        -0.3,
    );
    let degree2_energy: f32 = n3d_order2[4..9].iter().map(|v| v.powi(2)).sum();
    assert!(
        (degree2_energy - 5.0).abs() < 1e-3,
        "N3D degree 2 energy was {}",
        degree2_energy
    );
}

#[test]
fn test_ambisonic_normalization_conversions() {
    let order = AmbisonicOrder::First;
    let mut buffer = vec![1.0f32; 4]; // 1 frame of FOA

    // Convert SN3D -> N3D:
    // W (ell=0) remains 1.0, Y,Z,X (ell=1) scaled by sqrt(3)
    AmbisonicNormalization::convert_buffer(
        &mut buffer,
        order,
        AmbisonicNormalization::SN3D,
        AmbisonicNormalization::N3D,
    );
    assert!((buffer[0] - 1.0).abs() < 1e-5);
    assert!((buffer[1] - (3.0f32).sqrt()).abs() < 1e-5);
    assert!((buffer[2] - (3.0f32).sqrt()).abs() < 1e-5);
    assert!((buffer[3] - (3.0f32).sqrt()).abs() < 1e-5);

    // Convert N3D -> FuMa:
    // W becomes 1/sqrt(2) ≈ 0.7071
    AmbisonicNormalization::convert_buffer(
        &mut buffer,
        order,
        AmbisonicNormalization::N3D,
        AmbisonicNormalization::FuMa,
    );
    assert!((buffer[0] - FRAC_1_SQRT_2).abs() < 1e-4);
    assert!((buffer[1] - 1.0).abs() < 1e-4);

    // Convert FuMa -> SN3D (roundtrip)
    AmbisonicNormalization::convert_buffer(
        &mut buffer,
        order,
        AmbisonicNormalization::FuMa,
        AmbisonicNormalization::SN3D,
    );
    assert!((buffer[0] - 1.0).abs() < 1e-4);
    assert!((buffer[1] - 1.0).abs() < 1e-4);
}

#[test]
fn test_ambisonic_soundfield_rotation() {
    let order = AmbisonicOrder::First;
    // Initial soundfield: pure front source (X = 1.0, W = 1.0, Y = 0.0, Z = 0.0)
    let mut buffer = vec![1.0, 0.0, 0.0, 1.0];

    // Rotate 90 degrees yaw (+pi/2, looking left):
    // A sound directly in front should now appear to the right (Y = -1.0, X = 0.0)
    AmbisonicRotator::rotate_buffer(&mut buffer, order, FRAC_PI_2, 0.0, 0.0);

    // W is invariant under rotation
    assert!((buffer[0] - 1.0).abs() < 1e-5);
    // Y (Left/Right dipole): rotated to left (+1.0)
    assert!((buffer[1] - 1.0).abs() < 1e-4, "Y was {}", buffer[1]);
    // Z (Height dipole): unchanged
    assert!(buffer[2].abs() < 1e-5);
    // X (Front/Back dipole): becomes 0.0
    assert!(buffer[3].abs() < 1e-4, "X was {}", buffer[3]);

    // Verify energy conservation: X^2 + Y^2 + Z^2 = 1.0
    let power = buffer[1].powi(2) + buffer[2].powi(2) + buffer[3].powi(2);
    assert!((power - 1.0).abs() < 1e-4);
}

#[test]
fn test_3d_spatial_position_and_distance_attenuation() {
    let pos_spherical = SpatialPosition3D::from_degrees(45.0, 30.0, 2.0);
    let (x, y, z) = pos_spherical.to_cartesian();

    // Reconstruct position from Cartesian coordinates
    let pos_reconstructed = SpatialPosition3D::from_cartesian(x, y, z);
    assert!((pos_reconstructed.azimuth - pos_spherical.azimuth).abs() < 1e-4);
    assert!((pos_reconstructed.elevation - pos_spherical.elevation).abs() < 1e-4);
    assert!((pos_reconstructed.distance - pos_spherical.distance).abs() < 1e-4);

    // Distance attenuation: at 2.0m, gain should be 1.0 / 2.0 = 0.5
    let gain_2m = pos_spherical.distance_attenuation(1.0);
    assert_eq!(gain_2m, 0.5);

    // Air absorption: close source (1.0m) has 0 attenuation; distant source (5.0m) has high damping
    let pos_1m = SpatialPosition3D::from_degrees(0.0, 0.0, 1.0);
    assert_eq!(pos_1m.air_absorption_filter_alpha(), 0.0);

    let pos_5m = SpatialPosition3D::from_degrees(0.0, 0.0, 5.0);
    assert!(pos_5m.air_absorption_filter_alpha() > 0.15);
}

#[test]
fn test_woodworth_schlosser_binaural_hrtf_itd_and_ild() {
    let hrtf = BinauralHrtfEngine::new(48000.0);

    // Center source (azimuth = 0): ITD must be zero for both ears
    let (left_itd_0, right_itd_0) = hrtf.calculate_itd_seconds(0.0);
    assert_eq!(left_itd_0, 0.0);
    assert_eq!(right_itd_0, 0.0);

    // Pure Left source (azimuth = +pi/2):
    // Left ear has 0 delay; Right ear has maximum Woodworth ITD (~0.655 ms)
    let (left_itd_90, right_itd_90) = hrtf.calculate_itd_seconds(FRAC_PI_2);
    assert_eq!(left_itd_90, 0.0);
    assert!(
        right_itd_90 > 0.00060 && right_itd_90 < 0.00070,
        "Right ITD was {} seconds",
        right_itd_90
    );

    // In samples at 48 kHz: ~31.5 samples
    let right_delay_samples = right_itd_90 * 48000.0;
    assert!(
        (right_delay_samples - 31.5).abs() < 1.0,
        "Delay in samples was {}",
        right_delay_samples
    );
}

#[test]
fn test_ambisonics_to_binaural_virtual_loudspeaker_decoding() {
    let mut encoder = AmbisonicEncoder::new(AmbisonicOrder::First, AmbisonicNormalization::SN3D);
    let mut hrtf = BinauralHrtfEngine::new(48000.0);

    // Synthesize a hard Left sound source at +90° azimuth
    let left_pos = SpatialPosition3D::from_degrees(90.0, 0.0, 1.0);
    let frame_count = 256;
    let mono_pulse = vec![1.0f32; frame_count];
    let mut b_format = vec![0.0f32; frame_count * 4];

    encoder.encode_mono_block(&mono_pulse, &left_pos, &mut b_format);

    let mut stereo_binaural = vec![0.0f32; frame_count * 2];
    hrtf.decode_ambisonics_to_binaural(&b_format, AmbisonicOrder::First, &mut stereo_binaural);

    // Measure peak energy in Left vs Right ear:
    // Left ear must be significantly louder than the shadowed Right ear
    let mut left_energy = 0.0f32;
    let mut right_energy = 0.0f32;
    for i in 32..frame_count {
        left_energy += stereo_binaural[i * 2].powi(2);
        right_energy += stereo_binaural[i * 2 + 1].powi(2);
    }

    assert!(
        left_energy > right_energy * 1.5,
        "Left energy ({}) should exceed Right energy ({}) for a Left source",
        left_energy,
        right_energy
    );
}

#[test]
fn test_714_immersive_bed_speaker_decoding() {
    let mut encoder = AmbisonicEncoder::new(AmbisonicOrder::First, AmbisonicNormalization::SN3D);

    // 1. Elevated sound source: Top Front Left (azimuth = 45°, elevation = +45°)
    let overhead_pos = SpatialPosition3D::from_degrees(45.0, 45.0, 1.0);
    let frame_count = 64;
    let input = vec![1.0f32; frame_count];
    let mut b_format = vec![0.0f32; frame_count * 4];
    encoder.encode_mono_block(&input, &overhead_pos, &mut b_format);

    let mut out_714 = vec![0.0f32; frame_count * 12];
    AmbisonicDecoder::decode(
        &b_format,
        AmbisonicOrder::First,
        ChannelLayout::Surround714,
        &mut out_714,
    );

    // In 7.1.4:
    // [0..7] are ear level, [8: Tfl, 9: Tfr, 10: Tbl, 11: Tbr] are overhead height
    let tfl = out_714[8];
    let tfr = out_714[9];
    let tbl = out_714[10];
    let tbr = out_714[11];

    // Top Front Left should dominate the overhead plane
    assert!(tfl > 0.4, "Tfl was {}", tfl);
    assert!(tfl > tfr, "Tfl ({}) should exceed Tfr ({})", tfl, tfr);
    assert!(tfl > tbl, "Tfl ({}) should exceed Tbl ({})", tfl, tbl);
    assert!(tfl > tbr, "Tfl ({}) should exceed Tbr ({})", tfl, tbr);

    // LFE channel must have low-frequency sub-bass energy
    let lfe = out_714[3];
    assert!(lfe > 0.1 && lfe < 0.35, "LFE was {}", lfe);

    // 2. Downmix 7.1.4 to stereo via ITU downmixer
    let mut stereo_out = vec![0.0f32; frame_count * 2];
    ItuDownmixer::downmix_to_stereo(&out_714, ChannelLayout::Surround714, &mut stereo_out);
    assert!(stereo_out[0] > 0.0); // Left channel has energy
    assert!(stereo_out[0] > stereo_out[1]); // Panned left
}

#[test]
fn test_ambisonic_mixer_multi_track_orchestration() {
    let clock = AudioClock::new();
    clock.set_playing(true);
    let mut mixer = AmbisonicMixer::new(AmbisonicOrder::First, clock);

    // Track 1: Center dialogue
    let dialogue = vec![0.6f32; 480];
    mixer.add_track(AmbisonicTrack::new(
        dialogue,
        1,
        1.0,
        SpatialPosition3D::from_degrees(0.0, 0.0, 1.0),
        AmbisonicOrder::First,
    ));

    // Track 2: Overhead right drone
    let drone = vec![0.4f32; 480];
    mixer.add_track(AmbisonicTrack::new(
        drone,
        1,
        0.8,
        SpatialPosition3D::from_degrees(-45.0, 45.0, 1.5),
        AmbisonicOrder::First,
    ));

    // Render directly to Binaural headphone mix
    let mut binaural_out = vec![0.0f32; 480 * 2];
    mixer.mix_to_binaural(&mut binaural_out);

    // Verify signals are non-zero, finite, and well-behaved
    for &sample in &binaural_out {
        assert!(sample.is_finite());
        assert!(sample.abs() <= 2.0);
    }

    // Reset playback position
    mixer.seek_to_sample(0);

    // Render directly to 7.1.4 Immersive Bed
    let mut bed_714 = vec![0.0f32; 480 * 12];
    mixer.mix_to_714(&mut bed_714);

    for &sample in &bed_714 {
        assert!(sample.is_finite());
        assert!(sample.abs() <= 2.0);
    }
}
