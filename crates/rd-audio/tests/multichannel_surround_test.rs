use rd_audio::clock::AudioClock;
use rd_audio::spatial::{
    ChannelLayout, ItuDownmixer, MultichannelMixer, MultichannelTelemetry, SpatialPanner,
    SpatialParams,
};

#[test]
fn test_multichannel_51_spatial_panning_and_energy_conservation() {
    let clock = AudioClock::new();
    clock.set_playing(true);
    let mut mixer = MultichannelMixer::new(ChannelLayout::Surround51, clock);

    // Track 1: Front Center Dialogue (Mono, pan_x=0.0, pan_y=1.0, 100% center bleed)
    let dialogue = vec![0.8f32; 480];
    mixer.add_track(
        dialogue,
        1,
        1.0,
        SpatialParams {
            pan_x: 0.0,
            pan_y: 1.0,
            center_percentage: 1.0,
            main_send: 1.0,
            lfe_send: 0.0,
        },
    );

    // Track 2: Left Surround Ambience (Mono, pan_x=-1.0, pan_y=-1.0)
    let left_surround = vec![0.4f32; 480];
    mixer.add_track(
        left_surround,
        1,
        1.0,
        SpatialParams {
            pan_x: -1.0,
            pan_y: -1.0,
            center_percentage: 0.0,
            main_send: 1.0,
            lfe_send: 0.0,
        },
    );

    // Track 3: Subwoofer Bass Impact (Mono with 100% LFE send, 0.0 main send)
    let sub_bass = vec![0.6f32; 480];
    mixer.add_track(
        sub_bass,
        1,
        1.0,
        SpatialParams {
            pan_x: 0.0,
            pan_y: 0.0,
            center_percentage: 0.0,
            main_send: 0.0,
            lfe_send: 1.0,
        },
    );

    // 480 frames of 5.1 (6 channels * 480 frames = 2880 samples)
    let mut out_51 = vec![0.0f32; 480 * 6];
    mixer.mix_interleaved(&mut out_51);

    // Compute multichannel telemetry
    let telemetry = MultichannelTelemetry::from_interleaved(&out_51, 6);
    assert_eq!(telemetry.peaks.len(), 6);

    // Channel 2 (Center) should be dominated by dialogue (~0.8)
    assert!(
        (telemetry.peaks[2] - 0.8).abs() < 0.05,
        "Center peak was {}",
        telemetry.peaks[2]
    );

    // Channel 3 (LFE) should receive sub bass (~0.6)
    assert!(
        (telemetry.peaks[3] - 0.6).abs() < 0.05,
        "LFE peak was {}",
        telemetry.peaks[3]
    );

    // Channel 4 (Left Surround) should receive ambience (~0.4)
    assert!(
        (telemetry.peaks[4] - 0.4).abs() < 0.05,
        "Ls peak was {}",
        telemetry.peaks[4]
    );

    // Channel 5 (Right Surround) should be zero
    assert!(
        telemetry.peaks[5] < 0.01,
        "Rs peak was {}",
        telemetry.peaks[5]
    );
}

#[test]
fn test_multichannel_71_surround_discrete_routing() {
    let clock = AudioClock::new();
    let mut mixer = MultichannelMixer::new(ChannelLayout::Surround71, clock);

    // Stereo music track positioned in front
    let mut stereo_music = Vec::with_capacity(960);
    for _ in 0..480 {
        stereo_music.push(0.5); // Left
        stereo_music.push(0.5); // Right
    }

    mixer.add_track(
        stereo_music,
        2,
        1.0,
        SpatialParams {
            pan_x: 0.0,
            pan_y: 1.0,
            center_percentage: 0.0, // pure L/R front
            main_send: 1.0,
            lfe_send: 0.1,
        },
    );

    // Mono back-right flyby effect: pan_x = 1.0, pan_y = -1.0
    let flyby = vec![0.7f32; 480];
    mixer.add_track(
        flyby,
        1,
        1.0,
        SpatialParams {
            pan_x: 1.0,
            pan_y: -1.0,
            center_percentage: 0.0,
            main_send: 1.0,
            lfe_send: 0.0,
        },
    );

    let mut out_71 = vec![0.0f32; 480 * 8];
    mixer.mix_interleaved(&mut out_71);

    let telemetry = MultichannelTelemetry::from_interleaved(&out_71, 8);
    // [0: L, 1: R, 2: C, 3: LFE, 4: Ls, 5: Rs, 6: Lb, 7: Rb]
    assert!(telemetry.peaks[0] > 0.3, "Left was {}", telemetry.peaks[0]);
    assert!(telemetry.peaks[1] > 0.3, "Right was {}", telemetry.peaks[1]);
    assert!(
        telemetry.peaks[7] > 0.6,
        "Right Back (Rb) was {}",
        telemetry.peaks[7]
    );
    assert!(
        telemetry.peaks[6] < 0.01,
        "Left Back (Lb) was {}",
        telemetry.peaks[6]
    );
}

#[test]
fn test_itu_downmixer_headroom_and_balance() {
    // Construct a loud 5.1 stream where all 6 channels are playing full amplitude (1.0)
    let frame_count = 100;
    let multi_51 = vec![1.0f32; frame_count * 6];
    let mut stereo_out = vec![0.0f32; frame_count * 2];

    ItuDownmixer::downmix_to_stereo(&multi_51, ChannelLayout::Surround51, &mut stereo_out);

    // Verify ITU downmixing produces equal Left and Right levels
    for i in 0..frame_count {
        let left = stereo_out[i * 2];
        let right = stereo_out[i * 2 + 1];
        assert_eq!(left, right);
        // Headroom scaling ensures downmix remains safely bounded
        assert!(left <= 1.8, "Stereo sample exceeded safe range: {}", left);
        assert!(left > 0.5, "Stereo sample was unexpectedly quiet");
    }
}

#[test]
fn test_spatial_panner_trigonometric_invariants() {
    // Sweep pan_x across 21 points and verify power conservation for stereo
    for i in 0..=20 {
        let px = -1.0 + (i as f32 * 0.1);
        let params = SpatialParams {
            pan_x: px,
            pan_y: 1.0,
            center_percentage: 0.0,
            main_send: 1.0,
            lfe_send: 0.0,
        };
        let gains = SpatialPanner::compute_gains(ChannelLayout::Stereo, &params);
        let power = gains[0] * gains[0] + gains[1] * gains[1];
        assert!(
            (power - 1.0).abs() < 1e-4,
            "Stereo power violated at pan_x={}: {}",
            px,
            power
        );
    }

    // Sweep across 5.1 soundfield and verify power conservation
    for ix in -5..=5 {
        for iy in -5..=5 {
            let px = ix as f32 * 0.2;
            let py = iy as f32 * 0.2;
            let params = SpatialParams {
                pan_x: px,
                pan_y: py,
                center_percentage: 0.5,
                main_send: 1.0,
                lfe_send: 0.0,
            };
            let gains = SpatialPanner::compute_gains(ChannelLayout::Surround51, &params);
            let main_power = gains[0].powi(2)
                + gains[1].powi(2)
                + gains[2].powi(2)
                + gains[4].powi(2)
                + gains[5].powi(2);
            assert!(
                (main_power - 1.0).abs() < 1e-3,
                "5.1 main power violated at ({}, {}): {}",
                px,
                py,
                main_power
            );
        }
    }
}
