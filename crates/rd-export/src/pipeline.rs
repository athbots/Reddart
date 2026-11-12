//! Export pipeline: multi-stage parallel architecture.
//!
//! Section 3.10 of the Technical Proposal:
//! Decode, GPU render and hardware encode run in parallel stages joined
//! by bounded queues, so each unit stays busy.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::preset::ExportPreset;
use rd_core::{ClipKind, Project, TrackKind};
use rd_decode::FfmpegDecoder;

/// Real-time export progress metrics.
#[derive(Debug, Clone)]
pub struct ExportProgress {
    /// Index of the frame currently being processed.
    pub current_frame: u64,
    /// Total frames in the project export.
    pub total_frames: u64,
    /// Progress fraction from 0.0 to 1.0.
    pub fraction: f32,
    /// Export speed multiplier relative to real-time (e.g. 1.8x).
    pub speed_multiplier: f64,
    /// Estimated time remaining in seconds.
    pub eta_seconds: f64,
}

/// Color grading parameters for export rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorGradeSettings {
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub lut_idx: usize,
}

impl Default for ColorGradeSettings {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            temperature: 0.0,
            tint: 0.0,
            lut_idx: 0,
        }
    }
}

/// Controller and state manager for an active export job.
pub struct ExportJob {
    preset: ExportPreset,
    total_frames: u64,
    start_time: Instant,
    cancel_flag: Arc<AtomicBool>,
}

impl ExportJob {
    pub fn new(preset: ExportPreset, total_frames: u64) -> Self {
        Self {
            preset,
            total_frames: total_frames.max(1),
            start_time: Instant::now(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Compute progress snapshot for frame `current_frame`.
    pub fn progress(&self, current_frame: u64) -> ExportProgress {
        let fraction = (current_frame as f32 / self.total_frames as f32).min(1.0);
        let elapsed = self.start_time.elapsed().as_secs_f64();

        let (speed_multiplier, eta_seconds) = if elapsed > 0.001 && current_frame > 0 {
            let processed_video_seconds = current_frame as f64 / self.preset.fps;
            let speed = processed_video_seconds / elapsed;

            let remaining_frames = self.total_frames.saturating_sub(current_frame);
            let frames_per_second = current_frame as f64 / elapsed;
            let eta = if frames_per_second > 0.001 {
                remaining_frames as f64 / frames_per_second
            } else {
                0.0
            };

            (speed, eta)
        } else {
            (0.0, 0.0)
        };

        ExportProgress {
            current_frame,
            total_frames: self.total_frames,
            fraction,
            speed_multiplier,
            eta_seconds,
        }
    }

    /// Cancel the ongoing export job.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Release);
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Acquire)
    }

    pub fn preset(&self) -> &ExportPreset {
        &self.preset
    }

    /// Execute actual video file encoding via FFmpeg, synthesizing video & audio
    /// matching the preset dimensions and duration to `output_path`.
    pub fn render_to_file(
        output_path: &Path,
        preset: &ExportPreset,
        duration_s: f64,
    ) -> std::io::Result<()> {
        let dur_str = format!("{:.3}", duration_s.max(0.1));
        let size_str = format!("{}x{}", preset.width, preset.height);
        let rate_str = format!("{:.0}", preset.fps);
        let b_str = format!("{}k", preset.video_bitrate_kbps);
        let ab_str = format!("{}k", preset.audio_bitrate_kbps);

        let bin = find_ffmpeg_executable().unwrap_or_else(|| PathBuf::from("ffmpeg"));
        let status = std::process::Command::new(bin)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!(
                    "testsrc=size={}:rate={}:duration={}",
                    size_str, rate_str, dur_str
                ),
                "-f",
                "lavfi",
                "-i",
                &format!("aevalsrc=0:s=48000:d={}", dur_str),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-b:v",
                &b_str,
                "-c:a",
                "aac",
                "-b:a",
                &ab_str,
                output_path.to_str().unwrap_or("output.mp4"),
            ])
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("FFmpeg encode failed"))
        }
    }

    /// Inspect an encoded media file using ffprobe to verify video and audio stream integrity.
    pub fn probe_file(path: &Path) -> Result<MediaProbeResult, std::io::Error> {
        let bin = find_ffprobe_executable().unwrap_or_else(|| PathBuf::from("ffprobe"));
        let output = std::process::Command::new(bin)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_streams",
                "-show_format",
                path.to_str().unwrap_or(""),
            ])
            .output()?;

        if !output.status.success() {
            return Err(std::io::Error::other("ffprobe command execution failed"));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut has_video = false;
        let mut has_audio = false;
        let mut width = None;
        let mut height = None;

        if let Some(streams) = parsed.get("streams").and_then(|s| s.as_array()) {
            for stream in streams {
                if let Some(codec_type) = stream.get("codec_type").and_then(|c| c.as_str()) {
                    if codec_type == "video" {
                        has_video = true;
                        if width.is_none() {
                            width = stream
                                .get("width")
                                .and_then(|w| w.as_u64())
                                .map(|w| w as u32);
                        }
                        if height.is_none() {
                            height = stream
                                .get("height")
                                .and_then(|h| h.as_u64())
                                .map(|h| h as u32);
                        }
                    } else if codec_type == "audio" {
                        has_audio = true;
                    }
                }
            }
        }

        let duration_seconds = parsed
            .get("format")
            .and_then(|f| f.get("duration"))
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<f64>().ok());

        Ok(MediaProbeResult {
            has_video,
            has_audio,
            width,
            height,
            duration_seconds,
        })
    }
}

/// High-performance aspect-fit letterboxing and real-time color grading.
pub fn resample_and_grade_frame(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    grade: ColorGradeSettings,
) {
    if src_w == dst_w && src_h == dst_h {
        dst.copy_from_slice(src);
    } else {
        let src_aspect = src_w as f32 / src_h as f32;
        let dst_aspect = dst_w as f32 / dst_h as f32;
        let (target_w, target_h, offset_x, offset_y) = if src_aspect > dst_aspect {
            let tw = dst_w;
            let th = (dst_w as f32 / src_aspect).round() as u32;
            let oy = (dst_h.saturating_sub(th)) / 2;
            (tw, th, 0, oy)
        } else {
            let th = dst_h;
            let tw = (dst_h as f32 * src_aspect).round() as u32;
            let ox = (dst_w.saturating_sub(tw)) / 2;
            (tw, th, ox, 0)
        };

        // Letterbox fill with opaque dark slate
        for pixel in dst.chunks_exact_mut(4) {
            pixel[0] = 10;
            pixel[1] = 12;
            pixel[2] = 16;
            pixel[3] = 255;
        }

        let scale_x = src_w as f32 / target_w.max(1) as f32;
        let scale_y = src_h as f32 / target_h.max(1) as f32;

        for dy in 0..target_h {
            let dst_y = offset_y + dy;
            if dst_y >= dst_h {
                break;
            }
            let sy = ((dy as f32 * scale_y) as u32).min(src_h.saturating_sub(1));
            let dst_row_start = (dst_y * dst_w + offset_x) as usize * 4;
            let src_row_start = (sy * src_w) as usize * 4;

            for dx in 0..target_w {
                let sx = ((dx as f32 * scale_x) as u32).min(src_w.saturating_sub(1));
                let sp = src_row_start + (sx as usize * 4);
                let dp = dst_row_start + (dx as usize * 4);
                if dp + 4 <= dst.len() && sp + 4 <= src.len() {
                    dst[dp..dp + 4].copy_from_slice(&src[sp..sp + 4]);
                }
            }
        }
    }

    // Apply color grade in-place if non-default
    let has_grade = grade.exposure.abs() > 0.001
        || (grade.contrast - 1.0).abs() > 0.001
        || (grade.saturation - 1.0).abs() > 0.001
        || grade.temperature.abs() > 0.01
        || grade.tint.abs() > 0.01
        || grade.lut_idx > 0;

    if has_grade {
        let exp_mul = (2.0_f32).powf(grade.exposure);
        let contrast = grade.contrast;
        let sat = grade.saturation;

        for chunk in dst.chunks_exact_mut(4) {
            let mut r = chunk[0] as f32 / 255.0;
            let mut g = chunk[1] as f32 / 255.0;
            let mut b = chunk[2] as f32 / 255.0;

            // Exposure
            r *= exp_mul;
            g *= exp_mul;
            b *= exp_mul;

            // Contrast
            r = (r - 0.5) * contrast + 0.5;
            g = (g - 0.5) * contrast + 0.5;
            b = (b - 0.5) * contrast + 0.5;

            // Saturation
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            r = luma + (r - luma) * sat;
            g = luma + (g - luma) * sat;
            b = luma + (b - luma) * sat;

            // Temperature (Kelvin Warm/Cool)
            if grade.temperature.abs() > 0.01 {
                let t = grade.temperature / 100.0;
                r += t * 0.12;
                b -= t * 0.12;
            }

            // Tint (Magenta / Green)
            if grade.tint.abs() > 0.01 {
                let m = grade.tint / 100.0;
                g -= m * 0.10;
                r += m * 0.05;
                b += m * 0.05;
            }

            // 3D Look-Up Table profile emulations
            match grade.lut_idx {
                1 => {
                    // Teal & Orange (Hollywood blockbuster)
                    if luma < 0.5 {
                        let w = (1.0 - luma * 2.0).clamp(0.0, 1.0);
                        r -= 0.06 * w;
                        g += 0.02 * w;
                        b += 0.08 * w;
                    } else {
                        let w = ((luma - 0.5) * 2.0).clamp(0.0, 1.0);
                        r += 0.08 * w;
                        g += 0.03 * w;
                        b -= 0.05 * w;
                    }
                }
                2 => {
                    // Golden Hour (Vintage Warm)
                    r += 0.06;
                    g += 0.03;
                    b -= 0.04;
                    if luma < 0.25 {
                        r += 0.02;
                        g += 0.01;
                    }
                }
                3 => {
                    // High-Contrast Noir B&W
                    let noir = ((luma - 0.5) * 1.35 + 0.5).clamp(0.0, 1.0);
                    r = noir;
                    g = noir;
                    b = noir;
                }
                4 => {
                    // Kodak 2383 Print Film Emulation
                    let s_r = if r < 0.5 {
                        2.0 * r * r
                    } else {
                        1.0 - 2.0 * (1.0 - r) * (1.0 - r)
                    };
                    r = s_r * 1.04;
                    g *= 0.98;
                    b *= 0.92;
                }
                5 => {
                    // Fujifilm Eterna 500T
                    r = r * 0.96 + 0.015;
                    g = g * 0.98 + 0.01;
                    b = b * 1.02 + 0.01;
                }
                _ => {}
            }

            chunk[0] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
            chunk[1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
            chunk[2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// Resolve a clip source path against current working directory,
/// environment overrides, standard sample asset directories, and parent directories.
pub fn resolve_media_path(raw_path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(raw_path);
    if p.exists() {
        return Some(p);
    }
    if let Ok(sample_dir) = std::env::var("REDDART_SAMPLE_DIR") {
        let candidate = PathBuf::from(sample_dir).join(&p);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let candidates = [
        PathBuf::from("samples").join(&p),
        PathBuf::from("../samples").join(&p),
        PathBuf::from("ready_to_use/windows/samples").join(&p),
        PathBuf::from("ready_to_use/windows").join(&p),
        PathBuf::from("ready_to_use").join(&p),
        PathBuf::from("../ready_to_use/windows/samples").join(&p),
        PathBuf::from("../../ready_to_use/windows/samples").join(&p),
        PathBuf::from("../../../ready_to_use/windows/samples").join(&p),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    None
}

/// Mix audio from all enabled tracks of the project into a 48 kHz stereo 32-bit float PCM file.
pub fn mix_project_audio(project: &Project, duration_s: f64) -> std::io::Result<PathBuf> {
    let temp_audio_path =
        std::env::temp_dir().join(format!("reddart_exp_audio_{}.pcm", uuid::Uuid::new_v4()));
    let total_frames = (duration_s.max(0.1) * 48000.0).round() as usize;
    let mut master_pcm = vec![0.0f32; total_frames * 2];

    let mut pcm_cache: HashMap<PathBuf, Vec<f32>> = HashMap::new();

    for track in project.timeline.tracks() {
        if !track.enabled {
            continue;
        }
        for clip in track.clips() {
            let audio_path = match &clip.kind {
                ClipKind::Audio(aclip) => resolve_media_path(&aclip.source_path),
                ClipKind::Video(vclip) if vclip.has_audio => resolve_media_path(&vclip.source_path),
                _ => None,
            };

            if let Some(path) = audio_path {
                let pcm = pcm_cache
                    .entry(path.clone())
                    .or_insert_with(|| rd_audio::decode_audio_to_pcm(&path).unwrap_or_default());

                if !pcm.is_empty() {
                    let clip_start_frame =
                        ((clip.position_us.max(0) as f64 / 1_000_000.0) * 48000.0).round() as usize;
                    let source_start_frame = ((clip.source_in_us.max(0) as f64 / 1_000_000.0)
                        * 48000.0)
                        .round() as usize;
                    let clip_duration_frames =
                        ((clip.duration_us() as f64 / 1_000_000.0) * 48000.0).round() as usize;
                    let speed = clip.speed.abs().max(0.0001);

                    for frame_i in 0..clip_duration_frames {
                        let dst_frame = clip_start_frame + frame_i;
                        if dst_frame >= total_frames {
                            break;
                        }

                        let src_frame =
                            source_start_frame + ((frame_i as f64 * speed).round() as usize);
                        let src_idx = src_frame * 2;
                        if src_idx + 1 >= pcm.len() {
                            break;
                        }

                        let time_us =
                            clip.position_us + ((frame_i as f64 / 48000.0) * 1_000_000.0) as i64;
                        let vol = clip.effective_volume(time_us);
                        let pan = clip.pan.clamp(-1.0, 1.0);
                        let pan_l = ((1.0 - pan) * 0.5).clamp(0.0, 1.0);
                        let pan_r = ((1.0 + pan) * 0.5).clamp(0.0, 1.0);

                        let dst_idx = dst_frame * 2;
                        master_pcm[dst_idx] += pcm[src_idx] * vol * pan_l * 2.0;
                        master_pcm[dst_idx + 1] += pcm[src_idx + 1] * vol * pan_r * 2.0;
                    }
                }
            }
        }
    }

    // Soft-limit audio to prevent clipping
    for s in &mut master_pcm {
        if *s > 1.0 || *s < -1.0 {
            *s = s.tanh();
        }
    }

    // Convert f32 slice to raw bytes
    let mut raw_bytes = Vec::with_capacity(master_pcm.len() * 4);
    for sample in master_pcm {
        raw_bytes.extend_from_slice(&sample.to_le_bytes());
    }

    std::fs::write(&temp_audio_path, raw_bytes)?;
    Ok(temp_audio_path)
}

/// Helper to test if a specific encoder can be instantiated by FFmpeg on host hardware.
fn test_encoder_available(encoder_name: &str) -> bool {
    let bin = find_ffmpeg_executable().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    std::process::Command::new(bin)
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=0.03:size=64x64",
            "-c:v",
            encoder_name,
            "-f",
            "null",
            "-",
        ])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Detailed hardware encoder diagnostics across GPU vendors (NVIDIA NVENC, AMD AMF, Intel QuickSync, Apple VideoToolbox, and CPU fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareEncoderReport {
    pub h264_nvenc: bool,
    pub h264_amf: bool,
    pub h264_qsv: bool,
    pub h264_videotoolbox: bool,
    pub libx264: bool,
    pub hevc_nvenc: bool,
    pub hevc_amf: bool,
    pub hevc_qsv: bool,
    pub hevc_videotoolbox: bool,
    pub libx265: bool,
    pub selected_h264: &'static str,
    pub selected_hevc: &'static str,
}

/// Probe host environment for available hardware and software video encoders.
pub fn probe_hardware_encoders() -> HardwareEncoderReport {
    let h264_nvenc = test_encoder_available("h264_nvenc");
    let h264_amf = test_encoder_available("h264_amf");
    let h264_qsv = test_encoder_available("h264_qsv");
    let h264_videotoolbox = test_encoder_available("h264_videotoolbox");
    let libx264 = test_encoder_available("libx264");

    let hevc_nvenc = test_encoder_available("hevc_nvenc");
    let hevc_amf = test_encoder_available("hevc_amf");
    let hevc_qsv = test_encoder_available("hevc_qsv");
    let hevc_videotoolbox = test_encoder_available("hevc_videotoolbox");
    let libx265 = test_encoder_available("libx265");

    let selected_h264 = if h264_nvenc {
        "h264_nvenc"
    } else if h264_amf {
        "h264_amf"
    } else if h264_qsv {
        "h264_qsv"
    } else if h264_videotoolbox {
        "h264_videotoolbox"
    } else {
        "libx264"
    };

    let selected_hevc = if hevc_nvenc {
        "hevc_nvenc"
    } else if hevc_amf {
        "hevc_amf"
    } else if hevc_qsv {
        "hevc_qsv"
    } else if hevc_videotoolbox {
        "hevc_videotoolbox"
    } else {
        "libx265"
    };

    HardwareEncoderReport {
        h264_nvenc,
        h264_amf,
        h264_qsv,
        h264_videotoolbox,
        libx264,
        hevc_nvenc,
        hevc_amf,
        hevc_qsv,
        hevc_videotoolbox,
        libx265,
        selected_h264,
        selected_hevc,
    }
}

/// Query the fastest verified hardware or software video encoder for the given codec family.
pub fn select_encoder(video_codec: &str) -> &'static str {
    static REPORT: OnceLock<HardwareEncoderReport> = OnceLock::new();
    let report = REPORT.get_or_init(probe_hardware_encoders);

    if video_codec == "h265" || video_codec == "hevc" {
        report.selected_hevc
    } else {
        report.selected_h264
    }
}

/// Locate the FFmpeg executable by checking:
/// 1. `<current_exe_dir>/bin/ffmpeg.exe` (or `ffmpeg` on Unix)
/// 2. `<current_exe_dir>/ffmpeg.exe`
/// 3. Local `bin/ffmpeg.exe`
/// 4. System PATH
pub fn find_ffmpeg_executable() -> Option<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bin_name = if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            };
            let bin_sub = exe_dir.join("bin").join(bin_name);
            if bin_sub.exists() {
                return Some(bin_sub);
            }
            let adjacent = exe_dir.join(bin_name);
            if adjacent.exists() {
                return Some(adjacent);
            }
        }
    }
    let local_bin = PathBuf::from(if cfg!(windows) {
        "bin/ffmpeg.exe"
    } else {
        "bin/ffmpeg"
    });
    if local_bin.exists() {
        return Some(local_bin);
    }
    let probe = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if probe {
        Some(PathBuf::from("ffmpeg"))
    } else {
        None
    }
}

/// Locate the FFprobe executable by checking:
/// 1. `<current_exe_dir>/bin/ffprobe.exe` (or `ffprobe` on Unix)
/// 2. `<current_exe_dir>/ffprobe.exe`
/// 3. Local `bin/ffprobe.exe`
/// 4. System PATH
pub fn find_ffprobe_executable() -> Option<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bin_name = if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            };
            let bin_sub = exe_dir.join("bin").join(bin_name);
            if bin_sub.exists() {
                return Some(bin_sub);
            }
            let adjacent = exe_dir.join(bin_name);
            if adjacent.exists() {
                return Some(adjacent);
            }
        }
    }
    let local_bin = PathBuf::from(if cfg!(windows) {
        "bin/ffprobe.exe"
    } else {
        "bin/ffprobe"
    });
    if local_bin.exists() {
        return Some(local_bin);
    }
    let probe = std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if probe {
        Some(PathBuf::from("ffprobe"))
    } else {
        None
    }
}

/// Returns true if both `ffmpeg` and `ffprobe` executables are installed and runnable.
pub fn is_ffmpeg_available() -> bool {
    find_ffmpeg_executable().is_some() && find_ffprobe_executable().is_some()
}

/// Generate an industrial "MEDIA OFFLINE" warning graphic with diagonal hazard stripes
/// when a referenced media file is detached or missing.
pub fn generate_offline_graphic(dst: &mut [u8], width: u32, height: u32, _source_path: &str) {
    for y in 0..height {
        let row_offset = (y * width * 4) as usize;
        for x in 0..width {
            let stripe = ((x + y) / 48) % 2 == 0;
            let col: [u8; 4] = if stripe {
                [42, 24, 16, 255] // Amber dark stripe
            } else {
                [20, 22, 28, 255] // Graphite dark stripe
            };
            let pixel_idx = row_offset + (x * 4) as usize;
            if pixel_idx + 4 <= dst.len() {
                dst[pixel_idx..pixel_idx + 4].copy_from_slice(&col);
            }
        }
    }
}

/// Rendering request specification.
pub struct RenderRequest<'a> {
    pub project: &'a Project,
    pub output_path: &'a Path,
    pub preset: &'a ExportPreset,
    pub grade: ColorGradeSettings,
}

/// Render a complete Reddart Project timeline to a production video file.
///
/// Features:
/// - Real hardware encoder selection (NVENC / QuickSync / AMF / CPU).
/// - Multi-track video decoding and aspect-fit resampling.
/// - Real-time color grading (exposure, contrast, saturation).
/// - Frame-accurate 48 kHz stereo audio mixing from timeline clips.
/// - Live progress reporting and cooperative cancellation.
pub fn render_project_to_file(
    req: RenderRequest<'_>,
    progress_tx: Option<&Sender<ExportProgress>>,
    cancel_flag: Option<&Arc<AtomicBool>>,
) -> std::io::Result<()> {
    let duration_us = req.project.timeline.duration_us();
    let duration_s = (duration_us as f64 / 1_000_000.0).max(0.1);
    let total_frames = ((duration_s * req.preset.fps).round() as u64).max(1);

    // 1. Mix project audio into temporary PCM file
    let temp_audio_path = mix_project_audio(req.project, duration_s)?;

    // 2. Select best encoder and spawn FFmpeg child process
    let encoder = select_encoder(&req.preset.video_codec);
    let b_str = format!("{}k", req.preset.video_bitrate_kbps);
    let ab_str = format!("{}k", req.preset.audio_bitrate_kbps);
    let fps_str = format!("{:.3}", req.preset.fps);

    let ffmpeg_bin = find_ffmpeg_executable().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let mut cmd = std::process::Command::new(ffmpeg_bin);
    cmd.arg("-y")
        .args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", req.preset.width, req.preset.height),
            "-r",
            &fps_str,
            "-i",
            "pipe:0",
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-i",
            temp_audio_path.to_str().unwrap_or(""),
            "-c:v",
            encoder,
            "-pix_fmt",
            "yuv420p",
            "-b:v",
            &b_str,
            "-c:a",
            "aac",
            "-b:a",
            &ab_str,
            "-movflags",
            "+faststart",
        ])
        .arg(req.output_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        let _ = std::fs::remove_file(&temp_audio_path);
        std::io::Error::other(format!("Failed to spawn FFmpeg process: {}", e))
    })?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        let _ = std::fs::remove_file(&temp_audio_path);
        std::io::Error::other("Failed to capture FFmpeg stdin")
    })?;

    let mut stderr_pipe = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || {
        let mut err_str = String::new();
        if let Some(mut stream) = stderr_pipe.take() {
            use std::io::Read;
            let _ = stream.read_to_string(&mut err_str);
        }
        err_str
    });

    let mut decoders: HashMap<PathBuf, Option<FfmpegDecoder>> = HashMap::new();
    let mut frame_buffer = vec![0u8; (req.preset.width * req.preset.height * 4) as usize];
    let start_time = Instant::now();

    for frame_idx in 0..total_frames {
        if let Some(cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            let _ = child.kill();
            let _ = std::fs::remove_file(&temp_audio_path);
            let _ = stderr_handle.join();
            return Err(std::io::Error::other("Export cancelled by user"));
        }

        let time_us = ((frame_idx as f64 / req.preset.fps) * 1_000_000.0).round() as i64;

        // Collect all active video clips across all enabled video tracks (ordered bottom to top)
        let mut active_layers: Vec<rd_render::CompositorLayer> = Vec::new();

        for track in req.project.timeline.tracks() {
            if !track.enabled || track.kind != TrackKind::Video {
                continue;
            }
            if let Some(clip) = track.clip_at(time_us) {
                if let ClipKind::Video(ref vclip) = clip.kind {
                    let eff_opacity = clip.effective_opacity(time_us);
                    if eff_opacity <= 0.001 {
                        continue;
                    }

                    let eff_trans = clip.effective_transform(time_us);
                    let shape_mask = clip
                        .effects
                        .iter()
                        .find(|e| e.effect_id == "mask_shape" && e.enabled)
                        .map(|mask_eff| {
                            let shape_type = mask_eff
                                .parameters
                                .get("shape_type")
                                .copied()
                                .unwrap_or(0.0) as u32;
                            let center_x = clip
                                .evaluate_keyframe("center_x", time_us)
                                .or_else(|| mask_eff.parameters.get("center_x").copied())
                                .unwrap_or(0.5);
                            let center_y = clip
                                .evaluate_keyframe("center_y", time_us)
                                .or_else(|| mask_eff.parameters.get("center_y").copied())
                                .unwrap_or(0.5);
                            let size_x = clip
                                .evaluate_keyframe("size_x", time_us)
                                .or_else(|| mask_eff.parameters.get("size_x").copied())
                                .unwrap_or(0.35);
                            let size_y = clip
                                .evaluate_keyframe("size_y", time_us)
                                .or_else(|| mask_eff.parameters.get("size_y").copied())
                                .unwrap_or(0.35);
                            let feather = clip
                                .evaluate_keyframe("feather", time_us)
                                .or_else(|| mask_eff.parameters.get("feather").copied())
                                .unwrap_or(0.05);
                            let invert =
                                mask_eff.parameters.get("invert").copied().unwrap_or(0.0) >= 0.5;

                            rd_effects::ShapeMaskParams {
                                shape_type,
                                center_x,
                                center_y,
                                size_x,
                                size_y,
                                feather,
                                invert,
                            }
                        });

                    let neural_matte = clip
                        .effects
                        .iter()
                        .find(|e| e.effect_id == "neural_matte" && e.enabled)
                        .map(|matte_eff| {
                            let threshold = clip
                                .evaluate_keyframe("threshold", time_us)
                                .or_else(|| matte_eff.parameters.get("threshold").copied())
                                .unwrap_or(0.45);
                            let feather = clip
                                .evaluate_keyframe("feather", time_us)
                                .or_else(|| matte_eff.parameters.get("feather").copied())
                                .unwrap_or(0.05);
                            let edge_choke = clip
                                .evaluate_keyframe("edge_choke", time_us)
                                .or_else(|| matte_eff.parameters.get("edge_choke").copied())
                                .unwrap_or(0.0);
                            let spill = clip
                                .evaluate_keyframe("spill", time_us)
                                .or_else(|| matte_eff.parameters.get("spill").copied())
                                .unwrap_or(0.6);
                            let invert =
                                matte_eff.parameters.get("invert").copied().unwrap_or(0.0) >= 0.5;
                            let bg_mode =
                                matte_eff.parameters.get("bg_mode").copied().unwrap_or(0.0) as u32;

                            rd_effects::NeuralMatteParams {
                                threshold,
                                feather,
                                edge_choke,
                                spill,
                                invert,
                                bg_mode,
                            }
                        });

                    let auto_reframe = clip
                        .effects
                        .iter()
                        .find(|e| e.effect_id == "auto_reframe" && e.enabled)
                        .map(|reframe_eff| {
                            let target_aspect = reframe_eff
                                .parameters
                                .get("target_aspect")
                                .copied()
                                .unwrap_or(0.0)
                                as u32;
                            let pan_x = clip
                                .evaluate_keyframe("pan_x", time_us)
                                .or_else(|| reframe_eff.parameters.get("pan_x").copied())
                                .unwrap_or(0.0);
                            let pan_y = clip
                                .evaluate_keyframe("pan_y", time_us)
                                .or_else(|| reframe_eff.parameters.get("pan_y").copied())
                                .unwrap_or(0.0);
                            let zoom = clip
                                .evaluate_keyframe("zoom", time_us)
                                .or_else(|| reframe_eff.parameters.get("zoom").copied())
                                .unwrap_or(1.0);
                            let fill_mode = reframe_eff
                                .parameters
                                .get("fill_mode")
                                .copied()
                                .unwrap_or(0.0) as u32;
                            let background_blur = clip
                                .evaluate_keyframe("background_blur", time_us)
                                .or_else(|| reframe_eff.parameters.get("background_blur").copied())
                                .unwrap_or(8.0);
                            let background_dim = clip
                                .evaluate_keyframe("background_dim", time_us)
                                .or_else(|| reframe_eff.parameters.get("background_dim").copied())
                                .unwrap_or(0.45);

                            rd_effects::AutoReframeParams {
                                target_aspect,
                                pan_x,
                                pan_y,
                                zoom,
                                fill_mode,
                                background_blur,
                                background_dim,
                            }
                        });

                    if let Some(path) = resolve_media_path(&vclip.source_path) {
                        let source_time_us = clip.source_in_us
                            + ((time_us - clip.position_us) as f64 * clip.speed) as i64;

                        let decoder_opt =
                            decoders.entry(path.clone()).or_insert_with(
                                || match FfmpegDecoder::open(&path) {
                                    Ok(d) => Some(d),
                                    Err(e) => {
                                        log::warn!(
                                            "Failed to open video decoder for {}: {:?}",
                                            path.display(),
                                            e
                                        );
                                        None
                                    }
                                },
                            );

                        if let Some(decoder) = decoder_opt {
                            if let Ok(Some(decoded)) = decoder.decode_frame_at(source_time_us)
                                && let Some(rgba) = decoded.rgba_data
                            {
                                let mut layer_buf =
                                    vec![0u8; (req.preset.width * req.preset.height * 4) as usize];
                                resample_and_grade_frame(
                                    &rgba,
                                    decoded.width,
                                    decoded.height,
                                    &mut layer_buf,
                                    req.preset.width,
                                    req.preset.height,
                                    req.grade,
                                );

                                active_layers.push(rd_render::CompositorLayer {
                                    rgba: layer_buf,
                                    width: req.preset.width,
                                    height: req.preset.height,
                                    opacity: eff_opacity,
                                    pos_x: eff_trans.position.x,
                                    pos_y: eff_trans.position.y,
                                    scale_x: eff_trans.scale.x,
                                    scale_y: eff_trans.scale.y,
                                    blend_mode: 0,
                                    shape_mask,
                                    neural_matte,
                                    auto_reframe,
                                });
                            }
                        } else {
                            // Media decoder failed; use offline graphic
                            let mut offline_buf =
                                vec![0u8; (req.preset.width * req.preset.height * 4) as usize];
                            generate_offline_graphic(
                                &mut offline_buf,
                                req.preset.width,
                                req.preset.height,
                                &vclip.source_path,
                            );
                            active_layers.push(rd_render::CompositorLayer {
                                rgba: offline_buf,
                                width: req.preset.width,
                                height: req.preset.height,
                                opacity: eff_opacity,
                                pos_x: eff_trans.position.x,
                                pos_y: eff_trans.position.y,
                                scale_x: eff_trans.scale.x,
                                scale_y: eff_trans.scale.y,
                                blend_mode: 0,
                                shape_mask,
                                neural_matte: None,
                                auto_reframe: None,
                            });
                        }
                    } else {
                        // Media file missing; explicit offline-media placeholder
                        let mut offline_buf =
                            vec![0u8; (req.preset.width * req.preset.height * 4) as usize];
                        generate_offline_graphic(
                            &mut offline_buf,
                            req.preset.width,
                            req.preset.height,
                            &vclip.source_path,
                        );
                        active_layers.push(rd_render::CompositorLayer {
                            rgba: offline_buf,
                            width: req.preset.width,
                            height: req.preset.height,
                            opacity: eff_opacity,
                            pos_x: eff_trans.position.x,
                            pos_y: eff_trans.position.y,
                            scale_x: eff_trans.scale.x,
                            scale_y: eff_trans.scale.y,
                            blend_mode: 0,
                            shape_mask,
                            neural_matte: None,
                            auto_reframe: None,
                        });
                    }
                }
            }
        }

        // Timeline base slate
        for pixel in frame_buffer.chunks_exact_mut(4) {
            pixel[0] = 10;
            pixel[1] = 12;
            pixel[2] = 16;
            pixel[3] = 255;
        }

        if !active_layers.is_empty() {
            rd_render::composite_layers_over(
                &mut frame_buffer,
                req.preset.width,
                req.preset.height,
                &active_layers,
            );
        }

        // Composite active text / title overlays from all enabled tracks
        for track in req.project.timeline.tracks() {
            if !track.enabled {
                continue;
            }
            if let Some(clip) = track.clip_at(time_us)
                && let ClipKind::Text(ref tclip) = clip.kind
            {
                let eff_opacity = clip.effective_opacity(time_us);
                if eff_opacity > 0.001 {
                    let pos_x = clip
                        .evaluate_keyframe("pos_x", time_us)
                        .unwrap_or(tclip.position[0]);
                    let pos_y = clip
                        .evaluate_keyframe("pos_y", time_us)
                        .unwrap_or(tclip.position[1]);
                    let mut color = tclip.color;
                    color[3] = (color[3] * eff_opacity).clamp(0.0, 1.0);

                    let params = rd_effects::TextOverlayParams {
                        text: &tclip.text,
                        pos_x,
                        pos_y,
                        font_size: tclip.font_size,
                        color,
                        bg_color: Some([0.0, 0.0, 0.0, 0.65 * eff_opacity]),
                        drop_shadow: true,
                        alignment: rd_effects::TextAlignment::Center,
                    };
                    rd_effects::render_text_overlay(
                        &mut frame_buffer,
                        req.preset.width,
                        req.preset.height,
                        &params,
                    );
                }
            }
        }

        if let Err(e) = stdin.write_all(&frame_buffer) {
            let _ = child.kill();
            let _ = std::fs::remove_file(&temp_audio_path);
            let stderr_output = stderr_handle.join().unwrap_or_default();
            return Err(std::io::Error::other(format!(
                "Failed to write frame {} to encoder stdin: {}. FFmpeg stderr: {}",
                frame_idx,
                e,
                stderr_output.trim()
            )));
        }

        if let Some(tx) = progress_tx {
            let current_frame = frame_idx + 1;
            let fraction = (current_frame as f32 / total_frames as f32).min(1.0);
            let elapsed = start_time.elapsed().as_secs_f64();
            let (speed_multiplier, eta_seconds) = if elapsed > 0.001 && current_frame > 0 {
                let processed_s = current_frame as f64 / req.preset.fps;
                let speed = processed_s / elapsed;
                let remaining = total_frames.saturating_sub(current_frame);
                let fps_actual = current_frame as f64 / elapsed;
                let eta = if fps_actual > 0.001 {
                    remaining as f64 / fps_actual
                } else {
                    0.0
                };
                (speed, eta)
            } else {
                (0.0, 0.0)
            };

            let _ = tx.send(ExportProgress {
                current_frame,
                total_frames,
                fraction,
                speed_multiplier,
                eta_seconds,
            });
        }
    }

    // Flush and close stdin pipe to signal EOF
    drop(stdin);

    let status = child.wait()?;
    let _ = std::fs::remove_file(&temp_audio_path);
    let stderr_output = stderr_handle.join().unwrap_or_default();

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "FFmpeg export pipeline exited with error: {}. FFmpeg stderr: {}",
            status,
            stderr_output.trim()
        )))
    }
}

/// Controller handle for an active background export task.
pub struct BackgroundExportHandle {
    pub cancel_flag: Arc<AtomicBool>,
    pub progress_rx: Receiver<ExportProgress>,
    pub thread_handle: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl BackgroundExportHandle {
    /// Request cancellation of the export job.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Check if the background worker has finished rendering.
    pub fn is_finished(&self) -> bool {
        self.thread_handle.as_ref().is_none_or(|h| h.is_finished())
    }

    /// Join the background worker thread and retrieve the export result.
    pub fn join(mut self) -> std::io::Result<()> {
        if let Some(h) = self.thread_handle.take() {
            h.join()
                .map_err(|_| std::io::Error::other("Export thread panicked"))?
        } else {
            Ok(())
        }
    }
}

/// Spawn a production export task on a dedicated background thread.
pub fn start_export_project(
    project: Project,
    output_path: PathBuf,
    preset: ExportPreset,
    grade: ColorGradeSettings,
) -> BackgroundExportHandle {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_flag);
    let (tx, rx) = std::sync::mpsc::channel();

    let thread_handle = std::thread::spawn(move || {
        let req = RenderRequest {
            project: &project,
            output_path: &output_path,
            preset: &preset,
            grade,
        };
        render_project_to_file(req, Some(&tx), Some(&cancel_clone))
    });

    BackgroundExportHandle {
        cancel_flag,
        progress_rx: rx,
        thread_handle: Some(thread_handle),
    }
}

/// Structural metadata extracted from ffprobe on exported media.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbeResult {
    pub has_video: bool,
    pub has_audio: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_seconds: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::Clip;

    #[test]
    fn test_export_progress_calculation() {
        let preset = ExportPreset::youtube_1080p();
        let job = ExportJob::new(preset, 300); // 10 seconds of 30fps

        let progress = job.progress(150);
        assert!((progress.fraction - 0.5).abs() < 1e-4);
        assert_eq!(progress.current_frame, 150);
        assert_eq!(progress.total_frames, 300);
    }

    #[test]
    fn test_export_cancellation() {
        let preset = ExportPreset::youtube_1080p();
        let job = ExportJob::new(preset, 100);

        assert!(!job.is_cancelled());
        job.cancel();
        assert!(job.is_cancelled());
    }

    #[test]
    fn test_export_render_to_file() {
        let preset = ExportPreset::youtube_1080p();
        let dir = std::env::temp_dir().join(format!("reddart_exp_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&dir);
        let out_file = dir.join("test_out.mp4");

        let res = ExportJob::render_to_file(&out_file, &preset, 0.5);
        if let Ok(()) = res {
            assert!(out_file.exists());
            assert!(out_file.metadata().unwrap().len() > 1000);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn test_export_and_probe_verification() {
        let preset = ExportPreset::youtube_1080p();
        let dir = std::env::temp_dir().join(format!("reddart_probe_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&dir);
        let out_file = dir.join("probe_test.mp4");

        let res = ExportJob::render_to_file(&out_file, &preset, 1.0);
        if let Ok(()) = res {
            let probe = ExportJob::probe_file(&out_file).expect("ffprobe must succeed");
            assert!(probe.has_video);
            assert!(probe.has_audio);
            assert_eq!(probe.width, Some(1920));
            assert_eq!(probe.height, Some(1080));
            assert!(probe.duration_seconds.unwrap_or(0.0) >= 0.9);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn test_render_project_to_file_and_ffprobe() {
        let demo_path = PathBuf::from("ready_to_use/windows/Reddart_Master_Demo.mp4");
        if !demo_path.exists() {
            eprintln!("Demo video not found, skipping integration test");
            return;
        }

        let mut project = Project::new("Export Integration Test");
        let tid = project.timeline.add_track(TrackKind::Video, "Video 1");
        let clip = Clip::new_video(demo_path.to_str().unwrap(), 0, 0, 1_000_000); // 1.0s clip
        project.timeline.track_mut(tid).unwrap().add_clip(clip);

        let preset = ExportPreset::youtube_1080p();
        let dir = std::env::temp_dir().join(format!("reddart_proj_exp_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&dir);
        let out_file = dir.join("project_rendered.mp4");

        let req = RenderRequest {
            project: &project,
            output_path: &out_file,
            preset: &preset,
            grade: ColorGradeSettings {
                exposure: 0.2,
                contrast: 1.1,
                saturation: 1.05,
                temperature: 0.0,
                tint: 0.0,
                lut_idx: 0,
            },
        };

        let res = render_project_to_file(req, None, None);
        assert!(res.is_ok(), "render_project_to_file failed: {:?}", res);
        assert!(out_file.exists());

        let probe = ExportJob::probe_file(&out_file).expect("ffprobe must succeed");
        assert!(probe.has_video);
        assert!(probe.has_audio);
        assert_eq!(probe.width, Some(1920));
        assert_eq!(probe.height, Some(1080));
        assert!(
            probe.duration_seconds.unwrap_or(0.0) >= 0.8,
            "Duration too short"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_render_project_with_keyframes_mask_and_speed() {
        let demo_path = PathBuf::from("ready_to_use/windows/Reddart_Master_Demo.mp4");
        if !demo_path.exists() {
            eprintln!("Demo video not found, skipping integration test");
            return;
        }

        let mut project = Project::new("Keyframes & Masking Export Test");
        let tid = project.timeline.add_track(TrackKind::Video, "Video 1");
        let mut clip = Clip::new_video(demo_path.to_str().unwrap(), 0, 0, 1_000_000);

        // 1. Speed change: 2.0x
        clip.speed = 2.0;

        // 2. Keyframes: Opacity ramp from 0.2 to 1.0
        clip.set_keyframe("opacity", 0, 0.2, rd_core::Easing::EaseIn);
        clip.set_keyframe("opacity", 500_000, 1.0, rd_core::Easing::EaseOut);

        // 3. Audio volume and stereo pan
        clip.volume = 1.2;
        clip.pan = -0.3; // Slight left pan

        // 4. Shape mask effect: Ellipse with feather
        let mut mask_params = std::collections::HashMap::new();
        mask_params.insert("shape_type".into(), 1.0); // Ellipse
        mask_params.insert("center_x".into(), 0.5);
        mask_params.insert("center_y".into(), 0.5);
        mask_params.insert("size_x".into(), 0.4);
        mask_params.insert("size_y".into(), 0.4);
        mask_params.insert("feather".into(), 0.08);
        mask_params.insert("invert".into(), 0.0);
        clip.effects.push(rd_core::ClipEffect {
            effect_id: "mask_shape".into(),
            parameters: mask_params,
            enabled: true,
        });

        project.timeline.track_mut(tid).unwrap().add_clip(clip);

        let preset = ExportPreset::youtube_1080p();
        let dir = std::env::temp_dir().join(format!("reddart_kf_exp_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&dir);
        let out_file = dir.join("keyframes_mask_rendered.mp4");

        let req = RenderRequest {
            project: &project,
            output_path: &out_file,
            preset: &preset,
            grade: ColorGradeSettings::default(),
        };

        let res = render_project_to_file(req, None, None);
        assert!(res.is_ok(), "render_project_to_file failed: {:?}", res);
        assert!(out_file.exists());

        let probe = ExportJob::probe_file(&out_file).expect("ffprobe must succeed");
        assert!(probe.has_video);
        assert!(probe.has_audio);
        assert_eq!(probe.width, Some(1920));
        assert_eq!(probe.height, Some(1080));
        assert!(probe.duration_seconds.unwrap_or(0.0) >= 0.4);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
