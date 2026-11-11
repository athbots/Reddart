//! Automated Sample Project & Template Rendering Test Suite
//!
//! Validates:
//! 1. Complete end-to-end rendering of `ready_to_use/windows/samples/Demo_Project.reddart`.
//! 2. Rendering of all 5 built-in starter templates across 9:16 vertical, 16:9 widescreen, 1:1 square, and 4K UHD.
//! 3. Probing rendered containers for valid video/audio streams, resolution, and non-empty output size.

use rd_core::Project;
use rd_effects::TemplateSpec;
use rd_export::{
    ColorGradeSettings, ExportJob, ExportPreset, RenderRequest, render_project_to_file,
};
use std::path::Path;

#[test]
fn test_render_sample_project_file() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let sample_path = workspace_root.join("ready_to_use/windows/samples/Demo_Project.reddart");
    assert!(
        sample_path.exists(),
        "Demo_Project.reddart must exist at {:?}",
        sample_path
    );

    let project =
        Project::load_file(sample_path).expect("Loading bundled Demo_Project.reddart must succeed");

    assert_eq!(project.resolution.width, 1920);
    assert_eq!(project.resolution.height, 1080);
    assert_eq!(project.frame_rate.num, 60);
    assert!(project.timeline.tracks().len() >= 2);

    let temp_dir = std::env::temp_dir().join(format!("rd_sample_render_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let out_file = temp_dir.join("sample_project_export.mp4");

    // Use 30 fps 1080p preset for swift test execution
    let mut preset = ExportPreset::youtube_1080p();
    preset.fps = 30.0;

    // Render a slice of the sample project
    let req = RenderRequest {
        project: &project,
        output_path: &out_file,
        preset: &preset,
        grade: ColorGradeSettings::default(),
    };

    render_project_to_file(req, None, None).expect("Rendering Demo_Project.reddart must succeed");

    assert!(out_file.exists(), "Rendered MP4 file must exist");
    let file_len = std::fs::metadata(&out_file).unwrap().len();
    assert!(file_len > 1024, "Output MP4 size must exceed 1 KB");

    // Probe the rendered container
    let probe = ExportJob::probe_file(&out_file).expect("Probing rendered sample must succeed");
    assert!(probe.has_video, "Exported sample must contain video stream");
    assert!(probe.has_audio, "Exported sample must contain audio stream");
    assert_eq!(probe.width, Some(1920));
    assert_eq!(probe.height, Some(1080));
    assert!(probe.duration_seconds.unwrap_or(0.0) > 0.0);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_render_all_five_starter_templates() {
    let templates = TemplateSpec::starter_templates();
    assert_eq!(
        templates.len(),
        5,
        "Must have exactly 5 curated starter templates"
    );

    let temp_dir =
        std::env::temp_dir().join(format!("rd_templates_render_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    for tmpl in &templates {
        println!(
            "Testing rendering for template: {} ({})",
            tmpl.name, tmpl.id
        );

        let media_paths: Vec<String> = (0..tmpl.slots.len())
            .map(|i| format!("test_media_slot_{}.mp4", i))
            .collect();

        let mut project = tmpl
            .instantiate(&media_paths)
            .expect("Template instantiation must succeed");

        // Clamp timeline duration to 0.5s for fast test execution
        if let Some(track) = project.timeline.tracks_mut().get_mut(0) {
            for clip in track.clips_mut() {
                clip.source_out_us = clip.source_in_us + 500_000;
            }
        }

        let out_file = temp_dir.join(format!("{}.mp4", tmpl.id));

        let preset = ExportPreset {
            name: tmpl.name.clone(),
            width: tmpl.aspect_width.min(1920), // Scale 4K down to 1080p for test speed
            height: if tmpl.aspect_width == 3840 {
                1080
            } else {
                tmpl.aspect_height.min(1920)
            },
            fps: 30.0,
            video_bitrate_kbps: 4_000,
            audio_bitrate_kbps: 192,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
        };

        let req = RenderRequest {
            project: &project,
            output_path: &out_file,
            preset: &preset,
            grade: ColorGradeSettings::default(),
        };

        render_project_to_file(req, None, None)
            .unwrap_or_else(|e| panic!("Failed to render template {}: {:?}", tmpl.id, e));

        assert!(
            out_file.exists(),
            "Exported template video must exist: {:?}",
            out_file
        );
        let meta = std::fs::metadata(&out_file).unwrap();
        assert!(
            meta.len() > 1024,
            "Exported template file must be non-empty"
        );

        let probe = ExportJob::probe_file(&out_file).expect("Probing template export must succeed");
        assert!(probe.has_video, "Template export must have video stream");
        assert!(probe.has_audio, "Template export must have audio stream");
        assert!(probe.duration_seconds.unwrap_or(0.0) > 0.0);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
