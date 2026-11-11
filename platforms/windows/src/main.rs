//! Reddart Video Editor — Desktop Studio Application & Command-Line Tools
//!
//! Provides the primary desktop application entry point alongside a full-featured
//! command-line interface for headless rendering, media probing, template inspection,
//! and performance benchmarking.

mod app;
mod gpu;
pub mod ui;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn print_version() {
    println!("Reddart Video Editor v{}", env!("CARGO_PKG_VERSION"));
    println!("Platform: Windows 11 (x86_64-pc-windows-msvc)");
    println!("Engine: Rust 2024 | Direct3D 12 / Vulkan | wgpu 24 | egui 0.31 | 48 kHz WASAPI");
}

fn print_help() {
    print_version();
    println!();
    println!("USAGE:");
    println!("    reddart [OPTIONS] [PROJECT_FILE]");
    println!("    reddart <COMMAND> [ARGS...]");
    println!();
    println!("COMMANDS:");
    println!("    export <PROJECT> -o <OUTPUT> [--preset <PRESET>]");
    println!("                           Render project headlessly from terminal to MP4");
    println!("    probe <MEDIA_FILE>     Inspect media streams, codecs, resolution, and duration");
    println!("    templates              List all available built-in project starter templates");
    println!("    benchmark              Run timeline render graph latency benchmark suite");
    println!("    version                Print version and build environment metadata");
    println!("    help                   Print this command reference");
    println!();
    println!("ARGUMENTS:");
    println!("    [PROJECT_FILE]         Path to a .reddart project file to load on launch");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help             Print help information");
    println!("    -V, --version          Print version information");
    println!();
    println!("EXPORT PRESETS:");
    println!(
        "    youtube_1080p          1920x1080 @ 30.0 fps (default; NVENC / QuickSync / libx264)"
    );
    println!("    uhd_4k                 3840x2160 @ 30.0 fps UHD Master (NVENC / libx265)");
    println!(
        "    vertical_9_16_1080p    1080x1920 @ 30.0 fps 9:16 Vertical (TikTok, Reels, Shorts)"
    );
    println!(
        "    instagram_square       1080x1080 @ 30.0 fps 1:1 Square (Instagram, Social Cards)"
    );
    println!();
    println!("KEYBOARD SHORTCUTS IN STUDIO:");
    println!("    Space                  Toggle playback (Play / Pause)");
    println!(
        "    J / K / L              Shuttle backwards / Stop / Shuttle forward (1x, 2x, 4x, 8x)"
    );
    println!("    C                      Razor tool (split clip at playhead)");
    println!("    B                      Ripple trim tool (trim in/out with downstream ripple)");
    println!("    N                      Roll edit tool (adjust boundary preserving total time)");
    println!("    Y / U                  Slip / Slide tool (adjust content or position)");
    println!("    V                      Selection tool (move clips & drag edit points)");
    println!("    I / O                  Set Mark-In / Mark-Out render work area");
    println!("    M                      Add timeline cue marker");
    println!("    Ctrl+T                 Open starter project templates gallery");
    println!("    Ctrl+S                 Save project state to atomic WAL journal");
    println!("    Ctrl+E                 Open hardware video export modal");
}

fn handle_probe(path_str: &str) -> Result<()> {
    let path = Path::new(path_str);
    if !path.exists() {
        eprintln!("Error: media file does not exist: {}", path_str);
        std::process::exit(1);
    }

    println!("Inspecting container: {}", path.display());
    match rd_export::ExportJob::probe_file(path) {
        Ok(probe) => {
            println!(
                "  Video Stream: {}",
                if probe.has_video { "Detected" } else { "None" }
            );
            if let (Some(w), Some(h)) = (probe.width, probe.height) {
                println!("  Resolution:   {}x{}", w, h);
                let aspect = w as f64 / h as f64;
                let desc = if (aspect - 16.0 / 9.0).abs() < 0.05 {
                    "16:9 widescreen"
                } else if (aspect - 9.0 / 16.0).abs() < 0.05 {
                    "9:16 vertical"
                } else if (aspect - 1.0).abs() < 0.05 {
                    "1:1 square"
                } else {
                    "custom aspect ratio"
                };
                println!("  Aspect Ratio: {:.3} ({})", aspect, desc);
            }
            println!(
                "  Audio Stream: {}",
                if probe.has_audio { "Detected" } else { "None" }
            );
            if let Some(dur) = probe.duration_seconds {
                let smpte = app::format_smpte((dur * 1_000_000.0).round() as i64, 30.0);
                println!("  Duration:     {:.3}s (SMPTE: {})", dur, smpte);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: failed to probe media file via ffprobe: {}", e);
            std::process::exit(1);
        }
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn handle_templates() {
    let templates = rd_effects::template::TemplateSpec::starter_templates();
    println!(
        "Available Starter Project Templates ({} curated):",
        templates.len()
    );
    println!(
        "{:<28} {:<24} {:<8} {:<12} {:<10} {:<6}",
        "ID", "Name", "Aspect", "Resolution", "Duration", "Slots"
    );
    println!("{}", "-".repeat(90));
    for t in &templates {
        let g = gcd(t.aspect_width, t.aspect_height);
        let aspect_str = format!("{}:{}", t.aspect_width / g, t.aspect_height / g);
        let res_str = format!("{}x{}", t.aspect_width, t.aspect_height);
        let dur_s = t.duration_us as f64 / 1_000_000.0;
        let dur_str = format!("{:.1}s", dur_s);
        println!(
            "{:<28} {:<24} {:<8} {:<12} {:<10} {:<6}",
            t.id,
            t.name,
            aspect_str,
            res_str,
            dur_str,
            t.slots.len()
        );
    }
    println!("{}", "-".repeat(90));
    println!("Use 'reddart' to launch Studio and press Ctrl+T to instantiate any template.");
}

fn run_benchmark_suite() {
    println!("┌─────────────────────────────────────────────────────────┐");
    println!("│ Reddart DAG Compiler Latency Benchmark                  │");
    println!("└─────────────────────────────────────────────────────────┘");

    let configs: [(&str, usize, usize, usize); 4] = [
        ("Entry-Level PC (1 video track, 5 clips)", 1, 5, 1000),
        (
            "Mainstream Desktop (4 video tracks, 10 clips each)",
            4,
            10,
            1000,
        ),
        (
            "High-End Workstation (8 video tracks, 20 clips each)",
            8,
            20,
            1000,
        ),
        (
            "Stress Profile (16 video tracks, 50 clips each)",
            16,
            50,
            500,
        ),
    ];

    for (label, track_count, clips_per_track, iterations) in configs {
        let mut project = rd_core::Project::new(format!("Benchmark_{}", label));
        for t in 0..track_count {
            let tid = project
                .timeline
                .add_track(rd_core::TrackKind::Video, format!("V{}", t + 1));
            for c in 0..clips_per_track {
                let start_us = (c as i64) * 5_000_000;
                let clip = rd_core::Clip::new_video(
                    format!("media_{}_{}.mp4", t, c),
                    start_us,
                    0,
                    5_000_000,
                );
                project.timeline.track_mut(tid).unwrap().add_clip(clip);
            }
        }
        let total_dur = project.timeline.duration_us();

        let mut latencies_us: Vec<f64> = Vec::with_capacity(iterations);
        // Warmup
        for i in 0..30 {
            let t = (i as i64 * total_dur / 30).min(total_dur.saturating_sub(1));
            let _ = rd_render::graph::RenderGraph::compile_timeline(&project, t);
        }

        // Timed runs
        for i in 0..iterations {
            let time_us =
                ((i as i64 * total_dur) / iterations as i64).min(total_dur.saturating_sub(1));
            let t0 = Instant::now();
            let graph = rd_render::graph::RenderGraph::compile_timeline(&project, time_us);
            let _ = graph.topological_sort();
            let elapsed = t0.elapsed();
            latencies_us.push(elapsed.as_secs_f64() * 1_000_000.0);
        }

        latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = latencies_us[((0.50 * (latencies_us.len() - 1) as f64).round()) as usize];
        let p95 = latencies_us[((0.95 * (latencies_us.len() - 1) as f64).round()) as usize];
        let p99 = latencies_us[((0.99 * (latencies_us.len() - 1) as f64).round()) as usize];
        let mean: f64 = latencies_us.iter().sum::<f64>() / latencies_us.len() as f64;

        println!("Profile: {:45}", label);
        println!(
            "  Tracks: {:<2} | Clips: {:<3} | Iterations: {:<5} | p50: {:>6.2} µs | p95: {:>6.2} µs | p99: {:>6.2} µs | Mean: {:>6.2} µs",
            track_count,
            track_count * clips_per_track,
            iterations,
            p50,
            p95,
            p99,
            mean
        );
    }
}

fn handle_export(project_path_str: &str, output_path_str: &str, preset_name: &str) -> Result<()> {
    let proj_path = Path::new(project_path_str);
    if !proj_path.exists() {
        eprintln!("Error: project file does not exist: {}", project_path_str);
        std::process::exit(1);
    }
    let out_path = Path::new(output_path_str);

    let mut project = rd_core::Project::load_file(proj_path)
        .with_context(|| format!("Failed to read project file: {}", proj_path.display()))?;

    // If journal file exists, replay uncommitted transactions
    if let Ok(mut journal) = rd_core::ProjectJournal::open_or_create(proj_path) {
        match journal.replay_into(&mut project) {
            Ok(report) if report.valid_records_replayed > 0 => {
                println!(
                    "Replayed {} journaled edits from write-ahead log.",
                    report.valid_records_replayed
                );
            }
            _ => {}
        }
    }

    let preset = match preset_name {
        "uhd_4k" | "4k" | "youtube_4k" => rd_export::ExportPreset::uhd_4k(),
        "vertical_9_16_1080p" | "reels" | "reels_vertical" | "tiktok" => {
            rd_export::ExportPreset::vertical_9_16_1080p()
        }
        "instagram_square" | "square" | "square_social" => {
            rd_export::ExportPreset::instagram_square()
        }
        _ => rd_export::ExportPreset::youtube_1080p(),
    };

    let encoder = rd_export::select_encoder(&preset.video_codec);
    let dur_s = project.timeline.duration_us() as f64 / 1_000_000.0;

    println!("Executing Headless Export Pipeline:");
    println!("  Project Name:     {}", project.name);
    println!("  Output Location:  {}", out_path.display());
    println!(
        "  Format:           {} ({}x{} @ {:.0} fps)",
        preset.name, preset.width, preset.height, preset.fps
    );
    println!(
        "  Video Encoder:    {} (hardware acceleration active)",
        encoder
    );
    println!("  Audio Format:     48 kHz stereo AAC (320 kbps)");
    println!("  Sequence Length:  {:.2}s", dur_s);

    let (progress_tx, progress_rx) = std::sync::mpsc::channel();
    let req = rd_export::RenderRequest {
        project: &project,
        output_path: out_path,
        preset: &preset,
        grade: rd_export::ColorGradeSettings::default(),
    };

    let t0 = Instant::now();
    let render_result = std::thread::scope(|s| {
        let render_thread =
            s.spawn(move || rd_export::render_project_to_file(req, Some(&progress_tx), None));

        while let Ok(prog) = progress_rx.recv() {
            let pct = (prog.fraction * 100.0) as u32;
            let bar_len = 24;
            let filled = ((prog.fraction * bar_len as f32).round() as usize).min(bar_len);
            let bar: String = "=".repeat(filled) + &" ".repeat(bar_len - filled);
            eprint!(
                "\r[{}] {:>3}% ({}/{}) [{:.2}x real-time, ETA: {:.1}s] ",
                bar,
                pct,
                prog.current_frame,
                prog.total_frames,
                prog.speed_multiplier,
                prog.eta_seconds
            );
            let _ = std::io::Write::flush(&mut std::io::stderr());
            if prog.fraction >= 1.0 {
                break;
            }
        }
        eprintln!();

        render_thread.join()
    });

    match render_result {
        Ok(Ok(())) => {
            let elapsed = t0.elapsed().as_secs_f64();
            println!(
                "Export finished successfully in {:.2}s ({:.1}x real-time speed).",
                elapsed,
                dur_s / elapsed.max(0.001)
            );
            if let Ok(meta) = std::fs::metadata(out_path) {
                println!("File size: {:.2} MB", meta.len() as f64 / (1024.0 * 1024.0));
            }
            Ok(())
        }
        Ok(Err(e)) => {
            eprintln!("Export pipeline error: {}", e);
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("Export background thread encountered an unexpected panic.");
            std::process::exit(1);
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Check for flags
    if args.len() > 1 {
        match args[1].as_str() {
            "--help" | "-h" | "help" => {
                print_help();
                return Ok(());
            }
            "--version" | "-V" | "version" => {
                print_version();
                return Ok(());
            }
            "probe" => {
                if args.len() < 3 {
                    eprintln!("Usage: reddart probe <MEDIA_FILE>");
                    std::process::exit(1);
                }
                return handle_probe(&args[2]);
            }
            "templates" => {
                handle_templates();
                return Ok(());
            }
            "benchmark" => {
                run_benchmark_suite();
                return Ok(());
            }
            "export" => {
                if args.len() < 3 {
                    eprintln!(
                        "Usage: reddart export <PROJECT_FILE> -o <OUTPUT_PATH> [--preset <PRESET>]"
                    );
                    std::process::exit(1);
                }
                let project_file = &args[2];
                let mut output_file = "output.mp4";
                let mut preset = "youtube_1080p";

                let mut i = 3;
                while i < args.len() {
                    match args[i].as_str() {
                        "-o" | "--output" if i + 1 < args.len() => {
                            output_file = &args[i + 1];
                            i += 1;
                        }
                        "--preset" if i + 1 < args.len() => {
                            preset = &args[i + 1];
                            i += 1;
                        }
                        _ => {}
                    }
                    i += 1;
                }
                return handle_export(project_file, output_file, preset);
            }
            _ => {
                // If it looks like a path or project file
                let path = PathBuf::from(&args[1]);
                if path.extension().and_then(|s| s.to_str()) == Some("reddart") || path.exists() {
                    env_logger::Builder::from_env(
                        env_logger::Env::default().default_filter_or("info"),
                    )
                    .format_timestamp_millis()
                    .init();
                    return app::run_with_project(Some(path));
                }
            }
        }
    }

    // Default: Run the GUI application
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("Reddart Video Editor v{}", env!("CARGO_PKG_VERSION"));
    log::info!("Starting application event loop...");

    app::run()
}
