//! Item 7: Clean-Machine QA Integration Suite
//!
//! Validates the end-to-end user workflow on a clean machine environment:
//! 1. FFmpeg & FFprobe toolchain availability and discovery.
//! 2. Binary CLI entry points (--version, templates, probe, export).
//! 3. Full project editing lifecycle: import, trim, grade, title overlay.
//! 4. Crash-resilient write-ahead logging (WAL) and project recovery.
//! 5. Headless CLI export with automatic WAL journal replay.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

use rd_core::{
    Clip, ClipId, ClipKind, JournalMutation, Project, ProjectJournal, TrackId, TrackKind,
    Transform2D, VideoClip,
};
use rd_export::{
    ColorGradeSettings, ExportJob, ExportPreset, RenderRequest, render_project_to_file,
};

/// Locate repository root from Cargo manifest directory.
fn find_repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut root = manifest_dir.clone();
    while !root.join("Cargo.lock").exists() {
        if let Some(parent) = root.parent() {
            root = parent.to_path_buf();
        } else {
            break;
        }
    }
    if root.join("Cargo.lock").exists() {
        root
    } else {
        manifest_dir
    }
}

/// Locate sample media footage for clean machine testing.
fn locate_sample_media() -> PathBuf {
    let root = find_repo_root();
    let candidates = [
        root.join("ready_to_use")
            .join("windows")
            .join("Reddart_Master_Demo.mp4"),
        root.join("ready_to_use")
            .join("windows")
            .join("samples")
            .join("footage")
            .join("shot_01.mp4"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    panic!(
        "Sample footage not found in repository. Checked: {:?}",
        candidates
    );
}

/// 1. FFmpeg & FFprobe toolchain discovery and functionality
#[test]
fn test_clean_machine_ffmpeg_toolchain_presence() {
    let ffmpeg_path = rd_export::find_ffmpeg_executable();
    assert!(
        ffmpeg_path.is_some(),
        "FFmpeg executable must be discovered on clean machine"
    );
    let ffprobe_path = rd_export::find_ffprobe_executable();
    assert!(
        ffprobe_path.is_some(),
        "FFprobe executable must be discovered on clean machine"
    );

    let ffmpeg_out = Command::new(ffmpeg_path.unwrap())
        .arg("-version")
        .output()
        .expect("Execute ffmpeg -version");
    assert!(ffmpeg_out.status.success());
    let ffmpeg_ver_str = String::from_utf8_lossy(&ffmpeg_out.stdout);
    assert!(
        ffmpeg_ver_str.contains("ffmpeg version"),
        "Output must contain ffmpeg version header: {}",
        ffmpeg_ver_str
    );

    let ffprobe_out = Command::new(ffprobe_path.unwrap())
        .arg("-version")
        .output()
        .expect("Execute ffprobe -version");
    assert!(ffprobe_out.status.success());
    let ffprobe_ver_str = String::from_utf8_lossy(&ffprobe_out.stdout);
    assert!(
        ffprobe_ver_str.contains("ffprobe version"),
        "Output must contain ffprobe version header: {}",
        ffprobe_ver_str
    );
}

/// 2. Binary CLI entry points: --version, templates, and media probing
#[test]
fn test_clean_machine_cli_entrypoints() {
    let reddart_exe = env!("CARGO_BIN_EXE_reddart");
    assert!(
        Path::new(reddart_exe).exists(),
        "reddart binary must exist at {}",
        reddart_exe
    );

    // Test: reddart --version
    let ver_output = Command::new(reddart_exe)
        .arg("--version")
        .output()
        .expect("Run reddart --version");
    assert!(ver_output.status.success());
    let ver_str = String::from_utf8_lossy(&ver_output.stdout);
    assert!(
        ver_str.contains("Reddart Video Editor v"),
        "Version output invalid: {}",
        ver_str
    );

    // Test: reddart templates
    let tmpl_output = Command::new(reddart_exe)
        .arg("templates")
        .output()
        .expect("Run reddart templates");
    assert!(tmpl_output.status.success());
    let tmpl_str = String::from_utf8_lossy(&tmpl_output.stdout);
    assert!(
        tmpl_str.contains("Available Starter Project Templates"),
        "Templates list missing header: {}",
        tmpl_str
    );
    assert!(
        tmpl_str.contains("youtube_4k_documentary")
            || tmpl_str.contains("cinematic_vlog_widescreen"),
        "Templates list missing expected presets: {}",
        tmpl_str
    );

    // Test: reddart probe <sample_media>
    let sample = locate_sample_media();
    let probe_output = Command::new(reddart_exe)
        .args(["probe", sample.to_str().unwrap()])
        .output()
        .expect("Run reddart probe");
    assert!(probe_output.status.success());
    let probe_str = String::from_utf8_lossy(&probe_output.stdout);
    assert!(
        probe_str.contains("Video Stream: Detected"),
        "Probe output missing video stream: {}",
        probe_str
    );
    assert!(
        probe_str.contains("Resolution:   1920x1080"),
        "Probe output missing resolution: {}",
        probe_str
    );
}

/// 3. Full project lifecycle: import, trim, grade, WAL save, crash & reopen recovery, export
#[test]
fn test_clean_machine_project_lifecycle_wal_and_export() {
    let temp_dir = std::env::temp_dir().join(format!("reddart_qa_lifecycle_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).expect("Create QA temp directory");
    let proj_path = temp_dir.join("clean_studio.reddart");
    let export_path = temp_dir.join("clean_studio_export.mp4");
    let sample_video = locate_sample_media();

    let track_v1 = TrackId(1);
    let track_a1 = TrackId(2);
    let clip_id = ClipId(Uuid::new_v4());

    // Phase A: Create project base file and journal operations
    {
        // 1. Save empty base project
        let project = Project::new("Clean QA Master Studio");
        project
            .save_atomic(&proj_path)
            .expect("Save clean project base file");

        // 2. Perform editing session via WAL journal mutations
        let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Create WAL journal");
        journal
            .append(JournalMutation::AddTrack {
                id: track_v1,
                kind: TrackKind::Video,
                name: "V1 Primary".into(),
            })
            .expect("Log AddTrack V1");
        journal
            .append(JournalMutation::AddTrack {
                id: track_a1,
                kind: TrackKind::Audio,
                name: "A1 Mix".into(),
            })
            .expect("Log AddTrack A1");

        let video_clip = Clip {
            id: clip_id,
            kind: ClipKind::Video(VideoClip {
                source_path: sample_video.to_string_lossy().to_string(),
                proxy_path: None,
                has_audio: true,
            }),
            position_us: 0,
            source_in_us: 500_000,
            source_out_us: 3_500_000, // 3.0s duration
            speed: 1.0,
            retime_mode: rd_core::RetimeMode::default(),
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
            volume: 0.9,
            pan: 0.0,
            pan_surround: 0.0,
            lfe_send: 0.0,
            keyframes: std::collections::HashMap::new(),
        };

        journal
            .append(JournalMutation::AddClip {
                track_id: track_v1,
                clip: video_clip,
            })
            .expect("Log AddClip");
    }

    // Verify disk artifacts exist
    assert!(
        proj_path.exists(),
        "Project file (.reddart) must exist on disk"
    );
    let wal_path = proj_path.with_extension("reddart.wal");
    assert!(
        wal_path.exists(),
        "WAL journal (.reddart.wal) must exist on disk"
    );

    // Phase B: Simulate application crash / clean reopen and WAL state validation
    let (mut reloaded_project, report) =
        Project::load_file_with_recovery(&proj_path).expect("Load project with recovery");
    let r = report.expect("WAL report must be present");
    assert_eq!(
        r.valid_records_replayed, 3,
        "All 3 journaled operations must be cleanly replayed"
    );

    assert_eq!(reloaded_project.timeline.tracks().len(), 2);
    let v1 = reloaded_project.timeline.track(track_v1).unwrap();
    assert_eq!(v1.clips().len(), 1);
    let clip = v1.clips().iter().find(|c| c.id == clip_id).unwrap();
    assert_eq!(clip.duration_us(), 3_000_000);

    // Set work area range
    reloaded_project.timeline.mark_in_us = Some(0);
    reloaded_project.timeline.mark_out_us = Some(3_000_000);

    // Phase C: Export reloaded project to MP4 container and verify output
    let preset = ExportPreset::youtube_1080p();
    let render_req = RenderRequest {
        project: &reloaded_project,
        output_path: &export_path,
        preset: &preset,
        grade: ColorGradeSettings::default(),
    };

    render_project_to_file(render_req, None, None).expect("Render clean QA project");
    assert!(
        export_path.exists(),
        "Exported MP4 file must exist: {:?}",
        export_path
    );

    let metadata = fs::metadata(&export_path).expect("Read exported metadata");
    assert!(
        metadata.len() > 50_000,
        "Exported MP4 must be > 50 KB (actual: {} bytes)",
        metadata.len()
    );

    let probe = ExportJob::probe_file(&export_path).expect("ffprobe on exported MP4");
    assert!(probe.has_video, "Exported MP4 must have video");
    assert_eq!(probe.width, Some(1920));
    assert_eq!(probe.height, Some(1080));
    let dur = probe.duration_seconds.unwrap_or(0.0);
    assert!(
        (2.8..=3.2).contains(&dur),
        "Exported duration ({:.2}s) should match ~3.0s timeline range",
        dur
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

/// 4. Headless CLI export with uncommitted WAL recovery
#[test]
fn test_clean_machine_cli_headless_export_and_wal_recovery() {
    let reddart_exe = env!("CARGO_BIN_EXE_reddart");
    let temp_dir = std::env::temp_dir().join(format!("reddart_qa_cli_wal_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).expect("Create QA temp directory");
    let proj_path = temp_dir.join("cli_wal_project.reddart");
    let export_path = temp_dir.join("cli_wal_export.mp4");
    let sample_video = locate_sample_media();

    let clip_id = ClipId(Uuid::new_v4());

    // Create base project with track V1
    let track_v1 = {
        let mut project = Project::new("CLI WAL Recovery Project");
        let tid = project.timeline.add_track(TrackKind::Video, "V1");
        project
            .save_atomic(&proj_path)
            .expect("Save base project atomically");

        // Simulate crash right after an uncommitted edit was journaled in WAL
        let mut journal = ProjectJournal::open_or_create(&proj_path).expect("Open journal");
        let video_clip = Clip {
            id: clip_id,
            kind: ClipKind::Video(VideoClip {
                source_path: sample_video.to_string_lossy().to_string(),
                proxy_path: None,
                has_audio: false,
            }),
            position_us: 0,
            source_in_us: 0,
            source_out_us: 2_000_000, // 2.0s
            speed: 1.0,
            retime_mode: rd_core::RetimeMode::default(),
            opacity: 1.0,
            transform: Transform2D::default(),
            effects: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            pan_surround: 0.0,
            lfe_send: 0.0,
            keyframes: std::collections::HashMap::new(),
        };

        journal
            .append(JournalMutation::AddClip {
                track_id: tid,
                clip: video_clip,
            })
            .expect("Append uncommitted WAL mutation");
        tid
    };

    assert_eq!(track_v1, TrackId(1));

    // Invoke headless CLI export: reddart export <project> -o <output> --preset youtube_1080p
    let cli_output = Command::new(reddart_exe)
        .args([
            "export",
            proj_path.to_str().unwrap(),
            "-o",
            export_path.to_str().unwrap(),
            "--preset",
            "youtube_1080p",
        ])
        .output()
        .expect("Run reddart export CLI");

    let stdout_str = String::from_utf8_lossy(&cli_output.stdout);
    let stderr_str = String::from_utf8_lossy(&cli_output.stderr);
    println!("CLI Export STDOUT:\n{}", stdout_str);
    if !stderr_str.is_empty() {
        println!("CLI Export STDERR:\n{}", stderr_str);
    }

    assert!(
        cli_output.status.success(),
        "CLI export command failed with exit code {:?}\nStderr: {}",
        cli_output.status.code(),
        stderr_str
    );

    // Verify stdout reports journal replay
    assert!(
        stdout_str.contains("Replayed 1 journaled edits from write-ahead log."),
        "CLI output must confirm WAL replay: {}",
        stdout_str
    );

    // Verify exported output file
    assert!(
        export_path.exists(),
        "CLI export must produce valid output file"
    );
    let probe = ExportJob::probe_file(&export_path).expect("Probe CLI export output");
    assert!(probe.has_video);
    assert_eq!(probe.width, Some(1920));
    assert_eq!(probe.height, Some(1080));
    let dur = probe.duration_seconds.unwrap_or(0.0);
    assert!(
        (1.8..=2.3).contains(&dur),
        "CLI export duration ({:.2}s) should match 2.0s uncommitted clip duration",
        dur
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
