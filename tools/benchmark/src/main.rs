//! RD-003: End-to-End Render Graph Benchmark Harness
//!
//! Measures p50/p95/p99 latency for the full render graph pipeline:
//! scrub event → DAG compile → cache lookup → (synthetic) frame output.
//!
//! This is the closest approximation to true end-to-end latency achievable
//! without a real GPU presentation context in a headless test.
//!
//! What is measured:
//!   scrub_event (new timestamp) → RenderGraph::compile_timeline → topological_sort
//!   → cache lookup (by content_hash) → "frame result"
//!
//! What is NOT measured here (requires a live window):
//!   - GPU texture upload/download
//!   - wgpu submit + present latency
//!   - Actual video decoder decode_next() latency on real media

use std::collections::HashMap;
use std::time::{Duration, Instant};
use rd_core::{Clip, Project, TrackKind};
use rd_render::graph::RenderGraph;

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((pct / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn run_benchmark(label: &str, track_count: usize, clips_per_track: usize, iterations: usize) {
    // Build project
    let mut project = Project::new(format!("Benchmark_{}", label));
    for t in 0..track_count {
        let tid = project.timeline.add_track(TrackKind::Video, format!("V{}", t + 1));
        for c in 0..clips_per_track {
            let start_us = (c as i64) * 5_000_000; // 5-sec clips
            let clip = Clip::new_video(
                format!("clip_{}_{}_.mp4", t, c),
                start_us,
                0,
                5_000_000,
            );
            project.timeline.track_mut(tid).unwrap().add_clip(clip);
        }
    }
    let total_dur = project.timeline.duration_us();

    // Simulate LRU cache (content_hash -> "done")
    let mut cache: HashMap<u64, ()> = HashMap::new();

    let mut latencies_us: Vec<f64> = Vec::with_capacity(iterations);

    // Warm-up: 50 iterations
    for i in 0..50 {
        let t = (i as i64 * total_dur / 50).min(total_dur.saturating_sub(1));
        let _ = RenderGraph::compile_timeline(&project, t);
    }

    // Measured iterations
    for i in 0..iterations {
        // Simulate scrub to random position (sequential pattern to simulate real use)
        let time_us = ((i as i64 * total_dur) / iterations as i64).min(total_dur.saturating_sub(1));

        let t0 = Instant::now();

        // 1. Compile the render graph (DAG)
        let graph = RenderGraph::compile_timeline(&project, time_us);

        // 2. Topological sort (dependency order for execution)
        let order = graph.topological_sort().unwrap_or_default();

        // 3. Cache lookup - check if output node result is cached
        if let Some(out_id) = graph.output_node() {
            if let Some(node) = graph.get_node(out_id) {
                // Simulate cache miss on first visit; hit thereafter
                cache.entry(node.content_hash).or_insert(());
            }
        }

        // 4. "Execute" nodes (simulate: just iterate in topo order)
        let _ = order.len(); // prevent dead-code elimination

        let elapsed = t0.elapsed();
        latencies_us.push(elapsed.as_secs_f64() * 1_000_000.0);
    }

    latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let p50 = percentile(&latencies_us, 50.0);
    let p95 = percentile(&latencies_us, 95.0);
    let p99 = percentile(&latencies_us, 99.0);
    let min = latencies_us.first().copied().unwrap_or(0.0);
    let max = latencies_us.last().copied().unwrap_or(0.0);
    let mean = latencies_us.iter().sum::<f64>() / latencies_us.len() as f64;

    println!("┌─────────────────────────────────────────────────────────┐");
    println!("│ Benchmark: {:45} │", label);
    println!("│ Tracks: {:<3}  Clips/track: {:<3}  Iterations: {:<6}       │", track_count, clips_per_track, iterations);
    println!("├────────────┬──────────┬──────────┬──────────┬───────────┤");
    println!("│ Metric     │   min    │   p50    │   p95    │    p99    │");
    println!("├────────────┼──────────┼──────────┼──────────┼───────────┤");
    println!("│ Latency    │ {:6.2}µs │ {:6.2}µs │ {:6.2}µs │ {:7.2}µs │", min, p50, p95, p99);
    println!("├────────────┴──────────┴──────────┴──────────┴───────────┤");
    println!("│ mean={:.2}µs  max={:.2}µs                               │", mean, max);
    println!("└─────────────────────────────────────────────────────────┘");
    println!();

    // === IMPORTANT CAVEAT ===
    println!("  NOTE: This measures RenderGraph compilation + cache lookup only.");
    println!("  TRUE end-to-end latency (input → GPU frame presented) also includes:");
    println!("   • Video decode_next() on real media files");
    println!("   • GPU texture upload (wgpu queue.write_texture)");
    println!("   • wgpu command encoding + submit + present");
    println!("   • Display vsync (typically +0–16ms at 60Hz)");
    println!("  Those require a live windowed GPU context and real media.");
    println!();
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║   Reddart RD-003: Render Graph Benchmark Harness          ║");
    println!("║   Measuring DAG compile + cache lookup latency            ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    // Profile 1: Entry-level PC simulation — 1 video track, 5 clips
    run_benchmark("Entry PC — 1-track, 5 clips, 1080p", 1, 5, 1000);

    // Profile 2: Mainstream — 4 video tracks, 10 clips each
    run_benchmark("Mainstream PC — 4-track, 10 clips, 1080p", 4, 10, 1000);

    // Profile 3: High-end — 8 video tracks, 20 clips each
    run_benchmark("High-end PC — 8-track, 20 clips, 4K", 8, 20, 1000);

    // Profile 4: Stress test — maximum complexity
    run_benchmark("Stress — 16-track, 50 clips, 4K + effects sim", 16, 50, 500);

    println!("═══════════════════════════════════════════════════════════");
    println!("INTERPRETATION GUIDE:");
    println!("  DAG compile p95 ≤10µs → excellent (render graph overhead is negligible)");
    println!("  DAG compile p95 ≤100µs → acceptable (within budget)");
    println!("  DAG compile p95 >1000µs → needs optimization");
    println!();
    println!("Plan target (§6.4): scrub input-to-visible-frame p95 ≤70ms (mainstream)");
    println!("DAG compile contributes only the first few µs of that budget.");
    println!("The remaining budget is consumed by decode, GPU upload, and present.");
    println!("═══════════════════════════════════════════════════════════");
}

