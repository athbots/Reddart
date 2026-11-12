use rd_core::{Clip, Project, Timeline, TrackKind};
use rd_render::graph::{NodeKind, RenderGraph};
use rd_render::{CompositorLayer, composite_layers_over};

#[test]
fn test_compound_clip_dag_compilation_and_hierarchy() {
    let mut project = Project::new("Compound Clip Test");
    let main_v1 = project.timeline.add_track(TrackKind::Video, "Main V1 Base");
    let main_v2 = project
        .timeline
        .add_track(TrackKind::Video, "Main V2 Compound");

    // Base background video: 0 to 10s
    let bg_clip = Clip::new_video("main_bg.mp4", 0, 0, 10_000_000);
    project
        .timeline
        .track_mut(main_v1)
        .unwrap()
        .add_clip(bg_clip);

    // Inner timeline: contains 2 video tracks
    let mut inner = Timeline::new();
    let in_v1 = inner.add_track(TrackKind::Video, "Inner Lower Layer");
    let in_v2 = inner.add_track(TrackKind::Video, "Inner Upper Layer");

    let in_clip_1 = Clip::new_video("inner_layer_1.mp4", 0, 0, 5_000_000);
    let in_clip_2 = Clip::new_video("inner_layer_2.mp4", 1_000_000, 0, 3_000_000);

    inner.track_mut(in_v1).unwrap().add_clip(in_clip_1);
    inner.track_mut(in_v2).unwrap().add_clip(in_clip_2);

    // Create compound clip placed at 2.0s on Main V2
    let mut compound = Clip::new_compound("Nested Lower Third", inner, 2_000_000);
    compound.transform.scale.x = 0.5;
    compound.transform.scale.y = 0.5;
    compound.opacity = 0.8;

    project
        .timeline
        .track_mut(main_v2)
        .unwrap()
        .add_clip(compound);

    // At 1.0s: Compound clip is not yet active (starts at 2.0s). Only main bg is active.
    let graph_1s = RenderGraph::compile_timeline(&project, 1_000_000);
    let order_1s = graph_1s.topological_sort().expect("Valid DAG at 1s");
    let sources_1s: Vec<_> = order_1s
        .iter()
        .filter(|&&id| matches!(graph_1s.get_node(id).unwrap().kind, NodeKind::Source { .. }))
        .collect();
    assert_eq!(sources_1s.len(), 1, "Only base video active at 1.0s");

    // At 3.5s: 1.5s into compound clip.
    // In inner timeline:
    //   Inner clip 1 is active (at 1.5s, covers 0-5s)
    //   Inner clip 2 is active (at 1.5s, covers 1-4s)
    let graph_3_5s = RenderGraph::compile_timeline(&project, 3_500_000);
    let order_3_5s = graph_3_5s.topological_sort().expect("Valid DAG at 3.5s");

    let sources_3_5s: Vec<_> = order_3_5s
        .iter()
        .filter(|&&id| {
            matches!(
                graph_3_5s.get_node(id).unwrap().kind,
                NodeKind::Source { .. }
            )
        })
        .collect();
    assert_eq!(
        sources_3_5s.len(),
        3,
        "Base video + 2 inner nested clips active at 3.5s"
    );

    // Verify topological order: sources must appear before blends, and final output must be last
    let output_id = graph_3_5s.output_node().expect("Output exists");
    assert_eq!(*order_3_5s.last().unwrap(), output_id);
}

#[test]
fn test_compound_clip_software_compositing_pipeline() {
    let width = 64u32;
    let height = 64u32;
    let px_count = (width * height) as usize;

    // 1. Render inner timeline to an intermediate RGBA buffer
    // Inner Track 1: Solid Red (255, 0, 0, 255)
    let inner_l1 = CompositorLayer::new([255, 0, 0, 255].repeat(px_count), width, height);

    // Inner Track 2: Solid Green (0, 255, 0, 255) at 50% opacity
    let mut inner_l2 = CompositorLayer::new([0, 255, 0, 255].repeat(px_count), width, height);
    inner_l2.opacity = 0.5;

    let mut inner_rendered = vec![0u8; px_count * 4];
    composite_layers_over(&mut inner_rendered, width, height, &[inner_l1, inner_l2]);

    // Inner blend result: (128, 128, 0, 255)
    let inner_px = &inner_rendered[0..4];
    assert!((inner_px[0] as i16 - 128).abs() <= 2);
    assert!((inner_px[1] as i16 - 128).abs() <= 2);
    assert_eq!(inner_px[2], 0);
    assert_eq!(inner_px[3], 255);

    // 2. Now composite outer timeline:
    // Outer Track 1: Solid Blue background (0, 0, 255, 255)
    let outer_bg = CompositorLayer::new([0, 0, 255, 255].repeat(px_count), width, height);

    // Outer Track 2: The compound clip containing `inner_rendered`, scaled and with 50% opacity
    let compound_layer = CompositorLayer::with_transform(
        inner_rendered,
        width,
        height,
        0.5, // 50% outer opacity
        0.0,
        0.0,
        1.0,
        1.0,
        0, // Normal blend
    );

    let mut final_output = vec![0u8; px_count * 4];
    composite_layers_over(
        &mut final_output,
        width,
        height,
        &[outer_bg, compound_layer],
    );

    // Final result blends 50% inner (128, 128, 0) over Blue (0, 0, 255)
    // Red: 128 * 0.5 = 64
    // Green: 128 * 0.5 = 64
    // Blue: 255 * (1 - 0.5) = ~128
    // Alpha: 255
    let out_px = &final_output[0..4];
    assert_eq!(out_px[3], 255);
    assert!(
        (out_px[0] as i16 - 64).abs() <= 2,
        "Red was expected ~64, got {}",
        out_px[0]
    );
    assert!(
        (out_px[1] as i16 - 64).abs() <= 2,
        "Green was expected ~64, got {}",
        out_px[1]
    );
    assert!(
        (out_px[2] as i16 - 128).abs() <= 2,
        "Blue was expected ~128, got {}",
        out_px[2]
    );
}

#[test]
fn test_compound_clip_speed_remapping() {
    let mut project = Project::new("Speed Remap Project");
    let track_id = project.timeline.add_track(TrackKind::Video, "Video Track");

    let mut inner = Timeline::new();
    let inner_track = inner.add_track(TrackKind::Video, "Inner Track");
    let inner_clip = Clip::new_video("source.mp4", 0, 0, 10_000_000);
    inner.track_mut(inner_track).unwrap().add_clip(inner_clip);

    // Compound clip running at 2.0x speed
    let mut compound = Clip::new_compound("2x Fast Sequence", inner, 0);
    compound.speed = 2.0;

    project
        .timeline
        .track_mut(track_id)
        .unwrap()
        .add_clip(compound);

    // At 1.0s (1_000_000 us) on parent timeline:
    // With speed 2.0x, source time inside compound clip must be 2_000_000 us!
    let graph = RenderGraph::compile_timeline(&project, 1_000_000);
    let order = graph.topological_sort().expect("Valid DAG");

    let mut inner_source_time = None;
    for &id in &order {
        if let NodeKind::Source { source_time_us, .. } = graph.get_node(id).unwrap().kind {
            inner_source_time = Some(source_time_us);
            break;
        }
    }

    assert_eq!(
        inner_source_time,
        Some(2_000_000),
        "At 1.0s with 2x speed, inner sequence source time must be 2.0s"
    );
}
