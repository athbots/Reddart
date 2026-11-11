use std::sync::Arc;

use rd_audio::{AudioClock, AudioEngine, TimelineAudioScheduler};
use rd_core::{Clip, Project, TrackKind};

#[test]
fn test_audio_timeline_mute_and_solo_scheduling() {
    let clock = AudioClock::new();
    let engine = AudioEngine::new(clock.clone()).expect("Failed to initialize AudioEngine");
    let mut scheduler = TimelineAudioScheduler::new();

    let mut project = Project::new("Audio Mute/Solo Test");

    // Add Track 1 with soundtrack clip
    let t1_id = project.timeline.add_track(TrackKind::Audio, "A1");
    let clip1 = Clip::new_audio(
        "ready_to_use/windows/samples/audio/soundtrack.wav",
        0,
        2_000_000,
    );
    project.timeline.track_mut(t1_id).unwrap().add_clip(clip1);

    // Add Track 2 with soundtrack clip
    let t2_id = project.timeline.add_track(TrackKind::Audio, "A2");
    let clip2 = Clip::new_audio(
        "ready_to_use/windows/samples/audio/soundtrack.wav",
        1_000_000,
        2_000_000,
    );
    project.timeline.track_mut(t2_id).unwrap().add_clip(clip2);

    // 1. Normal sync (both enabled)
    assert!(scheduler.sync_timeline(&engine, &project, None).is_ok());

    // 2. Mute Track 1
    project.timeline.track_mut(t1_id).unwrap().enabled = false;
    assert!(scheduler.sync_timeline(&engine, &project, None).is_ok());

    // 3. Solo Track 1 (overrides enabled flag for solo)
    assert!(
        scheduler
            .sync_timeline(&engine, &project, Some(t1_id))
            .is_ok()
    );

    // 4. Solo Track 2
    assert!(
        scheduler
            .sync_timeline(&engine, &project, Some(t2_id))
            .is_ok()
    );
}

#[test]
fn test_audio_and_video_clock_synchronization() {
    let clock = AudioClock::new();
    let engine = Arc::new(AudioEngine::new(clock.clone()).expect("Failed to create engine"));

    // Initial position should be 0
    assert_eq!(clock.now_us(), 0);

    // Seek to 1,500,000 microseconds
    engine.seek_to_time_us(1_500_000);
    assert_eq!(clock.now_us(), 1_500_000);

    // Advance 48000 samples (1 full second = 1,000,000 us) while playing
    clock.set_playing(true);
    clock.advance_samples(48000, 48000);
    assert_eq!(clock.now_us(), 2_500_000);

    // Seek back to 0
    engine.seek_to_time_us(0);
    assert_eq!(clock.now_us(), 0);
}
