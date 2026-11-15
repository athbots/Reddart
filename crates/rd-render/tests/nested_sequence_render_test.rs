use rd_core::{Clip, Project, Timeline, TrackKind};
use rd_render::cache::{SubSequenceCache, SubSequenceCacheKey};
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

#[test]
fn test_multi_level_deep_recursion_3_levels() {
    // Level 3 (innermost): Two clips on two tracks
    let mut lvl3 = Timeline::new();
    let l3_t1 = lvl3.add_track(TrackKind::Video, "L3 T1");
    let l3_t2 = lvl3.add_track(TrackKind::Video, "L3 T2");
    lvl3.track_mut(l3_t1)
        .unwrap()
        .add_clip(Clip::new_video("leaf_a.mp4", 0, 0, 10_000_000));
    lvl3.track_mut(l3_t2)
        .unwrap()
        .add_clip(Clip::new_video("leaf_b.mp4", 0, 0, 10_000_000));

    // Level 2: Compound clip nesting Level 3 at 2.0x speed
    let mut lvl2 = Timeline::new();
    let l2_t1 = lvl2.add_track(TrackKind::Video, "L2 T1");
    let mut c3 = Clip::new_compound("Nested L3", lvl3, 0);
    c3.speed = 2.0;
    lvl2.track_mut(l2_t1).unwrap().add_clip(c3);

    // Level 1: Compound clip nesting Level 2 at 0.5x speed
    let mut lvl1 = Timeline::new();
    let l1_t1 = lvl1.add_track(TrackKind::Video, "L1 T1");
    let mut c2 = Clip::new_compound("Nested L2", lvl2, 0);
    c2.speed = 0.5;
    lvl1.track_mut(l1_t1).unwrap().add_clip(c2);

    // Level 0: Main project nesting Level 1 at position 1.0s (1_000_000 us)
    let mut project = Project::new("Deep 3-Level Nesting");
    let main_track = project.timeline.add_track(TrackKind::Video, "Main V1");
    let c1 = Clip::new_compound("Nested L1", lvl1, 1_000_000);
    project.timeline.track_mut(main_track).unwrap().add_clip(c1);

    // Query at main timeline timestamp 2.0s (2_000_000 us):
    // Offset into c1: 2.0s - 1.0s = 1.0s
    // c1 speed is 1.0 -> time into lvl1: 1.0s
    // c2 speed is 0.5 -> time into lvl2: 1.0s * 0.5 = 0.5s
    // c3 speed is 2.0 -> time into lvl3: 0.5s * 2.0 = 1.0s (1_000_000 us)
    let graph = RenderGraph::compile_timeline(&project, 2_000_000);
    let order = graph
        .topological_sort()
        .expect("Valid DAG with 3-level nesting");

    let mut source_times = Vec::new();
    for &id in &order {
        if let NodeKind::Source { source_time_us, .. } = graph.get_node(id).unwrap().kind {
            source_times.push(source_time_us);
        }
    }

    assert_eq!(
        source_times.len(),
        2,
        "Expected 2 active leaf sources in 3-level hierarchy"
    );
    assert_eq!(
        source_times[0], 1_000_000,
        "Leaf A source time mapped correctly through speed cascade"
    );
    assert_eq!(
        source_times[1], 1_000_000,
        "Leaf B source time mapped correctly through speed cascade"
    );
}

#[test]
fn test_sub_sequence_cache_hit_and_invalidation() {
    let mut project = Project::new("Cached Sequence Project");
    let main_track = project.timeline.add_track(TrackKind::Video, "Main Video");

    // Inner timeline with 3 tracks and 3 clips
    let mut inner = Timeline::new();
    let t1 = inner.add_track(TrackKind::Video, "Inner 1");
    let t2 = inner.add_track(TrackKind::Video, "Inner 2");
    let t3 = inner.add_track(TrackKind::Video, "Inner 3");
    inner
        .track_mut(t1)
        .unwrap()
        .add_clip(Clip::new_video("clip1.mp4", 0, 0, 5_000_000));
    inner
        .track_mut(t2)
        .unwrap()
        .add_clip(Clip::new_video("clip2.mp4", 0, 0, 5_000_000));
    inner
        .track_mut(t3)
        .unwrap()
        .add_clip(Clip::new_video("clip3.mp4", 0, 0, 5_000_000));

    let compound = Clip::new_compound("Heavy VFX Sequence", inner, 0);
    let compound_id = compound.id;
    let inner_hash = match &compound.kind {
        rd_core::ClipKind::Compound(c) => c.content_hash(),
        _ => unreachable!(),
    };

    project
        .timeline
        .track_mut(main_track)
        .unwrap()
        .add_clip(compound);

    // 1. Without cache: Graph expands inner tracks and clips (at least 3 sources + transforms + blends)
    let uncached_graph = RenderGraph::compile_timeline(&project, 1_000_000);
    let uncached_order = uncached_graph.topological_sort().unwrap();
    let source_count_uncached = uncached_order
        .iter()
        .filter(|&&id| {
            matches!(
                uncached_graph.get_node(id).unwrap().kind,
                NodeKind::Source { .. }
            )
        })
        .count();
    assert_eq!(
        source_count_uncached, 3,
        "Without cache, all 3 inner sources must be compiled"
    );

    // 2. Pre-populate sub-sequence cache for this compound clip at 1.0s
    let mut cache = SubSequenceCache::new(64);
    let cache_key = SubSequenceCacheKey {
        compound_clip_id: compound_id,
        source_time_us: 1_000_000,
        sequence_hash: inner_hash,
    };
    cache.insert(cache_key, 1920, 1080, vec![0u8; 1920 * 1080 * 4]);

    // 3. Compile with cache: Should produce a single CachedSequence node instead of 3 sources
    let cached_graph = RenderGraph::compile_timeline_with_cache(&project, 1_000_000, Some(&cache));
    let cached_order = cached_graph.topological_sort().unwrap();

    let cached_nodes: Vec<_> = cached_order
        .iter()
        .filter(|&&id| {
            matches!(
                cached_graph.get_node(id).unwrap().kind,
                NodeKind::CachedSequence { .. }
            )
        })
        .collect();

    assert_eq!(
        cached_nodes.len(),
        1,
        "Graph must contain exactly 1 CachedSequence node"
    );

    let source_nodes_cached = cached_order
        .iter()
        .filter(|&&id| {
            matches!(
                cached_graph.get_node(id).unwrap().kind,
                NodeKind::Source { .. }
            )
        })
        .count();
    assert_eq!(
        source_nodes_cached, 0,
        "Zero inner sources should be created when cache hits"
    );

    // 4. Invalidate clip: Purges cache and restores full graph compilation
    cache.invalidate_clip(compound_id);
    let invalidated_graph =
        RenderGraph::compile_timeline_with_cache(&project, 1_000_000, Some(&cache));
    let invalidated_order = invalidated_graph.topological_sort().unwrap();
    let source_nodes_invalidated = invalidated_order
        .iter()
        .filter(|&&id| {
            matches!(
                invalidated_graph.get_node(id).unwrap().kind,
                NodeKind::Source { .. }
            )
        })
        .count();
    assert_eq!(
        source_nodes_invalidated, 3,
        "After invalidation, full inner graph is recompiled"
    );
}

#[test]
fn test_sub_sequence_cache_eviction_lru() {
    let mut cache = SubSequenceCache::new(2);
    let clip_a = rd_core::ClipId::new();
    let clip_b = rd_core::ClipId::new();
    let clip_c = rd_core::ClipId::new();

    let key_a = SubSequenceCacheKey {
        compound_clip_id: clip_a,
        source_time_us: 1_000_000,
        sequence_hash: 100,
    };
    let key_b = SubSequenceCacheKey {
        compound_clip_id: clip_b,
        source_time_us: 2_000_000,
        sequence_hash: 200,
    };
    let key_c = SubSequenceCacheKey {
        compound_clip_id: clip_c,
        source_time_us: 3_000_000,
        sequence_hash: 300,
    };

    cache.insert(key_a, 64, 64, vec![10u8; 64 * 64 * 4]);
    cache.insert(key_b, 64, 64, vec![20u8; 64 * 64 * 4]);
    assert_eq!(cache.len(), 2);

    // Touch key_a to make key_b the oldest LRU entry
    let _ = cache.get(&key_a);
    assert_eq!(cache.hits(), 1);

    // Insert key_c: should evict key_b
    cache.insert(key_c, 64, 64, vec![30u8; 64 * 64 * 4]);
    assert_eq!(cache.len(), 2);

    assert!(
        cache.contains(&key_a),
        "Recently touched key_a must remain in cache"
    );
    assert!(
        !cache.contains(&key_b),
        "Oldest entry key_b must be evicted"
    );
    assert!(
        cache.contains(&key_c),
        "Newly inserted key_c must be present in cache"
    );
}
