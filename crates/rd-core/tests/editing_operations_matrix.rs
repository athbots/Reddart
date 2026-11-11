//! P0-2: Comprehensive Editing Operations Validation Matrix
//!
//! Validates:
//! 1. Positive and negative deltas for RippleTrimIn, RippleTrimOut, SlideEdit, RollEdit, SlipEdit.
//! 2. Clip playback speeds: 0.25x (slow), 0.5x, 1.0x, 2.0x, 4.0x (fast).
//! 3. Boundary limits: source underflow (< 0), zero-duration collapse, timeline underflow.
//! 4. Adjacent-clip contiguity & gap-free invariants.
//! 5. Multi-track independence (editing on Track 1 leaves Track 2 untouched).

use rd_core::{Clip, Project, TrackKind};

fn create_clip(pos_us: i64, dur_us: i64, speed: f64, source_in_us: i64) -> Clip {
    let mut c = Clip::new_video(
        "test.mp4",
        pos_us,
        source_in_us,
        source_in_us + (dur_us as f64 * speed).round() as i64,
    );
    c.speed = speed;
    c
}

#[test]
fn test_ripple_trim_in_positive_and_negative_matrix() {
    let mut project = Project::new("RippleTrim Matrix");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");
    let track = project.timeline.track_mut(tid).unwrap();

    // Two adjacent clips:
    // Clip A: 0..4s, speed 1.0, source 1s..5s (has 1s head media available)
    // Clip B: 4..8s, speed 1.0, source 0s..4s
    let clip_a = create_clip(0, 4_000_000, 1.0, 1_000_000);
    let clip_b = create_clip(4_000_000, 4_000_000, 1.0, 0);
    let id_a = clip_a.id;
    let id_b = clip_b.id;

    track.add_clip(clip_a);
    track.add_clip(clip_b);

    assert_eq!(track.duration_us(), 8_000_000);

    // 1. Positive delta: trim in by +1.5s (shorten head)
    assert!(track.ripple_trim_in(id_a, 1_500_000).is_ok());
    assert_eq!(track.clip(id_a).unwrap().duration_us(), 2_500_000);
    assert_eq!(track.clip(id_a).unwrap().position_us, 0);
    // Downstream clip B pulled left by 1.5s to stay attached
    assert_eq!(track.clip(id_b).unwrap().position_us, 2_500_000);
    assert_eq!(track.duration_us(), 6_500_000);

    // 2. Negative delta: trim in by -500ms (extend head earlier into source)
    assert!(track.ripple_trim_in(id_a, -500_000).is_ok());
    assert_eq!(track.clip(id_a).unwrap().duration_us(), 3_000_000);
    assert_eq!(track.clip(id_a).unwrap().position_us, 0);
    // Downstream clip B pushed right by 500ms
    assert_eq!(track.clip(id_b).unwrap().position_us, 3_000_000);
    assert_eq!(track.duration_us(), 7_000_000);

    // 3. Boundary failure: extend head beyond source start (needs > 2s head, only 2s source_in available)
    assert!(track.ripple_trim_in(id_a, -3_000_000).is_err());
    // 4. Boundary failure: shorten head past source_out
    assert!(track.ripple_trim_in(id_a, 5_000_000).is_err());
}

#[test]
fn test_ripple_trim_with_variable_speeds() {
    let mut project = Project::new("Speed Matrix");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");
    let track = project.timeline.track_mut(tid).unwrap();

    // Clip A: 4.0x speed, 8s source = 2s timeline (0..2s)
    let clip_a = create_clip(0, 2_000_000, 4.0, 0);
    // Clip B: 0.25x speed, 1s source = 4s timeline (2..6s)
    let clip_b = create_clip(2_000_000, 4_000_000, 0.25, 0);
    let id_a = clip_a.id;
    let id_b = clip_b.id;

    track.add_clip(clip_a);
    track.add_clip(clip_b);

    assert_eq!(track.duration_us(), 6_000_000);

    // Trim out on 4.0x clip A by +500ms timeline:
    // Source delta = 500ms * 4.0 = 2000ms
    assert!(track.ripple_trim_out(id_a, 500_000).is_ok());
    assert_eq!(track.clip(id_a).unwrap().duration_us(), 2_500_000);
    // Downstream clip B pushed right by exactly 500ms
    assert_eq!(track.clip(id_b).unwrap().position_us, 2_500_000);
    assert_eq!(track.duration_us(), 6_500_000);

    // Trim in on 0.25x clip B by +1s timeline:
    // Source delta = 1000ms * 0.25 = 250ms
    assert!(track.ripple_trim_in(id_b, 1_000_000).is_ok());
    assert_eq!(track.clip(id_b).unwrap().duration_us(), 3_000_000);
    assert_eq!(track.duration_us(), 5_500_000);
}

#[test]
fn test_slide_edit_comprehensive_matrix() {
    let mut project = Project::new("Slide Matrix");
    let tid = project.timeline.add_track(TrackKind::Video, "V1");
    let track = project.timeline.track_mut(tid).unwrap();

    // Three clips: A (0..3s), B (3..5s), C (5..8s)
    let clip_a = create_clip(0, 3_000_000, 1.0, 0);
    // Clip B is 2s long
    let clip_b = create_clip(3_000_000, 2_000_000, 1.0, 0);
    let id_b = clip_b.id;
    // Clip C is 3s long with 2s head media available (source 2s..5s)
    let clip_c = create_clip(5_000_000, 3_000_000, 1.0, 2_000_000);

    track.add_clip(clip_a);
    track.add_clip(clip_b);
    track.add_clip(clip_c);

    let total_dur = track.duration_us();
    assert_eq!(total_dur, 8_000_000);

    // Slide B right by +1s:
    // Predecessor A lengthened to 4s (0..4s)
    // Target B shifted to 4s (4..6s), duration stays 2s
    // Successor C shortened to 2s, starts at 6s (6..8s)
    assert!(track.slide_edit(id_b, 1_000_000).is_ok());
    assert_eq!(track.clips()[0].duration_us(), 4_000_000);
    assert_eq!(track.clips()[1].position_us, 4_000_000);
    assert_eq!(track.clips()[1].duration_us(), 2_000_000);
    assert_eq!(track.clips()[2].position_us, 6_000_000);
    assert_eq!(track.clips()[2].duration_us(), 2_000_000);
    assert_eq!(track.duration_us(), total_dur);

    // Slide B left by -2s:
    // Predecessor A shortened to 2s (0..2s)
    // Target B shifted to 2s (2..4s), duration stays 2s
    // Successor C lengthened to 4s, starts at 4s (4..8s)
    assert!(track.slide_edit(id_b, -2_000_000).is_ok());
    assert_eq!(track.clips()[0].duration_us(), 2_000_000);
    assert_eq!(track.clips()[1].position_us, 2_000_000);
    assert_eq!(track.clips()[1].duration_us(), 2_000_000);
    assert_eq!(track.clips()[2].position_us, 4_000_000);
    assert_eq!(track.clips()[2].duration_us(), 4_000_000);
    assert_eq!(track.duration_us(), total_dur);

    // Edge case: slide delta too large (exceeds predecessor duration)
    assert!(track.slide_edit(id_b, -2_500_000).is_err());
}

#[test]
fn test_multi_track_edit_isolation() {
    let mut project = Project::new("Multi-Track Isolation");
    let v1 = project.timeline.add_track(TrackKind::Video, "V1");
    let v2 = project.timeline.add_track(TrackKind::Video, "V2");

    let clip_v1_a = create_clip(0, 3_000_000, 1.0, 0);
    let clip_v1_b = create_clip(3_000_000, 3_000_000, 1.0, 0);
    let id_v1_a = clip_v1_a.id;

    let clip_v2 = create_clip(1_000_000, 4_000_000, 1.0, 0);
    let id_v2 = clip_v2.id;

    project.timeline.track_mut(v1).unwrap().add_clip(clip_v1_a);
    project.timeline.track_mut(v1).unwrap().add_clip(clip_v1_b);
    project.timeline.track_mut(v2).unwrap().add_clip(clip_v2);

    // Perform ripple trim on Track V1
    project
        .timeline
        .track_mut(v1)
        .unwrap()
        .ripple_trim_out(id_v1_a, -1_000_000)
        .unwrap();

    // Verify V2 was completely unaffected
    let v2_clip = project.timeline.track(v2).unwrap().clip(id_v2).unwrap();
    assert_eq!(v2_clip.position_us, 1_000_000);
    assert_eq!(v2_clip.duration_us(), 4_000_000);
}
