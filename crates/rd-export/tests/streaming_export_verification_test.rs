//! Item 6: Streaming Export Verification Suite
//!
//! Validates:
//! 1. 1080p streaming export peak memory remains strictly bounded under 400 MB.
//! 2. 4K UHD (3840x2160) streaming export peak memory remains strictly bounded under 800 MB.
//! 3. Decoded output frame content validation (genuine non-black pixel values and dimensions).
//! 4. Work-area (Mark-In / Mark-Out) range export duration and temporal accuracy.
//! 5. Memory invariance over time proving O(1) buffer consumption without accumulation.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rd_core::{Clip, Project, TrackKind};
use rd_export::{
    ColorGradeSettings, ExportJob, ExportPreset, RenderRequest, is_ffmpeg_available,
    render_project_to_file,
};

/// Retrieve current resident set size (RSS) in bytes across Windows, Linux, and macOS.
#[cfg(windows)]
fn get_current_rss_bytes() -> usize {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct PROCESS_MEMORY_COUNTERS {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    type FnGetProcessMemoryCounters = unsafe extern "system" fn(
        process: isize,
        counters: *mut PROCESS_MEMORY_COUNTERS,
        cb: u32,
    ) -> i32;

    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetModuleHandleA(lpModuleName: *const std::ffi::c_char) -> isize;
        fn LoadLibraryA(lpLibFileName: *const std::ffi::c_char) -> isize;
        fn GetProcAddress(hModule: isize, lpProcName: *const std::ffi::c_char) -> *const ();
    }

    unsafe {
        let h_kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
        let mut fn_ptr = if h_kernel32 != 0 {
            GetProcAddress(h_kernel32, c"K32GetProcessMemoryCounters".as_ptr())
        } else {
            std::ptr::null()
        };

        if fn_ptr.is_null() {
            let h_psapi = LoadLibraryA(c"psapi.dll".as_ptr());
            if h_psapi != 0 {
                fn_ptr = GetProcAddress(h_psapi, c"GetProcessMemoryInfo".as_ptr());
            }
        }

        if !fn_ptr.is_null() {
            let func: FnGetProcessMemoryCounters = std::mem::transmute(fn_ptr);
            let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            let proc = GetCurrentProcess();
            if func(proc, &mut counters, counters.cb) != 0 {
                return counters.WorkingSetSize;
            }
        }
        0
    }
}

#[cfg(target_os = "linux")]
fn get_current_rss_bytes() -> usize {
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        let parts: Vec<&str> = statm.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(pages) = parts[1].parse::<usize>() {
                return pages * 4096;
            }
        }
    }
    0
}

#[cfg(target_os = "macos")]
fn get_current_rss_bytes() -> usize {
    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [u32; 2],
        system_time: [u32; 2],
        policy: i32,
        suspend_count: i32,
    }
    unsafe extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut MachTaskBasicInfo,
            task_info_outCnt: *mut u32,
        ) -> i32;
    }
    unsafe {
        let mut info: MachTaskBasicInfo = std::mem::zeroed();
        let mut count = (std::mem::size_of::<MachTaskBasicInfo>() / 4) as u32;
        if task_info(mach_task_self(), 20, &mut info, &mut count) == 0 {
            info.resident_size as usize
        } else {
            0
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn get_current_rss_bytes() -> usize {
    0
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

fn require_or_skip_ffmpeg(test_name: &str) -> bool {
    if !is_ffmpeg_available() {
        let is_ci = std::env::var("CI").is_ok() || std::env::var("REDDART_REQUIRE_FFMPEG").is_ok();
        if is_ci {
            panic!(
                "CRITICAL: FFmpeg and FFprobe are required in CI environment (test '{}'), but were not found on system PATH!",
                test_name
            );
        } else {
            eprintln!(
                "[SKIPPED TEST] {}: FFmpeg/FFprobe not found on local PATH. Set REDDART_REQUIRE_FFMPEG=1 to enforce.",
                test_name
            );
            return false;
        }
    }
    true
}

fn locate_sample_media() -> (PathBuf, PathBuf) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let video_path = workspace_root.join("ready_to_use/windows/Reddart_Master_Demo.mp4");
    let audio_path = workspace_root.join("ready_to_use/windows/samples/audio/soundtrack.wav");

    // Fallback to sample shots if Master Demo is absent
    let final_video = if video_path.exists() {
        video_path
    } else {
        workspace_root.join("ready_to_use/windows/samples/footage/shot_01.mp4")
    };

    (final_video, audio_path)
}

// ---------------------------------------------------------------------------
// 1. 1080p Long Streaming Export — Peak RSS < 400 MB & Content Validation
// ---------------------------------------------------------------------------
#[test]
fn test_streaming_export_1080p_memory_ceiling_and_content_verification() {
    if !require_or_skip_ffmpeg(
        "test_streaming_export_1080p_memory_ceiling_and_content_verification",
    ) {
        return;
    }

    let (video_path, audio_path) = locate_sample_media();
    if !video_path.exists() {
        eprintln!(
            "Sample video asset {:?} not found, skipping integration test",
            video_path
        );
        return;
    }

    // Build 5.0 second project (150 frames @ 30 fps)
    let mut project = Project::new("Streaming Export 1080p Test");
    let v_track = project.timeline.add_track(TrackKind::Video, "Video 1");
    let clip_v = Clip::new_video(video_path.to_str().unwrap(), 0, 0, 5_000_000);
    project
        .timeline
        .track_mut(v_track)
        .unwrap()
        .add_clip(clip_v);

    if audio_path.exists() {
        let a_track = project.timeline.add_track(TrackKind::Audio, "Audio 1");
        let clip_a = Clip::new_audio(audio_path.to_str().unwrap(), 0, 5_000_000);
        project
            .timeline
            .track_mut(a_track)
            .unwrap()
            .add_clip(clip_a);
    }

    let preset = ExportPreset::youtube_1080p();
    let temp_dir =
        std::env::temp_dir().join(format!("reddart_stream_1080p_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Create temp dir");
    let out_file = temp_dir.join("export_1080p_streaming.mp4");

    // Monitor peak RSS concurrently during the render
    let peak_rss = Arc::new(AtomicUsize::new(get_current_rss_bytes()));
    let monitor_done = Arc::new(AtomicBool::new(false));

    let peak_clone = peak_rss.clone();
    let done_clone = monitor_done.clone();
    let monitor_thread = thread::spawn(move || {
        while !done_clone.load(Ordering::Relaxed) {
            let current = get_current_rss_bytes();
            peak_clone.fetch_max(current, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(20));
        }
    });

    let grade = ColorGradeSettings {
        exposure: 0.1,
        contrast: 1.05,
        saturation: 1.05,
        temperature: 0.0,
        tint: 0.0,
        lut_idx: 0,
    };

    let req = RenderRequest {
        project: &project,
        output_path: &out_file,
        preset: &preset,
        grade,
    };

    let start_instant = Instant::now();
    let render_res = render_project_to_file(req, None, None);
    let render_duration = start_instant.elapsed();

    monitor_done.store(true, Ordering::Relaxed);
    let _ = monitor_thread.join();

    assert!(
        render_res.is_ok(),
        "1080p render_project_to_file failed: {:?}",
        render_res
    );
    assert!(out_file.exists(), "Rendered 1080p MP4 must exist");
    assert!(
        out_file.metadata().unwrap().len() > 20_000,
        "Rendered 1080p file size must be non-trivial"
    );

    // 1. Peak RSS Verification: Must stay well below 400 MB
    let peak_bytes = peak_rss.load(Ordering::Relaxed);
    let peak_mb = peak_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "1080p Streaming Export: rendered 150 frames in {:.2?} (Peak RSS: {:.2} MB)",
        render_duration, peak_mb
    );
    if peak_bytes > 0 {
        assert!(
            peak_mb < 400.0,
            "1080p export peak memory ({:.2} MB) exceeded 400 MB threshold!",
            peak_mb
        );
    }

    // 2. Container Probe Verification
    let probe = ExportJob::probe_file(&out_file).expect("ffprobe must succeed on 1080p export");
    assert!(probe.has_video, "Exported file must contain video stream");
    assert_eq!(probe.width, Some(1920), "Exported width must be 1920");
    assert_eq!(probe.height, Some(1080), "Exported height must be 1080");
    let dur = probe.duration_seconds.unwrap_or(0.0);
    assert!(
        (4.7..=5.3).contains(&dur),
        "Duration ({:.2}s) must be approximately 5.0s",
        dur
    );

    // 3. Genuine Content Verification: Decode frames at t=0.5s and t=4.0s
    let mut decoder =
        rd_decode::FfmpegDecoder::open(&out_file).expect("Open rendered 1080p decoder");

    // Frame at 0.5s
    let f1 = decoder
        .decode_frame_at(500_000)
        .expect("Decode frame at 0.5s")
        .expect("Frame at 0.5s present");
    assert_eq!(f1.width, 1920);
    assert_eq!(f1.height, 1080);
    let (mean1, std1) = compute_image_metrics(f1.rgba_data.as_ref().unwrap());
    assert!(
        mean1 > 3.0,
        "Frame at 0.5s luminance ({:.2}) must be non-zero",
        mean1
    );
    assert!(
        std1 > 3.0,
        "Frame at 0.5s variance ({:.2}) must show genuine content",
        std1
    );

    // Frame at 4.0s
    let f2 = decoder
        .decode_frame_at(4_000_000)
        .expect("Decode frame at 4.0s")
        .expect("Frame at 4.0s present");
    assert_eq!(f2.width, 1920);
    assert_eq!(f2.height, 1080);
    let (mean2, std2) = compute_image_metrics(f2.rgba_data.as_ref().unwrap());
    assert!(
        mean2 > 3.0,
        "Frame at 4.0s luminance ({:.2}) must be non-zero",
        mean2
    );
    assert!(
        std2 > 3.0,
        "Frame at 4.0s variance ({:.2}) must show genuine content",
        std2
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 2. 4K UHD Streaming Export — Peak RSS < 800 MB & Content Validation
// ---------------------------------------------------------------------------
#[test]
fn test_streaming_export_4k_memory_ceiling_and_content_verification() {
    if !require_or_skip_ffmpeg("test_streaming_export_4k_memory_ceiling_and_content_verification") {
        return;
    }

    let (video_path, audio_path) = locate_sample_media();
    if !video_path.exists() {
        eprintln!(
            "Sample video asset {:?} not found, skipping integration test",
            video_path
        );
        return;
    }

    // Build 2.0 second 4K project (60 frames @ 30 fps)
    let mut project = Project::new("Streaming Export 4K Test");
    let v_track = project.timeline.add_track(TrackKind::Video, "Video 1");
    let clip_v = Clip::new_video(video_path.to_str().unwrap(), 0, 0, 2_000_000);
    project
        .timeline
        .track_mut(v_track)
        .unwrap()
        .add_clip(clip_v);

    if audio_path.exists() {
        let a_track = project.timeline.add_track(TrackKind::Audio, "Audio 1");
        let clip_a = Clip::new_audio(audio_path.to_str().unwrap(), 0, 2_000_000);
        project
            .timeline
            .track_mut(a_track)
            .unwrap()
            .add_clip(clip_a);
    }

    let preset = ExportPreset::uhd_4k();
    let temp_dir = std::env::temp_dir().join(format!("reddart_stream_4k_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Create temp dir");
    let out_file = temp_dir.join("export_4k_streaming.mp4");

    // Monitor peak RSS concurrently during the 4K render
    let peak_rss = Arc::new(AtomicUsize::new(get_current_rss_bytes()));
    let monitor_done = Arc::new(AtomicBool::new(false));

    let peak_clone = peak_rss.clone();
    let done_clone = monitor_done.clone();
    let monitor_thread = thread::spawn(move || {
        while !done_clone.load(Ordering::Relaxed) {
            let current = get_current_rss_bytes();
            peak_clone.fetch_max(current, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(20));
        }
    });

    let grade = ColorGradeSettings {
        exposure: 0.0,
        contrast: 1.0,
        saturation: 1.0,
        temperature: 0.0,
        tint: 0.0,
        lut_idx: 0,
    };

    let req = RenderRequest {
        project: &project,
        output_path: &out_file,
        preset: &preset,
        grade,
    };

    let start_instant = Instant::now();
    let render_res = render_project_to_file(req, None, None);
    let render_duration = start_instant.elapsed();

    monitor_done.store(true, Ordering::Relaxed);
    let _ = monitor_thread.join();

    assert!(
        render_res.is_ok(),
        "4K render_project_to_file failed: {:?}",
        render_res
    );
    assert!(out_file.exists(), "Rendered 4K MP4 must exist");

    // 1. Peak RSS Verification: 4K memory must stay below 800 MB
    let peak_bytes = peak_rss.load(Ordering::Relaxed);
    let peak_mb = peak_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "4K Streaming Export: rendered 60 frames in {:.2?} (Peak RSS: {:.2} MB)",
        render_duration, peak_mb
    );
    if peak_bytes > 0 {
        assert!(
            peak_mb < 800.0,
            "4K export peak memory ({:.2} MB) exceeded 800 MB threshold!",
            peak_mb
        );
    }

    // 2. Container Probe Verification
    let probe = ExportJob::probe_file(&out_file).expect("ffprobe must succeed on 4K export");
    assert!(probe.has_video, "Exported file must contain video stream");
    assert_eq!(probe.width, Some(3840), "Exported width must be 3840");
    assert_eq!(probe.height, Some(2160), "Exported height must be 2160");
    let dur = probe.duration_seconds.unwrap_or(0.0);
    assert!(
        (1.8..=2.3).contains(&dur),
        "Duration ({:.2}s) must be approximately 2.0s",
        dur
    );

    // 3. Genuine Content Verification: Decode 4K frame at t=1.0s
    let mut decoder = rd_decode::FfmpegDecoder::open(&out_file).expect("Open rendered 4K decoder");
    let f1 = decoder
        .decode_frame_at(1_000_000)
        .expect("Decode 4K frame at 1.0s")
        .expect("4K frame at 1.0s present");
    assert_eq!(f1.width, 3840);
    assert_eq!(f1.height, 2160);
    let (mean, std_dev) = compute_image_metrics(f1.rgba_data.as_ref().unwrap());
    assert!(
        mean > 2.0,
        "4K frame luminance ({:.2}) must be non-zero",
        mean
    );
    assert!(
        std_dev > 2.0,
        "4K frame variance ({:.2}) must show genuine content",
        std_dev
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 3. Work-Area (Mark-In / Mark-Out) Range Export & Temporal Accuracy
// ---------------------------------------------------------------------------
#[test]
fn test_streaming_export_work_area_range_and_temporal_accuracy() {
    if !require_or_skip_ffmpeg("test_streaming_export_work_area_range_and_temporal_accuracy") {
        return;
    }

    let (video_path, audio_path) = locate_sample_media();
    if !video_path.exists() {
        eprintln!(
            "Sample video asset {:?} not found, skipping integration test",
            video_path
        );
        return;
    }

    // Build 6.0 second timeline project
    let mut project = Project::new("Work Area Range Export Test");
    let v_track = project.timeline.add_track(TrackKind::Video, "Video 1");
    let clip_v = Clip::new_video(video_path.to_str().unwrap(), 0, 0, 6_000_000);
    project
        .timeline
        .track_mut(v_track)
        .unwrap()
        .add_clip(clip_v);

    if audio_path.exists() {
        let a_track = project.timeline.add_track(TrackKind::Audio, "Audio 1");
        let clip_a = Clip::new_audio(audio_path.to_str().unwrap(), 0, 6_000_000);
        project
            .timeline
            .track_mut(a_track)
            .unwrap()
            .add_clip(clip_a);
    }

    // Set Work Area: Mark-In at 1.5s (1,500,000 µs), Mark-Out at 4.0s (4,000,000 µs)
    // Expected output duration: exactly 2.5 seconds (75 frames @ 30 fps)
    project.timeline.set_mark_in(1_500_000);
    project.timeline.set_mark_out(4_000_000);

    let preset = ExportPreset::youtube_1080p();
    let temp_dir = std::env::temp_dir().join(format!("reddart_work_area_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Create temp dir");
    let out_file = temp_dir.join("export_work_area_range.mp4");

    let req = RenderRequest {
        project: &project,
        output_path: &out_file,
        preset: &preset,
        grade: ColorGradeSettings::default(),
    };

    let render_res = render_project_to_file(req, None, None);
    assert!(
        render_res.is_ok(),
        "Work area render_project_to_file failed: {:?}",
        render_res
    );
    assert!(out_file.exists());

    let probe = ExportJob::probe_file(&out_file).expect("Probe work-area output");
    assert!(probe.has_video);
    assert_eq!(probe.width, Some(1920));
    assert_eq!(probe.height, Some(1080));

    let dur = probe.duration_seconds.unwrap_or(0.0);
    println!(
        "Work Area Export: expected 2.50s, measured duration: {:.2}s",
        dur
    );
    assert!(
        (2.3..=2.7).contains(&dur),
        "Work area duration ({:.2}s) must match the 2.5s mark-in/out range, NOT full 6.0s!",
        dur
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 4. Memory Invariance Over Time (O(1) Streaming Proof)
// ---------------------------------------------------------------------------
#[test]
fn test_streaming_export_memory_invariance_over_time() {
    if !require_or_skip_ffmpeg("test_streaming_export_memory_invariance_over_time") {
        return;
    }

    let (video_path, _) = locate_sample_media();
    if !video_path.exists() {
        eprintln!(
            "Sample video asset {:?} not found, skipping integration test",
            video_path
        );
        return;
    }

    // Build 6.0 second timeline (180 frames)
    let mut project = Project::new("Streaming Temporal Memory Invariance Test");
    let v_track = project.timeline.add_track(TrackKind::Video, "Video 1");
    let clip_v = Clip::new_video(video_path.to_str().unwrap(), 0, 0, 6_000_000);
    project
        .timeline
        .track_mut(v_track)
        .unwrap()
        .add_clip(clip_v);

    let preset = ExportPreset::youtube_1080p();
    let temp_dir = std::env::temp_dir().join(format!("reddart_mem_invar_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Create temp dir");
    let out_file = temp_dir.join("export_temporal_invariance.mp4");

    // Track memory samples at start, quarter-way, and finish
    let rss_samples = Arc::new(std::sync::Mutex::new(Vec::new()));
    let monitor_done = Arc::new(AtomicBool::new(false));

    let samples_clone = rss_samples.clone();
    let done_clone = monitor_done.clone();
    let monitor_thread = thread::spawn(move || {
        while !done_clone.load(Ordering::Relaxed) {
            let current = get_current_rss_bytes();
            if current > 0 {
                let mut guard = samples_clone.lock().unwrap();
                guard.push(current);
            }
            thread::sleep(Duration::from_millis(50));
        }
    });

    let req = RenderRequest {
        project: &project,
        output_path: &out_file,
        preset: &preset,
        grade: ColorGradeSettings::default(),
    };

    let res = render_project_to_file(req, None, None);
    monitor_done.store(true, Ordering::Relaxed);
    let _ = monitor_thread.join();

    assert!(res.is_ok(), "Temporal invariance render failed: {:?}", res);

    let samples = rss_samples.lock().unwrap().clone();
    if samples.len() >= 10 {
        // Compare midpoint memory with late-point memory
        let mid_idx = samples.len() / 2;
        let late_idx = (samples.len() * 4) / 5;
        let mid_mb = samples[mid_idx] as f64 / (1024.0 * 1024.0);
        let late_mb = samples[late_idx] as f64 / (1024.0 * 1024.0);
        let delta_mb = (late_mb - mid_mb).abs();

        println!(
            "Temporal Memory Invariance: mid={:.2} MB, late={:.2} MB, delta={:.2} MB (samples={})",
            mid_mb,
            late_mb,
            delta_mb,
            samples.len()
        );

        // Memory delta between mid and late stages must not balloon (> 60 MB variance)
        assert!(
            delta_mb < 60.0,
            "Memory growth between mid ({:.2} MB) and late ({:.2} MB) stage exceeded 60 MB tolerance!",
            mid_mb,
            late_mb
        );
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
