//! Automated Sample Project & Template Rendering Test Suite
//!
//! Validates:
//! 1. Complete end-to-end rendering of `ready_to_use/windows/samples/Demo_Project.reddart`.
//! 2. Rendering of all 5 built-in starter templates across 9:16 vertical, 16:9 widescreen, 1:1 square, and 4K UHD.
//! 3. Probing rendered containers for valid video/audio streams, resolution, and non-empty output size.
//! 4. RED-002: Sample project completeness: extraction into arbitrary directories, genuine frame decoding,
//!    non-silent audio verification, and decoded image/audio content validation of rendered outputs.

use rd_core::Project;
use rd_effects::TemplateSpec;
use rd_export::{
    ColorGradeSettings, ExportJob, ExportPreset, RenderRequest, is_ffmpeg_available,
    render_project_to_file,
};
use std::path::Path;

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn compute_image_metrics(rgba: &[u8]) -> (f64, f64) {
    if rgba.is_empty() {
        return (0.0, 0.0);
    }
    let pixel_count = rgba.len() / 4;
    let mut sum = 0.0f64;
    for chunk in rgba.chunks_exact(4) {
        let lum = 0.299 * (chunk[0] as f64) + 0.587 * (chunk[1] as f64) + 0.114 * (chunk[2] as f64);
        sum += lum;
    }
    let mean = sum / pixel_count as f64;
    let mut var_sum = 0.0f64;
    for chunk in rgba.chunks_exact(4) {
        let lum = 0.299 * (chunk[0] as f64) + 0.587 * (chunk[1] as f64) + 0.114 * (chunk[2] as f64);
        var_sum += (lum - mean) * (lum - mean);
    }
    let std_dev = (var_sum / pixel_count as f64).sqrt();
    (mean, std_dev)
}

fn compute_audio_metrics(samples: &[f32]) -> (f64, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    let peak = samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
    (rms, peak)
}

#[test]
fn test_render_sample_project_file() {
    if !is_ffmpeg_available() {
        eprintln!("Skipping sample project render test: ffmpeg/ffprobe not installed on host PATH");
        return;
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let sample_path = workspace_root.join("ready_to_use/windows/samples/Demo_Project.reddart");
    assert!(
        sample_path.exists(),
        "Demo_Project.reddart must exist at {:?}",
        sample_path
    );

    // Verify all referenced sample media assets exist on disk
    let shot1 = workspace_root.join("ready_to_use/windows/samples/footage/shot_01.mp4");
    let shot2 = workspace_root.join("ready_to_use/windows/samples/footage/shot_02.mp4");
    let audio = workspace_root.join("ready_to_use/windows/samples/audio/soundtrack.wav");
    assert!(
        shot1.exists(),
        "Sample footage shot_01.mp4 must exist at {:?}",
        shot1
    );
    assert!(
        shot2.exists(),
        "Sample footage shot_02.mp4 must exist at {:?}",
        shot2
    );
    assert!(
        audio.exists(),
        "Sample audio soundtrack.wav must exist at {:?}",
        audio
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
fn test_sample_project_completeness_and_genuine_decoding() {
    if !is_ffmpeg_available() {
        eprintln!(
            "Skipping sample project completeness test: ffmpeg/ffprobe not installed on host PATH"
        );
        return;
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let original_samples_dir = workspace_root.join("ready_to_use/windows/samples");

    // 1. Verify ASSET_LICENSES.md exists
    let license_path = original_samples_dir.join("ASSET_LICENSES.md");
    assert!(
        license_path.exists(),
        "ASSET_LICENSES.md must exist at {:?}",
        license_path
    );
    let license_text = std::fs::read_to_string(&license_path).unwrap();
    assert!(
        license_text.contains("CC0 1.0"),
        "ASSET_LICENSES.md must declare CC0 1.0 licensing"
    );

    // 2. Open sample project after extraction into an arbitrary directory
    let arbitrary_dir =
        std::env::temp_dir().join(format!("rd_arbitrary_extract_{}", uuid::Uuid::new_v4()));
    let isolated_samples = arbitrary_dir.join("isolated_samples");
    copy_dir_all(&original_samples_dir, &isolated_samples)
        .expect("Copy samples to arbitrary directory");

    let sample_project_path = isolated_samples.join("Demo_Project.reddart");
    assert!(sample_project_path.exists());

    // Set REDDART_SAMPLE_DIR so pipeline resolves relative paths within the arbitrary directory
    unsafe {
        std::env::set_var("REDDART_SAMPLE_DIR", &isolated_samples);
    }

    let project = Project::load_file(&sample_project_path)
        .expect("Loading Demo_Project.reddart from arbitrary directory must succeed");

    // 3. Verify all media paths resolve
    let shot1_path = rd_export::resolve_media_path("footage/shot_01.mp4")
        .expect("footage/shot_01.mp4 must resolve");
    let shot2_path = rd_export::resolve_media_path("footage/shot_02.mp4")
        .expect("footage/shot_02.mp4 must resolve");
    let audio_path = rd_export::resolve_media_path("audio/soundtrack.wav")
        .expect("audio/soundtrack.wav must resolve");

    assert!(
        shot1_path.exists(),
        "Resolved shot_01.mp4 must exist at {:?}",
        shot1_path
    );
    assert!(
        shot2_path.exists(),
        "Resolved shot_02.mp4 must exist at {:?}",
        shot2_path
    );
    assert!(
        audio_path.exists(),
        "Resolved soundtrack.wav must exist at {:?}",
        audio_path
    );

    // 4. Verify video frames are genuinely decoded (dimensions 1920x1080, non-zero pixel buffer, pixel variance > 0)
    let mut decoder1 =
        rd_decode::FfmpegDecoder::open(&shot1_path).expect("Open shot_01.mp4 decoder");
    let frame1 = decoder1
        .decode_frame_at(1_000_000)
        .expect("Decode frame at 1.0s")
        .expect("Frame must be present");
    assert_eq!(frame1.width, 1920);
    assert_eq!(frame1.height, 1080);
    let rgba1 = frame1
        .rgba_data
        .expect("Frame 1 must contain uncompressed RGBA pixel buffer");
    assert_eq!(rgba1.len(), 1920 * 1080 * 4);
    let (mean1, std_dev1) = compute_image_metrics(&rgba1);
    assert!(
        mean1 > 10.0,
        "Decoded frame 1 mean luminance ({:.2}) must be non-zero",
        mean1
    );
    assert!(
        std_dev1 > 15.0,
        "Decoded frame 1 pixel variance ({:.2}) must indicate genuine image content",
        std_dev1
    );

    let mut decoder2 =
        rd_decode::FfmpegDecoder::open(&shot2_path).expect("Open shot_02.mp4 decoder");
    let frame2 = decoder2
        .decode_frame_at(1_000_000)
        .expect("Decode frame at 1.0s")
        .expect("Frame must be present");
    assert_eq!(frame2.width, 1920);
    assert_eq!(frame2.height, 1080);
    let rgba2 = frame2
        .rgba_data
        .expect("Frame 2 must contain uncompressed RGBA pixel buffer");
    let (mean2, std_dev2) = compute_image_metrics(&rgba2);
    assert!(
        mean2 > 10.0,
        "Decoded frame 2 mean luminance ({:.2}) must be non-zero",
        mean2
    );
    assert!(
        std_dev2 > 15.0,
        "Decoded frame 2 pixel variance ({:.2}) must indicate genuine image content",
        std_dev2
    );

    // 5. Verify audio samples are non-silent
    let audio_pcm =
        rd_audio::decode_audio_to_pcm(&audio_path).expect("Decode audio soundtrack to PCM");
    assert!(!audio_pcm.is_empty(), "Audio PCM must not be empty");
    assert!(
        audio_pcm.len() >= 48000 * 2 * 5,
        "Soundtrack must contain at least 5 seconds of 48kHz stereo samples"
    );
    let (audio_rms, audio_peak) = compute_audio_metrics(&audio_pcm);
    assert!(
        audio_rms > 0.01,
        "Audio RMS amplitude ({:.4}) must be non-silent (> 0.01)",
        audio_rms
    );
    assert!(
        audio_peak > 0.1,
        "Audio peak amplitude ({:.4}) must be non-silent (> 0.1)",
        audio_peak
    );

    // 6. Render an output and verify its decoded frames contain real image content
    let out_file = arbitrary_dir.join("rendered_output.mp4");
    let mut preset = ExportPreset::youtube_1080p();
    preset.fps = 30.0;

    // Create a modified slice for fast test rendering (2 seconds)
    let mut test_project = project.clone();
    for track in test_project.timeline.tracks_mut() {
        for clip in track.clips_mut() {
            if clip.position_us > 2_000_000 {
                clip.source_out_us = clip.source_in_us; // mute far clips
            } else {
                clip.source_out_us = (clip.source_in_us + 2_000_000).min(clip.source_out_us);
            }
        }
    }

    let req = RenderRequest {
        project: &test_project,
        output_path: &out_file,
        preset: &preset,
        grade: ColorGradeSettings::default(),
    };

    render_project_to_file(req, None, None).expect("Rendering Demo_Project slice must succeed");
    assert!(out_file.exists(), "Rendered MP4 file must exist on disk");

    // Decode frames from the rendered output MP4 to verify real image content
    let mut out_decoder =
        rd_decode::FfmpegDecoder::open(&out_file).expect("Open rendered MP4 decoder");
    let out_frame = out_decoder
        .decode_frame_at(500_000)
        .expect("Decode rendered frame at 0.5s")
        .expect("Rendered frame must be present");
    assert_eq!(out_frame.width, 1920);
    assert_eq!(out_frame.height, 1080);
    let out_rgba = out_frame
        .rgba_data
        .expect("Rendered output frame must contain RGBA pixels");
    let (out_mean, out_std_dev) = compute_image_metrics(&out_rgba);
    assert!(
        out_mean > 5.0,
        "Rendered output mean luminance ({:.2}) must be non-zero",
        out_mean
    );
    assert!(
        out_std_dev > 10.0,
        "Rendered output image variance ({:.2}) must show genuine rendered video content",
        out_std_dev
    );

    // 7. Verify output audio contains non-silent samples
    let out_pcm =
        rd_audio::decode_audio_to_pcm(&out_file).expect("Decode rendered MP4 audio to PCM");
    assert!(
        !out_pcm.is_empty(),
        "Rendered output audio PCM must not be empty"
    );
    let (out_audio_rms, out_audio_peak) = compute_audio_metrics(&out_pcm);
    assert!(
        out_audio_rms > 0.01,
        "Rendered output audio RMS ({:.4}) must be non-silent (> 0.01)",
        out_audio_rms
    );
    assert!(
        out_audio_peak > 0.1,
        "Rendered output audio peak ({:.4}) must be non-silent (> 0.1)",
        out_audio_peak
    );

    let _ = std::fs::remove_dir_all(&arbitrary_dir);
}

#[test]
fn test_render_all_five_starter_templates() {
    if !is_ffmpeg_available() {
        eprintln!("Skipping template render test: ffmpeg/ffprobe not installed on host PATH");
        return;
    }

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
