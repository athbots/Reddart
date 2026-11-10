//! Application: main event loop, egui integration, and window management.
//!
//! Sets up the winit window, wgpu rendering, and egui UI.
//! The editor UI features an interactive timeline, media import,
//! inspector properties, transport controls, and playhead scrubbing.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

use crate::gpu;
use rd_audio::{AudioClock, AudioEngine, MeteringState, decode_audio_to_pcm};
use rd_core::{Clip, ClipId, Project, TrackId, TrackKind};
use rd_decode::FfmpegDecoder;

/// Studio editing workspace mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Edit,
    Color,
    Audio,
    Titles,
    Export,
}

/// Tab displayed in the left utility panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftPanelTab {
    #[default]
    MediaBin,
    Templates,
}

/// Active timeline editing tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditTool {
    #[default]
    Select, // [V] Selection & Move
    Razor,      // [C] Razor Split
    RippleTrim, // [B] Ripple Trim
    Roll,       // [N] Roll Edit
    Slip,       // [Y] Slip Edit
    Slide,      // [U] Slide Edit
}

/// Convert timestamp in microseconds to standard SMPTE timecode (HH:MM:SS:FF).
pub fn format_smpte(timestamp_us: i64, fps: f64) -> String {
    let fps = fps.max(1.0);
    let total_frames = ((timestamp_us.max(0) as f64 / 1_000_000.0) * fps).round() as i64;
    let frames_per_sec = fps.round() as i64;
    let frames = total_frames % frames_per_sec;
    let total_seconds = total_frames / frames_per_sec;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;
    format!("{:02}:{:02}:{:02}:{:02}", hours, minutes, seconds, frames)
}

/// Parameters for real-time video color grading.
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorGradeSettings {
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
}

/// Editor state holding user project data, selection, and playback state.
pub struct EditorData {
    pub workspace_mode: WorkspaceMode,
    pub project: Project,
    pub history: rd_core::CommandHistory,
    pub is_playing: bool,
    pub shuttle_speed: f32,
    pub last_frame_time: Instant,
    pub zoom_px_per_sec: f32,
    pub selected_clip: Option<ClipId>,
    pub selected_track: Option<TrackId>,
    pub media_files: Vec<PathBuf>,
    pub show_export_modal: bool,
    pub export_preset_idx: usize,
    pub export_destination: Option<PathBuf>,
    pub active_export: Option<rd_export::BackgroundExportHandle>,
    pub export_progress: Option<rd_export::ExportProgress>,
    pub export_completed: bool,
    pub last_save_path: Option<PathBuf>,
    pub last_autosave_time: Instant,
    pub missing_media: Vec<PathBuf>,
    pub status_message: Option<(String, Instant)>,
    pub snapping_enabled: bool,
    pub master_volume: f32,
    pub color_exposure: f32,
    pub color_contrast: f32,
    pub color_saturation: f32,
    pub color_temperature: f32,
    pub color_tint: f32,
    pub color_lut_idx: usize,
    pub audio_eq_low: f32,
    pub audio_eq_mid: f32,
    pub audio_eq_high: f32,
    pub title_text: String,
    pub title_font_size: f32,
    pub solo_track: Option<TrackId>,
    pub show_safe_guides: bool,
    pub monitor_texture_id: Option<egui::TextureId>,
    pub decoders: HashMap<PathBuf, FfmpegDecoder>,
    pub last_rendered_pts_us: i64,
    pub monitor_frame_buffer: Vec<u8>,
    pub needs_monitor_update: bool,
    pub audio_engine: Option<Arc<AudioEngine>>,
    pub audio_metering: MeteringState,
    pub active_tool: EditTool,
    pub journal: Option<rd_core::ProjectJournal>,
    pub show_template_modal: bool,
    pub selected_template_idx: usize,
    pub left_panel_tab: LeftPanelTab,
    pub template_project_name: String,
    pub keyframe_param_selected: String,
    pub keyframe_easing_selected: rd_core::Easing,
}

impl EditorData {
    pub fn new() -> Self {
        let mut project = Project::new("Untitled Project");
        // Initialize with default video and audio tracks
        project.timeline.add_track(TrackKind::Video, "Video 1");
        project.timeline.add_track(TrackKind::Audio, "Audio 1");

        // Initialize production Write-Ahead Log journal & perform crash recovery if needed
        let default_save_path = PathBuf::from("Untitled_Project.reddart");
        let mut journal = rd_core::ProjectJournal::open_or_create(&default_save_path).ok();
        let mut wal_msg = None;
        if let Some(ref mut j) = journal
            && let Ok(report) = j.replay_into(&mut project)
            && report.valid_records_replayed > 0
        {
            wal_msg = Some(format!(
                "Recovered {} uncommitted edits from transaction journal",
                report.valid_records_replayed
            ));
        }

        let mut decoders = HashMap::new();
        let demo_path = [
            PathBuf::from("ready_to_use/windows/Reddart_Master_Demo.mp4"),
            PathBuf::from("Reddart_Master_Demo.mp4"),
            PathBuf::from("samples/Reddart_Master_Demo.mp4"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|exe| {
                exe.parent().and_then(|dir| {
                    let p1 = dir.join("Reddart_Master_Demo.mp4");
                    if p1.exists() {
                        Some(p1)
                    } else {
                        let p2 = dir.join("samples/Reddart_Master_Demo.mp4");
                        if p2.exists() { Some(p2) } else { None }
                    }
                })
            })
        });

        if let Some(ref d_path) = demo_path {
            let demo_str = d_path.to_string_lossy().to_string();
            let clip = Clip::new_video(&demo_str, 0, 0, 10_000_000);
            if let Some(track) = project
                .timeline
                .tracks_mut()
                .iter_mut()
                .find(|t| t.kind == TrackKind::Video)
            {
                track.add_clip(clip);
            }
            if let Ok(decoder) = FfmpegDecoder::open(d_path) {
                decoders.insert(d_path.clone(), decoder);
            }
        }

        // Initialize low-latency CPAL hardware audio engine
        let audio_clock = AudioClock::new();
        let audio_engine = match AudioEngine::new(audio_clock) {
            Ok(eng) => {
                let eng = Arc::new(eng);
                if let Some(ref d_path) = demo_path
                    && let Ok(pcm) = decode_audio_to_pcm(d_path)
                {
                    eng.load_track_pcm(pcm, 1.0);
                }
                Some(eng)
            }
            Err(e) => {
                log::warn!("AudioEngine initialization notice: {:?}", e);
                None
            }
        };

        let audio_metering = MeteringState {
            peak_l_dbfs: -96.0,
            peak_r_dbfs: -96.0,
            rms_l_dbfs: -96.0,
            rms_r_dbfs: -96.0,
            peak_hold_l_dbfs: -96.0,
            peak_hold_r_dbfs: -96.0,
            clipped_l: false,
            clipped_r: false,
        };

        Self {
            workspace_mode: WorkspaceMode::Edit,
            active_tool: EditTool::Select,
            project,
            history: rd_core::CommandHistory::new(100),
            is_playing: false,
            shuttle_speed: 1.0,
            last_frame_time: Instant::now(),
            zoom_px_per_sec: 100.0, // 100 pixels = 1 second
            selected_clip: None,
            selected_track: None,
            media_files: Vec::new(),
            show_export_modal: false,
            export_preset_idx: 0,
            export_destination: None,
            active_export: None,
            export_progress: None,
            export_completed: false,
            last_save_path: None,
            last_autosave_time: Instant::now(),
            missing_media: Vec::new(),
            status_message: Some((
                wal_msg.unwrap_or_else(|| {
                    "Reddart Studio v0.2.1 | Press Space to preview, Ctrl+T for templates, Ctrl+E to export"
                        .into()
                }),
                Instant::now(),
            )),
            snapping_enabled: true,
            master_volume: 1.0,
            color_exposure: 0.0,
            color_contrast: 0.0,
            color_saturation: 1.0,
            color_temperature: 0.0,
            color_tint: 0.0,
            color_lut_idx: 0,
            audio_eq_low: 0.0,
            audio_eq_mid: 0.0,
            audio_eq_high: 0.0,
            title_text: "Reddart Production Title".into(),
            title_font_size: 42.0,
            solo_track: None,
            show_safe_guides: true,
            monitor_texture_id: None,
            decoders,
            last_rendered_pts_us: -999,
            monitor_frame_buffer: vec![18u8; 1920 * 1080 * 4],
            needs_monitor_update: true,
            audio_engine,
            audio_metering,
            journal,
            show_template_modal: false,
            selected_template_idx: 0,
            left_panel_tab: LeftPanelTab::MediaBin,
            template_project_name: "New Template Project".into(),
            keyframe_param_selected: "opacity".into(),
            keyframe_easing_selected: rd_core::Easing::Linear,
        }
    }

    /// Instantiate a curated starter template into the active editor session.
    pub fn instantiate_template(&mut self, template_idx: usize, project_name: Option<&str>) {
        let templates = rd_effects::template::TemplateSpec::starter_templates();
        if let Some(t) = templates.get(template_idx) {
            let media_paths: Vec<String> = if !self.media_files.is_empty() {
                self.media_files
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect()
            } else {
                (1..=t.slots.len())
                    .map(|i| format!("media_slot_{:02}.mp4", i))
                    .collect()
            };

            match t.instantiate(&media_paths) {
                Ok(mut new_project) => {
                    if let Some(name) = project_name {
                        let trimmed = name.trim();
                        if !trimmed.is_empty() {
                            new_project.name = trimmed.to_string();
                        }
                    }
                    let t_name = t.name.clone();
                    let res_w = new_project.resolution.width;
                    let res_h = new_project.resolution.height;
                    let total_clips = new_project.timeline.total_clip_count();

                    let save_path =
                        PathBuf::from(format!("{}.reddart", new_project.name.replace(' ', "_")));
                    self.journal = rd_core::ProjectJournal::open_or_create(&save_path).ok();
                    self.last_save_path = Some(save_path);

                    self.project = new_project;
                    self.history.clear();
                    self.selected_clip = None;
                    self.selected_track = None;
                    self.missing_media.clear();
                    self.needs_monitor_update = true;

                    self.status_message = Some((
                        format!(
                            "Instantiated Template: {} ({}x{}, {} clips)",
                            t_name, res_w, res_h, total_clips
                        ),
                        Instant::now(),
                    ));
                    log::info!("Instantiated template: {} ({}x{})", t_name, res_w, res_h);
                }
                Err(e) => {
                    self.status_message = Some((format!("Template error: {}", e), Instant::now()));
                    log::error!("Template instantiation failed: {}", e);
                }
            }
        }
    }

    /// Record an atomic timeline mutation into the durable write-ahead log.
    pub fn record_mutation(&mut self, mutation: rd_core::JournalMutation) {
        if let Some(ref mut j) = self.journal {
            let _ = j.append(mutation);
        }
    }

    /// Set playback state and synchronize hardware audio engine.
    pub fn set_playing(&mut self, playing: bool) {
        self.is_playing = playing;
        if let Some(ref audio) = self.audio_engine {
            if playing && (self.shuttle_speed - 1.0).abs() < 0.05 {
                audio.seek_to_time_us(self.project.timeline.playhead_us);
                audio.play();
            } else {
                audio.pause();
            }
        }
    }

    /// Toggle playback state.
    pub fn toggle_playback(&mut self) {
        let next = !self.is_playing;
        self.shuttle_speed = 1.0;
        self.set_playing(next);
    }

    /// Seek to a specific timestamp in microseconds and synchronize audio position.
    pub fn seek_to_time_us(&mut self, time_us: i64) {
        self.project.timeline.playhead_us = time_us.max(0);
        if let Some(ref audio) = self.audio_engine {
            audio.seek_to_time_us(time_us);
        }
    }

    /// Set master volume gain and update hardware audio engine.
    pub fn set_master_volume(&mut self, vol: f32) {
        self.master_volume = vol.clamp(0.0, 2.0);
        if let Some(ref audio) = self.audio_engine {
            audio.set_master_volume(self.master_volume);
        }
    }

    /// Update the 1920x1080 Program Monitor hardware GPU texture from live video decode or broadcast raster.
    pub fn update_monitor_frame(&mut self, queue: &wgpu::Queue, texture: &wgpu::Texture) {
        let playhead_us = self.project.timeline.playhead_us;

        // Find active video clip on topmost enabled video track
        let active_clip = self
            .project
            .timeline
            .tracks()
            .iter()
            .rev()
            .filter(|t| t.kind == TrackKind::Video && t.enabled)
            .find_map(|t| t.clip_at(playhead_us).cloned());

        let mut updated = false;

        if let Some(clip) = active_clip {
            if let rd_core::ClipKind::Video(ref vclip) = clip.kind {
                let path = PathBuf::from(&vclip.source_path);
                if path.exists() {
                    let source_time_us = clip.source_in_us
                        + ((playhead_us - clip.position_us) as f64 * clip.speed) as i64;

                    if source_time_us != self.last_rendered_pts_us || self.needs_monitor_update {
                        let decoder = self.decoders.entry(path.clone()).or_insert_with(|| {
                            FfmpegDecoder::open(&path).expect("Failed to initialize video decoder")
                        });

                        if let Ok(Some(frame)) = decoder.decode_frame_at(source_time_us)
                            && let Some(rgba) = frame.rgba_data
                        {
                            Self::resample_and_grade_frame(
                                &rgba,
                                frame.width,
                                frame.height,
                                &mut self.monitor_frame_buffer,
                                1920,
                                1080,
                                ColorGradeSettings {
                                    exposure: self.color_exposure,
                                    contrast: self.color_contrast,
                                    saturation: self.color_saturation,
                                },
                            );

                            // Apply effective keyframed opacity
                            let eff_opacity = clip.effective_opacity(playhead_us);
                            if eff_opacity < 0.999 {
                                for pixel in self.monitor_frame_buffer.as_chunks_mut::<4>().0 {
                                    pixel[0] = ((pixel[0] as f32) * eff_opacity) as u8;
                                    pixel[1] = ((pixel[1] as f32) * eff_opacity) as u8;
                                    pixel[2] = ((pixel[2] as f32) * eff_opacity) as u8;
                                    pixel[3] = ((pixel[3] as f32) * eff_opacity) as u8;
                                }
                            }

                            // Apply shape mask if present on clip (with keyframed mask parameters)
                            if let Some(mask_eff) = clip
                                .effects
                                .iter()
                                .find(|e| e.effect_id == "mask_shape" && e.enabled)
                            {
                                let shape_type = mask_eff
                                    .parameters
                                    .get("shape_type")
                                    .copied()
                                    .unwrap_or(0.0)
                                    as u32;
                                let center_x = clip
                                    .evaluate_keyframe("center_x", playhead_us)
                                    .or_else(|| mask_eff.parameters.get("center_x").copied())
                                    .unwrap_or(0.5);
                                let center_y = clip
                                    .evaluate_keyframe("center_y", playhead_us)
                                    .or_else(|| mask_eff.parameters.get("center_y").copied())
                                    .unwrap_or(0.5);
                                let size_x = clip
                                    .evaluate_keyframe("size_x", playhead_us)
                                    .or_else(|| mask_eff.parameters.get("size_x").copied())
                                    .unwrap_or(0.35);
                                let size_y = clip
                                    .evaluate_keyframe("size_y", playhead_us)
                                    .or_else(|| mask_eff.parameters.get("size_y").copied())
                                    .unwrap_or(0.35);
                                let feather = clip
                                    .evaluate_keyframe("feather", playhead_us)
                                    .or_else(|| mask_eff.parameters.get("feather").copied())
                                    .unwrap_or(0.05);
                                let invert =
                                    mask_eff.parameters.get("invert").copied().unwrap_or(0.0)
                                        >= 0.5;

                                let mask_params = rd_effects::ShapeMaskParams {
                                    shape_type,
                                    center_x,
                                    center_y,
                                    size_x,
                                    size_y,
                                    feather,
                                    invert,
                                };
                                rd_effects::apply_shape_mask(
                                    &mut self.monitor_frame_buffer,
                                    1920,
                                    1080,
                                    &mask_params,
                                );
                            }

                            updated = true;
                            self.last_rendered_pts_us = source_time_us;
                            self.needs_monitor_update = false;
                        }
                    }
                } else if self.last_rendered_pts_us != -2 {
                    Self::generate_offline_graphic(
                        &mut self.monitor_frame_buffer,
                        1920,
                        1080,
                        &vclip.source_path,
                    );
                    updated = true;
                    self.last_rendered_pts_us = -2;
                }
            }
        } else if self.last_rendered_pts_us != playhead_us || self.needs_monitor_update {
            Self::generate_smpte_pattern(&mut self.monitor_frame_buffer, 1920, 1080, playhead_us);
            updated = true;
            self.last_rendered_pts_us = playhead_us;
            self.needs_monitor_update = false;
        }

        // Composite active text / title overlays from all enabled tracks
        for track in self.project.timeline.tracks() {
            if !track.enabled {
                continue;
            }
            if let Some(clip) = track.clip_at(playhead_us)
                && let rd_core::ClipKind::Text(ref tclip) = clip.kind
            {
                let eff_opacity = clip.effective_opacity(playhead_us);
                if eff_opacity > 0.001 {
                    let pos_x = clip
                        .evaluate_keyframe("pos_x", playhead_us)
                        .unwrap_or(tclip.position[0]);
                    let pos_y = clip
                        .evaluate_keyframe("pos_y", playhead_us)
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
                        &mut self.monitor_frame_buffer,
                        1920,
                        1080,
                        &params,
                    );
                    updated = true;
                }
            }
        }

        if updated {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &self.monitor_frame_buffer,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1920 * 4),
                    rows_per_image: Some(1080),
                },
                wgpu::Extent3d {
                    width: 1920,
                    height: 1080,
                    depth_or_array_layers: 1,
                },
            );
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
                (tw, th, 0u32, oy)
            } else {
                let th = dst_h;
                let tw = (dst_h as f32 * src_aspect).round() as u32;
                let ox = (dst_w.saturating_sub(tw)) / 2;
                (tw, th, ox, 0u32)
            };

            // Clear letterbox borders to studio dark graphite
            dst.fill(12);

            for y in 0..target_h {
                let sy = ((y as u64 * src_h as u64) / target_h as u64) as u32;
                let src_row = (sy * src_w * 4) as usize;
                let dst_y = offset_y + y;
                if dst_y >= dst_h {
                    break;
                }
                let dst_row = ((dst_y * dst_w + offset_x) * 4) as usize;

                for x in 0..target_w {
                    let sx = ((x as u64 * src_w as u64) / target_w as u64) as u32;
                    let src_idx = src_row + (sx * 4) as usize;
                    let dst_idx = dst_row + (x * 4) as usize;
                    if src_idx + 4 <= src.len() && dst_idx + 4 <= dst.len() {
                        dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
                    }
                }
            }
        }

        // Apply realtime color grading in-place if non-default
        if grade.exposure != 0.0 || grade.contrast != 0.0 || (grade.saturation - 1.0).abs() > 0.001
        {
            let exp_mult = 2.0_f32.powf(grade.exposure);
            let contrast_mult = 1.0 + grade.contrast;
            for chunk in dst.as_chunks_mut::<4>().0 {
                let mut r = chunk[0] as f32 / 255.0;
                let mut g = chunk[1] as f32 / 255.0;
                let mut b = chunk[2] as f32 / 255.0;

                // Exposure
                r *= exp_mult;
                g *= exp_mult;
                b *= exp_mult;

                // Contrast (pivot at 0.5 midgray)
                r = (r - 0.5) * contrast_mult + 0.5;
                g = (g - 0.5) * contrast_mult + 0.5;
                b = (b - 0.5) * contrast_mult + 0.5;

                // Saturation (Rec. 709 luma)
                if (grade.saturation - 1.0).abs() > 0.001 {
                    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    r = luma + (r - luma) * grade.saturation;
                    g = luma + (g - luma) * grade.saturation;
                    b = luma + (b - luma) * grade.saturation;
                }

                chunk[0] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
                chunk[1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
                chunk[2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }

    /// Generate broadcast SMPTE color bars pattern with studio raster when over an empty timeline region.
    pub fn generate_smpte_pattern(dst: &mut [u8], width: u32, height: u32, time_us: i64) {
        let smpte_75: &[[u8; 4]] = &[
            [191, 191, 191, 255], // White
            [191, 191, 0, 255],   // Yellow
            [0, 191, 191, 255],   // Cyan
            [0, 191, 0, 255],     // Green
            [191, 0, 191, 255],   // Magenta
            [191, 0, 0, 255],     // Red
            [0, 0, 191, 255],     // Blue
        ];

        let bottom_bars: &[[u8; 4]] = &[
            [0, 33, 71, 255],     // Dark Blue
            [255, 255, 255, 255], // White
            [50, 0, 106, 255],    // Dark Purple
            [19, 19, 19, 255],    // Superblack
            [28, 28, 28, 255],    // Black
            [38, 38, 38, 255],    // Gray
            [19, 19, 19, 255],    // Superblack
        ];

        let top_h = (height * 3) / 4;
        let bar_w = width / 7;

        for y in 0..height {
            let is_top = y < top_h;
            let row_offset = (y * width * 4) as usize;

            for x in 0..width {
                let bar_idx = ((x / bar_w) as usize).min(6);
                let col = if is_top {
                    smpte_75[bar_idx]
                } else {
                    bottom_bars[bar_idx]
                };

                let pixel_idx = row_offset + (x * 4) as usize;
                if pixel_idx + 4 <= dst.len() {
                    dst[pixel_idx..pixel_idx + 4].copy_from_slice(&col);
                }
            }
        }

        // Subdued crossline scan indicator
        let scan_x = ((time_us / 25_000) % width as i64) as u32;
        for y in 0..height {
            let pixel_idx = ((y * width + scan_x) * 4) as usize;
            if pixel_idx + 4 <= dst.len() {
                dst[pixel_idx] = dst[pixel_idx].saturating_add(40);
                dst[pixel_idx + 1] = dst[pixel_idx + 1].saturating_add(40);
                dst[pixel_idx + 2] = dst[pixel_idx + 2].saturating_add(40);
            }
        }
    }

    /// Generate an industrial "MEDIA OFFLINE" warning card when a referenced source file is missing.
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
}

// ---------------------------------------------------------------------------
// Precision Studio Control Helpers (Professional NLE Hardware Deck Style)
// ---------------------------------------------------------------------------

/// Precision Studio Industrial Button.
/// Renders a machined graphite button with custom border strokes, hover glow, and active press states.
pub fn studio_btn(
    ui: &mut egui::Ui,
    label: &str,
    accent: Option<egui::Color32>,
    active: bool,
) -> egui::Response {
    let text = egui::RichText::new(label).size(11.5).color(if active {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(220, 225, 235)
    });
    let mut btn = egui::Button::new(text);
    if let Some(accent_col) = accent {
        if active {
            btn = btn.fill(accent_col).stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(191, 219, 254),
            ));
        } else {
            btn = btn
                .fill(egui::Color32::from_rgb(28, 30, 38))
                .stroke(egui::Stroke::new(1.0_f32, accent_col));
        }
    } else if active {
        btn = btn
            .fill(egui::Color32::from_rgb(37, 99, 235))
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(147, 197, 253),
            ));
    } else {
        btn = btn
            .fill(egui::Color32::from_rgb(28, 30, 38))
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(50, 54, 66),
            ));
    }
    ui.add(btn)
}

/// Reset Parameter Pill Button [R].
pub fn studio_reset_btn(ui: &mut egui::Ui) -> egui::Response {
    let btn = egui::Button::new(egui::RichText::new("R").size(9.5).monospace())
        .min_size(egui::vec2(18.0, 18.0))
        .fill(egui::Color32::from_rgb(28, 32, 42))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(50, 56, 72),
        ));
    ui.add(btn).on_hover_text("Reset parameter to default")
}

/// Render a timeline track pill button (e.g., Mute [M], Solo [S], Lock [L]).
pub fn studio_pill_btn(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    active: bool,
    active_bg: egui::Color32,
    active_fg: egui::Color32,
) {
    let (bg, border, fg) = if active {
        (active_bg, active_bg.linear_multiply(1.2), active_fg)
    } else {
        (
            egui::Color32::from_rgb(30, 32, 40),
            egui::Color32::from_rgb(46, 50, 64),
            egui::Color32::from_rgb(148, 163, 184),
        )
    };

    painter.rect_filled(rect, 2.0, bg);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(10.0),
        fg,
    );
}

/// Main application state.
struct ReddartApp {
    state: Option<AppState>,
    initial_project: Option<PathBuf>,
}

/// Rendering state tied to a window.
struct AppState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    adapter: wgpu::Adapter,
    #[allow(dead_code)]
    surface_format: wgpu::TextureFormat,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    monitor_texture: wgpu::Texture,
    #[allow(dead_code)]
    monitor_texture_view: wgpu::TextureView,
    editor: EditorData,
}

impl ReddartApp {
    fn new(initial_project: Option<PathBuf>) -> Self {
        Self {
            state: None,
            initial_project,
        }
    }
}

impl ApplicationHandler for ReddartApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        log::info!("Creating window...");

        let window_attrs = WindowAttributes::default()
            .with_title("Reddart — Video Editor")
            .with_inner_size(LogicalSize::new(1400.0, 900.0))
            .with_min_inner_size(LogicalSize::new(800.0, 600.0));

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("Failed to create window"),
        );

        // Initialize wgpu
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let gpu = pollster::block_on(gpu::GpuContext::new(&instance, Some(&surface)))
            .expect("Failed to initialize GPU");

        let size = window.inner_size();
        let surface_format =
            gpu::configure_surface(&surface, &gpu.adapter, &gpu.device, size.width, size.height);

        // Initialize egui
        let egui_ctx = egui::Context::default();

        // Configure egui style for professional dark NLE theme
        let mut style = (*egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(3);

        style.visuals.panel_fill = egui::Color32::from_rgb(17, 19, 24);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(10, 11, 14);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(24, 26, 33);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);

        // Machined graphite widget states
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(30, 32, 40);
        style.visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(48, 52, 64));
        style.visuals.widgets.inactive.fg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(215, 220, 230));

        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(42, 46, 58);
        style.visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(96, 165, 250));
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(29, 78, 216);
        style.visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(147, 197, 253));
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

        style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(36, 40, 52);
        style.visuals.widgets.open.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(96, 165, 250));

        style.visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 38, 48));

        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        egui_ctx.set_style(style);
        crate::ui::init_studio_ui(&egui_ctx);

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            None,
            None,
            None,
        );

        let mut egui_renderer =
            egui_wgpu::Renderer::new(&gpu.device, surface_format, None, 1, false);

        // Create 1920x1080 Program Monitor hardware texture
        let monitor_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Studio Monitor Texture"),
            size: wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let monitor_texture_view =
            monitor_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let monitor_texture_id = egui_renderer.register_native_texture(
            &gpu.device,
            &monitor_texture_view,
            wgpu::FilterMode::Linear,
        );

        let mut editor = if let Some(ref path) = self.initial_project {
            let mut ed = EditorData::new();
            if path.exists() {
                if let Ok(proj) = rd_core::Project::load_file(path) {
                    ed.project = proj;
                    ed.last_save_path = Some(path.clone());
                    ed.journal = rd_core::ProjectJournal::open_or_create(path).ok();
                    log::info!("Loaded project from CLI: {}", path.display());
                }
            } else {
                ed.project.name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("New Project")
                    .to_string();
                ed.last_save_path = Some(path.clone());
                ed.journal = rd_core::ProjectJournal::open_or_create(path).ok();
                log::info!("Initialized new project from CLI path: {}", path.display());
            }
            ed
        } else {
            EditorData::new()
        };
        editor.monitor_texture_id = Some(monitor_texture_id);
        log::info!("Editor initialized with project: {}", editor.project.name);

        self.state = Some(AppState {
            window,
            surface,
            device: gpu.device,
            queue: gpu.queue,
            adapter: gpu.adapter,
            surface_format,
            egui_ctx,
            egui_state,
            egui_renderer,
            monitor_texture,
            monitor_texture_view,
            editor,
        });

        log::info!("Application initialized successfully");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        // Let egui handle events first
        let egui_response = state.egui_state.on_window_event(&state.window, &event);
        if egui_response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested, shutting down...");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    gpu::configure_surface(
                        &state.surface,
                        &state.adapter,
                        &state.device,
                        new_size.width,
                        new_size.height,
                    );
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                render_frame(state);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

/// Render a frame: handle playback time advancement, run egui UI, then submit GPU commands.
fn render_frame(state: &mut AppState) {
    // 1. Playback time advancement (supports forward & reverse J/K/L shuttle)
    if state.editor.is_playing {
        let now = Instant::now();
        let dt = now.duration_since(state.editor.last_frame_time);
        state.editor.last_frame_time = now;
        let delta_us = (dt.as_micros() as f32 * state.editor.shuttle_speed) as i64;
        let total_us = state.editor.project.timeline.duration_us();

        if delta_us >= 0 {
            state.editor.project.timeline.playhead_us += delta_us;
            if total_us > 0 && state.editor.project.timeline.playhead_us >= total_us {
                // Loop back to start
                state.editor.project.timeline.playhead_us = 0;
            }
        } else {
            let abs_delta = delta_us.unsigned_abs() as i64;
            state.editor.project.timeline.playhead_us = state
                .editor
                .project
                .timeline
                .playhead_us
                .saturating_sub(abs_delta);
            if state.editor.project.timeline.playhead_us == 0 {
                state.editor.is_playing = false;
                state.editor.shuttle_speed = 1.0;
            }
        }
    } else {
        state.editor.last_frame_time = Instant::now();
    }

    // 2. Periodic autosave with WAL checkpointing (every 30 seconds)
    if state.editor.last_autosave_time.elapsed() > std::time::Duration::from_secs(30) {
        let autosave_name = format!(
            ".{}_autosave.reddart",
            state.editor.project.name.replace(' ', "_")
        );
        let path = std::path::PathBuf::from(&autosave_name);
        if let Some(ref mut j) = state.editor.journal {
            let _ = j.checkpoint(&state.editor.project, &path);
        } else {
            let _ = state.editor.project.save_atomic(&path);
        }
        state.editor.last_autosave_time = Instant::now();
        log::debug!("Autosaved project with WAL checkpoint to {}", autosave_name);
    }

    let output = match state.surface.get_current_texture() {
        Ok(output) => output,
        Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
            let size = state.window.inner_size();
            gpu::configure_surface(
                &state.surface,
                &state.adapter,
                &state.device,
                size.width,
                size.height,
            );
            return;
        }
        Err(e) => {
            log::error!("Surface error: {:?}", e);
            return;
        }
    };

    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    // 3. Update Program Monitor live GPU texture
    state
        .editor
        .update_monitor_frame(&state.queue, &state.monitor_texture);

    // 4. Update Audio Telemetry & dBFS Metering
    if let Some(ref audio) = state.editor.audio_engine {
        state.editor.audio_metering = audio.read_metering();
    }

    // Run egui
    let egui_ctx = state.egui_ctx.clone();
    let raw_input = state.egui_state.take_egui_input(&state.window);
    let full_output = egui_ctx.run(raw_input, |ctx| {
        build_editor_ui(ctx, &mut state.editor);
    });

    state
        .egui_state
        .handle_platform_output(&state.window, full_output.platform_output);

    let paint_jobs = state
        .egui_ctx
        .tessellate(full_output.shapes, full_output.pixels_per_point);

    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [
            state.window.inner_size().width,
            state.window.inner_size().height,
        ],
        pixels_per_point: full_output.pixels_per_point,
    };

    // Update egui textures
    for (id, delta) in &full_output.textures_delta.set {
        state
            .egui_renderer
            .update_texture(&state.device, &state.queue, *id, delta);
    }

    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Egui Encoder"),
        });

    state.egui_renderer.update_buffers(
        &state.device,
        &state.queue,
        &mut encoder,
        &paint_jobs,
        &screen_descriptor,
    );

    {
        let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Egui Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.07,
                        g: 0.07,
                        b: 0.09,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        state.egui_renderer.render(
            &mut render_pass.forget_lifetime(),
            &paint_jobs,
            &screen_descriptor,
        );
    }

    state.queue.submit(std::iter::once(encoder.finish()));
    output.present();

    // Free egui textures
    for id in &full_output.textures_delta.free {
        state.egui_renderer.free_texture(id);
    }
}

/// Build the full editor UI using egui.
fn build_editor_ui(ctx: &egui::Context, ed: &mut EditorData) {
    // 0. Handle global keyboard shortcuts
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Space) {
            ed.toggle_playback();
        }
        if i.key_pressed(egui::Key::J) {
            if !ed.is_playing {
                ed.shuttle_speed = -1.0;
                ed.set_playing(true);
            } else if ed.shuttle_speed <= 0.0 {
                ed.shuttle_speed = (ed.shuttle_speed * 2.0).max(-8.0);
            } else {
                ed.shuttle_speed = -1.0;
            }
            ed.status_message =
                Some((format!("Shuttle: {:.0}x", ed.shuttle_speed), Instant::now()));
        }
        if i.key_pressed(egui::Key::K) {
            ed.shuttle_speed = 1.0;
            ed.set_playing(false);
            ed.status_message = Some(("Playback Paused".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::L) {
            if !ed.is_playing {
                ed.shuttle_speed = 1.0;
                ed.set_playing(true);
            } else if ed.shuttle_speed >= 0.0 {
                ed.shuttle_speed = (ed.shuttle_speed * 2.0).min(8.0);
            } else {
                ed.shuttle_speed = 1.0;
            }
            ed.status_message =
                Some((format!("Shuttle: {:.0}x", ed.shuttle_speed), Instant::now()));
        }
        if i.key_pressed(egui::Key::Home) {
            ed.seek_to_time_us(0);
        }
        if i.key_pressed(egui::Key::End) {
            let dur = ed.project.timeline.duration_us();
            ed.seek_to_time_us(dur);
        }
        // NLE Tool Selection Shortcuts: V, C, B, N, Y, U
        if i.key_pressed(egui::Key::V) {
            ed.active_tool = EditTool::Select;
            ed.status_message = Some(("Tool: Selection & Move [V]".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::C) {
            ed.active_tool = EditTool::Razor;
            ed.status_message = Some(("Tool: Razor Split [C]".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::B) {
            ed.active_tool = EditTool::RippleTrim;
            ed.status_message = Some(("Tool: Ripple Trim [B]".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::N) && !i.modifiers.shift {
            ed.active_tool = EditTool::Roll;
            ed.status_message = Some(("Tool: Roll Edit [N]".into(), Instant::now()));
        }
        if !i.modifiers.ctrl && i.key_pressed(egui::Key::Y) {
            ed.active_tool = EditTool::Slip;
            ed.status_message = Some(("Tool: Slip Media [Y]".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::U) {
            ed.active_tool = EditTool::Slide;
            ed.status_message = Some(("Tool: Slide Clip [U]".into(), Instant::now()));
        }

        // Timeline Range & Cue Markers: I, O, X, M
        if i.key_pressed(egui::Key::I) {
            let playhead = ed.project.timeline.playhead_us;
            ed.project.timeline.set_mark_in(playhead);
            ed.record_mutation(rd_core::JournalMutation::SetMarkInOut {
                mark_in_us: ed.project.timeline.mark_in_us,
                mark_out_us: ed.project.timeline.mark_out_us,
            });
            ed.status_message = Some(("Work Area: Mark In Set [I]".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::O) {
            let playhead = ed.project.timeline.playhead_us;
            ed.project.timeline.set_mark_out(playhead);
            ed.record_mutation(rd_core::JournalMutation::SetMarkInOut {
                mark_in_us: ed.project.timeline.mark_in_us,
                mark_out_us: ed.project.timeline.mark_out_us,
            });
            ed.status_message = Some(("Work Area: Mark Out Set [O]".into(), Instant::now()));
        }
        if !i.modifiers.ctrl && i.key_pressed(egui::Key::X) {
            ed.project.timeline.clear_in_out();
            ed.record_mutation(rd_core::JournalMutation::SetMarkInOut {
                mark_in_us: None,
                mark_out_us: None,
            });
            ed.status_message = Some(("Work Area: In/Out Cleared [X]".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::M) {
            let playhead = ed.project.timeline.playhead_us;
            let mid = ed
                .project
                .timeline
                .add_marker(playhead, "Cue", [59, 130, 246]);
            ed.record_mutation(rd_core::JournalMutation::AddMarker {
                id: mid,
                time_us: playhead,
                name: "Cue".into(),
                color_rgb: [59, 130, 246],
            });
            ed.status_message = Some((
                format!("Marker #{} Placed at Playhead [M]", mid),
                Instant::now(),
            ));
        }

        // Duplicate Selected Clip [D]
        if !i.modifiers.ctrl
            && i.key_pressed(egui::Key::D)
            && let Some(cid) = ed.selected_clip
            && let Some(tid) = ed.selected_track
        {
            let maybe_dup = if let Some(track) = ed.project.timeline.track_mut(tid) {
                track
                    .duplicate_clip(cid)
                    .and_then(|new_id| track.clip(new_id).cloned().map(|c| (new_id, c)))
            } else {
                None
            };
            if let Some((new_id, new_clip)) = maybe_dup {
                ed.record_mutation(rd_core::JournalMutation::AddClip {
                    track_id: tid,
                    clip: new_clip,
                });
                ed.selected_clip = Some(new_id);
                ed.status_message = Some(("Duplicated Selected Clip [D]".into(), Instant::now()));
            }
        }

        // Snapping: S or Shift+N
        if (i.key_pressed(egui::Key::S) && !i.modifiers.ctrl && ed.active_tool != EditTool::Razor)
            || (i.modifiers.shift && i.key_pressed(egui::Key::N))
        {
            ed.snapping_enabled = !ed.snapping_enabled;
            ed.status_message = Some((
                format!(
                    "Snapping {}",
                    if ed.snapping_enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                ),
                Instant::now(),
            ));
        }

        // Razor Split at Playhead: Ctrl+K or S (when Razor active)
        if (i.modifiers.ctrl && i.key_pressed(egui::Key::K))
            || (i.key_pressed(egui::Key::S)
                && !i.modifiers.ctrl
                && ed.active_tool == EditTool::Razor)
        {
            let playhead = ed.project.timeline.playhead_us;
            let track_id = ed
                .selected_track
                .or_else(|| ed.project.timeline.tracks().first().map(|t| t.id));
            if let Some(tid) = track_id {
                let maybe_split = if let Some(track) = ed.project.timeline.track_mut(tid)
                    && let Some(clip) = track.clip_at(playhead)
                {
                    let cid = clip.id;
                    track
                        .split_clip_at(cid, playhead)
                        .and_then(|new_cid| track.clip(new_cid).cloned().map(|c| (cid, c)))
                } else {
                    None
                };
                if let Some((orig_cid, new_c)) = maybe_split {
                    ed.record_mutation(rd_core::JournalMutation::SplitClip {
                        track_id: tid,
                        clip_id: orig_cid,
                        split_time_us: playhead,
                        new_clip: new_c,
                    });
                    ed.status_message = Some(("Split Clip at Playhead".into(), Instant::now()));
                }
            }
        }

        // Delete / Ripple Delete: Delete / Backspace / Shift+Delete
        if (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && let Some(cid) = ed.selected_clip.take()
        {
            if i.modifiers.shift {
                let mut ripple_tid = None;
                for track in ed.project.timeline.tracks_mut() {
                    if track.ripple_delete(cid).is_some() {
                        ripple_tid = Some(track.id);
                        break;
                    }
                }
                if let Some(tid) = ripple_tid {
                    ed.record_mutation(rd_core::JournalMutation::RippleDelete {
                        track_id: tid,
                        clip_id: cid,
                    });
                    ed.status_message = Some((
                        "Ripple Deleted Clip (gap closed) [Shift+Del]".into(),
                        Instant::now(),
                    ));
                }
            } else {
                let mut deleted_tids = Vec::new();
                for track in ed.project.timeline.tracks_mut() {
                    if track.remove_clip(cid).is_some() {
                        deleted_tids.push(track.id);
                    }
                }
                for tid in deleted_tids {
                    ed.record_mutation(rd_core::JournalMutation::RemoveClip {
                        track_id: tid,
                        clip_id: cid,
                    });
                }
                ed.status_message = Some(("Deleted Clip [Del]".into(), Instant::now()));
            }
        }
        if i.modifiers.ctrl && i.key_pressed(egui::Key::Z) {
            let _ = ed.history.undo(&mut ed.project);
        }
        if i.modifiers.ctrl && i.key_pressed(egui::Key::Y) {
            let _ = ed.history.redo(&mut ed.project);
        }
        if i.modifiers.ctrl && i.key_pressed(egui::Key::S) {
            let target = ed.last_save_path.clone().unwrap_or_else(|| {
                PathBuf::from(format!("{}.reddart", ed.project.name.replace(' ', "_")))
            });
            let save_res = if let Some(ref mut j) = ed.journal {
                j.checkpoint(&ed.project, &target)
            } else {
                ed.project.save_atomic(&target)
            };
            if let Ok(()) = save_res {
                ed.last_save_path = Some(target.clone());
                ed.status_message = Some((
                    format!("Saved & Checkpointed to {}", target.display()),
                    Instant::now(),
                ));
            }
        }
        if (i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::N))
            || (i.modifiers.ctrl && i.key_pressed(egui::Key::T))
        {
            ed.show_template_modal = true;
        }
        if i.key_pressed(egui::Key::ArrowLeft) {
            let step = if i.modifiers.shift { 33_333 } else { 1_000_000 };
            ed.project.timeline.playhead_us = ed.project.timeline.playhead_us.saturating_sub(step);
        }
        if i.key_pressed(egui::Key::ArrowRight) {
            let step = if i.modifiers.shift { 33_333 } else { 1_000_000 };
            ed.project.timeline.playhead_us += step;
        }
    });

    // 1. Top menu bar & Studio Workspace Selector
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            // Studio Brand Mark
            let logo_rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(28.0, 22.0));
            ui.painter()
                .rect_filled(logo_rect, 3.0, egui::Color32::from_rgb(220, 38, 38));
            ui.painter().text(
                logo_rect.center(),
                egui::Align2::CENTER_CENTER,
                "RD",
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );
            ui.add_space(34.0);

            ui.menu_button("File", |ui| {
                if ui.button("New Project").clicked() {
                    ed.project = Project::new("Untitled Project");
                    ed.project.timeline.add_track(TrackKind::Video, "Video 1");
                    ed.project.timeline.add_track(TrackKind::Audio, "Audio 1");
                    ed.history.clear();
                    ed.selected_clip = None;
                    ed.selected_track = None;
                    ed.last_save_path = None;
                    ed.journal = None;
                    ed.missing_media.clear();
                    ed.status_message = Some(("New Project Created".into(), Instant::now()));
                    ui.close_menu();
                }
                if ui.button("New from Template... (Ctrl+T)").clicked() {
                    ed.show_template_modal = true;
                    ui.close_menu();
                }
                if ui.button("Open Project...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Reddart Project", &["reddart", "json"])
                        .pick_file()
                        && let Ok(mut proj) = Project::load_file(&path)
                    {
                        let mut journal = rd_core::ProjectJournal::open_or_create(&path).ok();
                        let mut msg = format!("Loaded {}", proj.name);
                        if let Some(ref mut j) = journal
                            && let Ok(report) = j.replay_into(&mut proj)
                            && report.valid_records_replayed > 0
                        {
                            msg = format!(
                                "Loaded {} (Recovered {} journal edits)",
                                proj.name, report.valid_records_replayed
                            );
                        }
                        ed.missing_media = proj.find_missing_media();
                        ed.last_save_path = Some(path.clone());
                        ed.project = proj;
                        ed.journal = journal;
                        ed.history.clear();
                        ed.selected_clip = None;
                        ed.status_message = Some((msg, Instant::now()));
                        log::info!("Loaded project: {}", ed.project.name);
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save Project (Ctrl+S)").clicked() {
                    let target = ed.last_save_path.clone().unwrap_or_else(|| {
                        PathBuf::from(format!("{}.reddart", ed.project.name.replace(' ', "_")))
                    });
                    let save_res = if let Some(ref mut j) = ed.journal {
                        j.checkpoint(&ed.project, &target)
                    } else {
                        ed.project.save_atomic(&target)
                    };
                    if let Ok(()) = save_res {
                        ed.last_save_path = Some(target.clone());
                        ed.status_message = Some((
                            format!("Checkpoint Saved: {}", target.display()),
                            Instant::now(),
                        ));
                        log::info!("Checkpoint saved project to {}", target.display());
                    }
                    ui.close_menu();
                }
                if ui.button("Save Project As...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("{}.reddart", ed.project.name))
                        .add_filter("Reddart Project", &["reddart", "json"])
                        .save_file()
                    {
                        let mut journal = rd_core::ProjectJournal::open_or_create(&path).ok();
                        let save_res = if let Some(ref mut j) = journal {
                            j.checkpoint(&ed.project, &path)
                        } else {
                            ed.project.save_atomic(&path)
                        };
                        if let Ok(()) = save_res {
                            ed.last_save_path = Some(path.clone());
                            ed.journal = journal;
                            ed.status_message = Some((
                                format!("Saved As & Checkpointed: {}", path.display()),
                                Instant::now(),
                            ));
                            log::info!("Saved project as {}", path.display());
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Export Media...").clicked() {
                    ed.show_export_modal = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("Edit", |ui| {
                if ui.button("Undo (Ctrl+Z)").clicked() {
                    let _ = ed.history.undo(&mut ed.project);
                    ui.close_menu();
                }
                if ui.button("Redo (Ctrl+Y)").clicked() {
                    let _ = ed.history.redo(&mut ed.project);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Split Clip at Playhead (S)").clicked() {
                    let playhead = ed.project.timeline.playhead_us;
                    let track_id = ed
                        .selected_track
                        .or_else(|| ed.project.timeline.tracks().first().map(|t| t.id));
                    if let Some(tid) = track_id
                        && let Some(track) = ed.project.timeline.track_mut(tid)
                        && let Some(clip) = track.clip_at(playhead)
                    {
                        let cid = clip.id;
                        let _ = track.split_clip_at(cid, playhead);
                    }
                    ui.close_menu();
                }
                if ui.button("Delete Selected Clip (Del)").clicked() {
                    if let Some(cid) = ed.selected_clip.take() {
                        for track in ed.project.timeline.tracks_mut() {
                            track.remove_clip(cid);
                        }
                    }
                    ui.close_menu();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("Zoom In (Timeline)").clicked() {
                    ed.zoom_px_per_sec = (ed.zoom_px_per_sec * 1.25).min(500.0);
                    ui.close_menu();
                }
                if ui.button("Zoom Out (Timeline)").clicked() {
                    ed.zoom_px_per_sec = (ed.zoom_px_per_sec / 1.25).max(10.0);
                    ui.close_menu();
                }
                if ui.button("Fit Sequence to Window").clicked() {
                    let dur_s = (ed.project.timeline.duration_us() as f32 / 1_000_000.0).max(1.0);
                    ed.zoom_px_per_sec = (800.0 / dur_s).clamp(10.0, 300.0);
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(if ed.show_safe_guides {
                        "Hide Safe Area Guides"
                    } else {
                        "Show Safe Area Guides"
                    })
                    .clicked()
                {
                    ed.show_safe_guides = !ed.show_safe_guides;
                    ui.close_menu();
                }
            });

            ui.menu_button("Track", |ui| {
                if ui.button("Add Video Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project
                        .timeline
                        .add_track(TrackKind::Video, format!("Video {}", count));
                    ui.close_menu();
                }
                if ui.button("Add Audio Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project
                        .timeline
                        .add_track(TrackKind::Audio, format!("Audio {}", count));
                    ui.close_menu();
                }
                if ui.button("Add Subtitle Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project
                        .timeline
                        .add_track(TrackKind::Text, format!("Text {}", count));
                    ui.close_menu();
                }
            });

            ui.separator();

            // Precision Segmented Studio Navigation Deck
            if crate::ui::widgets::studio_workspace_tab(
                ui,
                ed.workspace_mode == WorkspaceMode::Edit,
                "EDIT",
                crate::ui::icons::draw_nav_edit,
            )
            .clicked()
            {
                ed.workspace_mode = WorkspaceMode::Edit;
            }
            if crate::ui::widgets::studio_workspace_tab(
                ui,
                ed.workspace_mode == WorkspaceMode::Color,
                "COLOR",
                crate::ui::icons::draw_nav_color,
            )
            .clicked()
            {
                ed.workspace_mode = WorkspaceMode::Color;
            }
            if crate::ui::widgets::studio_workspace_tab(
                ui,
                ed.workspace_mode == WorkspaceMode::Audio,
                "AUDIO",
                crate::ui::icons::draw_nav_audio,
            )
            .clicked()
            {
                ed.workspace_mode = WorkspaceMode::Audio;
            }
            if crate::ui::widgets::studio_workspace_tab(
                ui,
                ed.workspace_mode == WorkspaceMode::Titles,
                "TITLES",
                crate::ui::icons::draw_nav_titles,
            )
            .clicked()
            {
                ed.workspace_mode = WorkspaceMode::Titles;
            }
            if crate::ui::widgets::studio_workspace_tab(
                ui,
                ed.workspace_mode == WorkspaceMode::Export || ed.show_export_modal,
                "DELIVER",
                crate::ui::icons::draw_nav_export,
            )
            .clicked()
            {
                ed.workspace_mode = WorkspaceMode::Export;
                ed.show_export_modal = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("AUDIO: 48.0 kHz 32-bit")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(16, 185, 129)),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("Reddart v{}", env!("CARGO_PKG_VERSION")))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(140, 145, 160)),
                );
                ui.separator();
                if let Some((msg, time)) = &ed.status_message
                    && time.elapsed().as_secs() < 5
                {
                    ui.label(
                        egui::RichText::new(msg)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(96, 165, 250)),
                    );
                }
            });
        });
    });

    // 2. Left Side Panel: Media Browser
    egui::SidePanel::left("media_panel")
        .resizable(true)
        .min_width(220.0)
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let media_tab_btn = ui.selectable_label(
                    ed.left_panel_tab == LeftPanelTab::MediaBin,
                    egui::RichText::new("📁 Media Bin").strong().size(11.0),
                );
                if media_tab_btn.clicked() {
                    ed.left_panel_tab = LeftPanelTab::MediaBin;
                }

                let tmpl_tab_btn = ui.selectable_label(
                    ed.left_panel_tab == LeftPanelTab::Templates,
                    egui::RichText::new("📐 Templates").strong().size(11.0),
                );
                if tmpl_tab_btn.clicked() {
                    ed.left_panel_tab = LeftPanelTab::Templates;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let count_str = match ed.left_panel_tab {
                        LeftPanelTab::MediaBin => format!("[{}]", ed.media_files.len()),
                        LeftPanelTab::Templates => "[5]".to_string(),
                    };
                    ui.label(
                        egui::RichText::new(count_str)
                            .monospace()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(96, 165, 250)),
                    );
                });
            });
            ui.separator();

            match ed.left_panel_tab {
                LeftPanelTab::MediaBin => {
                    if studio_btn(
                        ui,
                        "+ IMPORT MEDIA FILES...",
                        Some(egui::Color32::from_rgb(37, 99, 235)),
                        false,
                    )
                    .on_hover_text("Import video, audio, or graphic assets into project bin")
                    .clicked()
                        && let Some(files) = rfd::FileDialog::new()
                            .add_filter(
                                "Media Files",
                                &[
                                    "mp4", "mov", "mkv", "avi", "webm", "png", "jpg", "jpeg",
                                    "wav", "mp3",
                                ],
                            )
                            .pick_files()
                    {
                        for file in files {
                            let path_str = file.to_string_lossy().to_string();

                            if ed.project.timeline.track_count() == 0 {
                                ed.project.timeline.add_track(TrackKind::Video, "Video 1");
                            }
                            let track_id = ed.project.timeline.tracks()[0].id;
                            let start_pos = ed.project.timeline.duration_us();
                            let default_dur = 5_000_000;

                            let clip = Clip::new_video(&path_str, start_pos, 0, default_dur);
                            if let Some(track) = ed.project.timeline.track_mut(track_id) {
                                track.add_clip(clip);
                            }
                            ed.media_files.push(file);
                            ed.status_message =
                                Some((format!("Imported {}", path_str), Instant::now()));
                        }
                    }

                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Sequence Bin Pool")
                            .size(10.5)
                            .color(egui::Color32::from_rgb(140, 145, 160)),
                    );

                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            if ed.media_files.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Bin is empty.\nClick Import Media Files above.",
                                    )
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(100, 105, 120)),
                                );
                            }
                            for file in &ed.media_files {
                                let filename =
                                    file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                                let ext = file
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                let badge = match ext.as_str() {
                                    "wav" | "mp3" | "aac" | "flac" => "[AUD]",
                                    "png" | "jpg" | "jpeg" | "webp" => "[IMG]",
                                    _ => "[VID]",
                                };
                                let item_label = format!("{} {}", badge, filename);
                                if ui.selectable_label(false, item_label).clicked() {
                                    let track_id = ed
                                        .selected_track
                                        .unwrap_or_else(|| ed.project.timeline.tracks()[0].id);
                                    let clip = Clip::new_video(
                                        file.to_string_lossy(),
                                        ed.project.timeline.playhead_us,
                                        0,
                                        5_000_000,
                                    );
                                    if let Some(track) = ed.project.timeline.track_mut(track_id) {
                                        track.add_clip(clip);
                                        ed.status_message = Some((
                                            format!("Added clip to {}", track.name),
                                            Instant::now(),
                                        ));
                                    }
                                }
                            }
                        });
                }
                LeftPanelTab::Templates => {
                    if ui
                        .add(
                            crate::ui::widgets::StudioButton::new("+ OPEN TEMPLATE GALLERY...")
                                .accent(egui::Color32::from_rgb(37, 99, 235))
                                .active(true)
                                .icon(crate::ui::icons::draw_template),
                        )
                        .on_hover_text("Open visual template gallery with slot inspector (Ctrl+T)")
                        .clicked()
                    {
                        ed.show_template_modal = true;
                    }

                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("CURATED STARTER TEMPLATES")
                            .strong()
                            .size(10.5)
                            .color(egui::Color32::from_rgb(140, 145, 160)),
                    );

                    let templates = rd_effects::template::TemplateSpec::starter_templates();
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for (idx, tmpl) in templates.iter().enumerate() {
                                ui.group(|ui| {
                                    let badge = if tmpl.aspect_width < tmpl.aspect_height {
                                        "[9:16]"
                                    } else if tmpl.aspect_width == tmpl.aspect_height {
                                        "[1:1]"
                                    } else if tmpl.aspect_width >= 3840 {
                                        "[4K]"
                                    } else {
                                        "[16:9]"
                                    };

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(badge)
                                                .monospace()
                                                .size(10.0)
                                                .color(egui::Color32::from_rgb(0, 229, 255)),
                                        );
                                        ui.label(
                                            egui::RichText::new(&tmpl.name)
                                                .strong()
                                                .size(11.0)
                                                .color(egui::Color32::WHITE),
                                        );
                                    });

                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{}x{} | {:.0}s | {} slots",
                                            tmpl.aspect_width,
                                            tmpl.aspect_height,
                                            tmpl.duration_us as f64 / 1_000_000.0,
                                            tmpl.slots.len()
                                        ))
                                        .size(9.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );

                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("Apply")
                                            .on_hover_text("Create project from this template")
                                            .clicked()
                                        {
                                            ed.instantiate_template(idx, None);
                                        }
                                        if ui.small_button("Details...").clicked() {
                                            ed.selected_template_idx = idx;
                                            ed.template_project_name = format!("New {}", tmpl.name);
                                            ed.show_template_modal = true;
                                        }
                                    });
                                });
                                ui.add_space(3.0);
                            }
                        });
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.label(
                egui::RichText::new("PROJECT METRICS")
                    .strong()
                    .size(11.0)
                    .color(egui::Color32::from_rgb(200, 205, 220)),
            );

            // Machined Project Specs Card
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new(format!("Name: {}", ed.project.name))
                        .size(11.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Format: {}x{} Progressive",
                        ed.project.resolution.width, ed.project.resolution.height
                    ))
                    .size(10.5),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Timebase: {:.2} fps NTSC",
                        ed.project.frame_rate.as_f64()
                    ))
                    .size(10.5),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Total Clips: {}",
                        ed.project.timeline.total_clip_count()
                    ))
                    .size(10.5),
                );
                ui.label(
                    egui::RichText::new("Autosave: Active (30s Interval)")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(52, 211, 153)),
                );
            });
        });

    // 3. Right Side Panel: Properties & Effects Inspector (Workspace-adaptive)
    egui::SidePanel::right("properties_panel")
        .resizable(true)
        .min_width(280.0)
        .default_width(330.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            match ed.workspace_mode {
                WorkspaceMode::Edit => {
                    ui.label(
                        egui::RichText::new("INSPECTOR (EDIT & TRANSFORM)")
                            .strong()
                            .size(11.5)
                            .color(egui::Color32::from_rgb(220, 225, 235)),
                    );
                    ui.separator();

                    let mut clip_found = false;
                    if let Some(selected_cid) = ed.selected_clip {
                        let playhead_us = ed.project.timeline.playhead_us;
                        let fps_val = ed.project.frame_rate.as_f64();
                        let mut kf_param = ed.keyframe_param_selected.clone();
                        let mut kf_easing = ed.keyframe_easing_selected;
                        let mut trigger_update = false;

                        for track in ed.project.timeline.tracks_mut() {
                            let track_name = track.name.clone();
                            if let Some(clip) =
                                track.clips_mut().iter_mut().find(|c| c.id == selected_cid)
                            {
                                clip_found = true;
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("[{}]", track_name))
                                                .strong()
                                                .color(egui::Color32::from_rgb(96, 165, 250)),
                                        );
                                        ui.label(format!("Clip ID: {}", clip.id.0));
                                    });
                                    ui.label(format!(
                                        "In: {} | Dur: {}",
                                        format_smpte(clip.position_us, fps_val),
                                        format_smpte(clip.duration_us(), fps_val)
                                    ));
                                });

                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("GEOMETRIC TRANSFORM")
                                        .strong()
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(180, 185, 200)),
                                );

                                ui.horizontal(|ui| {
                                    ui.label("Pos X:");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut clip.transform.position.x)
                                                .speed(1.0),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    ui.label("Y:");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut clip.transform.position.y)
                                                .speed(1.0),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    if studio_reset_btn(ui).clicked() {
                                        clip.transform.position.x = 0.0;
                                        clip.transform.position.y = 0.0;
                                        trigger_update = true;
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Scale X:");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut clip.transform.scale.x)
                                                .speed(0.01)
                                                .range(0.05..=10.0),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    ui.label("Y:");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut clip.transform.scale.y)
                                                .speed(0.01)
                                                .range(0.05..=10.0),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    if studio_reset_btn(ui).clicked() {
                                        clip.transform.scale.x = 1.0;
                                        clip.transform.scale.y = 1.0;
                                        trigger_update = true;
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Rotation:");
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut clip.transform.rotation,
                                                -180.0..=180.0,
                                            )
                                            .suffix("°"),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    if studio_reset_btn(ui).clicked() {
                                        clip.transform.rotation = 0.0;
                                        trigger_update = true;
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Opacity:");
                                    if ui
                                        .add(egui::Slider::new(&mut clip.opacity, 0.0..=1.0))
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    if studio_reset_btn(ui).clicked() {
                                        clip.opacity = 1.0;
                                        trigger_update = true;
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Speed:");
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut clip.speed, 0.25..=4.0)
                                                .suffix("x"),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    if studio_reset_btn(ui).clicked() {
                                        clip.speed = 1.0;
                                        trigger_update = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    if ui.small_button("0.5x Slow").clicked() {
                                        clip.speed = 0.5;
                                        trigger_update = true;
                                    }
                                    if ui.small_button("1.0x").clicked() {
                                        clip.speed = 1.0;
                                        trigger_update = true;
                                    }
                                    if ui.small_button("1.5x").clicked() {
                                        clip.speed = 1.5;
                                        trigger_update = true;
                                    }
                                    if ui.small_button("2.0x Fast").clicked() {
                                        clip.speed = 2.0;
                                        trigger_update = true;
                                    }
                                    if ui.small_button("4.0x").clicked() {
                                        clip.speed = 4.0;
                                        trigger_update = true;
                                    }
                                });

                                if let rd_core::ClipKind::Text(ref mut tclip) = clip.kind {
                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.label(
                                        egui::RichText::new("TITLE & SUBTITLE SETTINGS")
                                            .strong()
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(217, 119, 6)),
                                    );
                                    ui.label("Text Content:");
                                    if ui
                                        .add(
                                            egui::TextEdit::multiline(&mut tclip.text)
                                                .desired_rows(2)
                                                .desired_width(f32::INFINITY),
                                        )
                                        .changed()
                                    {
                                        trigger_update = true;
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label("Font Size:");
                                        if ui
                                            .add(
                                                egui::Slider::new(
                                                    &mut tclip.font_size,
                                                    12.0..=120.0,
                                                )
                                                .suffix(" pt"),
                                            )
                                            .changed()
                                        {
                                            trigger_update = true;
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Pos X:");
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut tclip.position[0], 0.0..=1.0)
                                                    .step_by(0.01),
                                            )
                                            .changed()
                                        {
                                            trigger_update = true;
                                        }
                                        ui.label("Y:");
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut tclip.position[1], 0.0..=1.0)
                                                    .step_by(0.01),
                                            )
                                            .changed()
                                        {
                                            trigger_update = true;
                                        }
                                    });
                                }

                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("AUDIO VOLUME & PAN")
                                        .strong()
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(180, 185, 200)),
                                );
                                ui.horizontal(|ui| {
                                    ui.label("Volume:");
                                    ui.add(
                                        egui::Slider::new(&mut clip.volume, 0.0..=2.0).suffix("x"),
                                    );
                                    if studio_reset_btn(ui).clicked() {
                                        clip.volume = 1.0;
                                    }
                                });
                                let c_db = if clip.volume <= 0.001 {
                                    -60.0
                                } else {
                                    20.0 * clip.volume.log10()
                                };
                                ui.label(
                                    egui::RichText::new(format!("Gain: {:.1} dB", c_db))
                                        .monospace()
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(52, 211, 153)),
                                );
                                ui.horizontal(|ui| {
                                    ui.label("Pan:");
                                    ui.add(egui::Slider::new(&mut clip.pan, -1.0..=1.0));
                                    if studio_reset_btn(ui).clicked() {
                                        clip.pan = 0.0;
                                    }
                                });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.label(
                                    egui::RichText::new("EFFECTS & MASKING PIPELINE")
                                        .strong()
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(180, 185, 200)),
                                );

                                ui.horizontal(|ui| {
                                    if studio_btn(
                                        ui,
                                        "+ SHAPE MASK",
                                        Some(egui::Color32::from_rgb(16, 185, 129)),
                                        false,
                                    )
                                    .on_hover_text("Add Shape Mask (Rect / Ellipse / Vignette)")
                                    .clicked()
                                    {
                                        let mut params = std::collections::HashMap::new();
                                        params.insert("shape_type".into(), 0.0);
                                        params.insert("center_x".into(), 0.5);
                                        params.insert("center_y".into(), 0.5);
                                        params.insert("size_x".into(), 0.35);
                                        params.insert("size_y".into(), 0.35);
                                        params.insert("feather".into(), 0.05);
                                        params.insert("invert".into(), 0.0);
                                        clip.effects.push(rd_core::ClipEffect {
                                            effect_id: "mask_shape".into(),
                                            parameters: params,
                                            enabled: true,
                                        });
                                        trigger_update = true;
                                    }

                                    if studio_btn(
                                        ui,
                                        "+ BRIGHTNESS",
                                        Some(egui::Color32::from_rgb(37, 99, 235)),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        let mut params = std::collections::HashMap::new();
                                        params.insert("brightness".into(), 0.05);
                                        params.insert("contrast".into(), 0.1);
                                        clip.effects.push(rd_core::ClipEffect {
                                            effect_id: "brightness_contrast".into(),
                                            parameters: params,
                                            enabled: true,
                                        });
                                        trigger_update = true;
                                    }
                                });

                                let mut to_remove = None;
                                for (e_idx, effect) in clip.effects.iter_mut().enumerate() {
                                    ui.group(|ui| {
                                        ui.horizontal(|ui| {
                                            if ui
                                                .checkbox(&mut effect.enabled, &effect.effect_id)
                                                .changed()
                                            {
                                                trigger_update = true;
                                            }
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if studio_btn(
                                                        ui,
                                                        "Remove",
                                                        Some(egui::Color32::from_rgb(220, 38, 38)),
                                                        false,
                                                    )
                                                    .on_hover_text("Remove this effect pass")
                                                    .clicked()
                                                    {
                                                        to_remove = Some(e_idx);
                                                        trigger_update = true;
                                                    }
                                                },
                                            );
                                        });

                                        if effect.effect_id == "mask_shape" {
                                            let shape_names = [
                                                "0: Rectangle",
                                                "1: Ellipse",
                                                "2: Linear Gradient",
                                                "3: Vignette",
                                            ];
                                            let mut cur_st = effect
                                                .parameters
                                                .get("shape_type")
                                                .copied()
                                                .unwrap_or(0.0)
                                                as usize;
                                            if cur_st >= 4 {
                                                cur_st = 0;
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label("Shape:");
                                                egui::ComboBox::from_id_salt(format!(
                                                    "mask_shape_cb_{}",
                                                    e_idx
                                                ))
                                                .selected_text(shape_names[cur_st])
                                                .show_ui(ui, |ui| {
                                                    for (s_i, s_name) in
                                                        shape_names.iter().enumerate()
                                                    {
                                                        if ui
                                                            .selectable_value(
                                                                &mut cur_st,
                                                                s_i,
                                                                *s_name,
                                                            )
                                                            .clicked()
                                                        {
                                                            effect.parameters.insert(
                                                                "shape_type".into(),
                                                                cur_st as f32,
                                                            );
                                                            trigger_update = true;
                                                        }
                                                    }
                                                });
                                            });

                                            let mut cx = effect
                                                .parameters
                                                .get("center_x")
                                                .copied()
                                                .unwrap_or(0.5);
                                            let mut cy = effect
                                                .parameters
                                                .get("center_y")
                                                .copied()
                                                .unwrap_or(0.5);
                                            ui.horizontal(|ui| {
                                                ui.label("Center X:");
                                                if ui
                                                    .add(egui::Slider::new(&mut cx, 0.0..=1.0))
                                                    .changed()
                                                {
                                                    effect
                                                        .parameters
                                                        .insert("center_x".into(), cx);
                                                    trigger_update = true;
                                                }
                                                ui.label("Y:");
                                                if ui
                                                    .add(egui::Slider::new(&mut cy, 0.0..=1.0))
                                                    .changed()
                                                {
                                                    effect
                                                        .parameters
                                                        .insert("center_y".into(), cy);
                                                    trigger_update = true;
                                                }
                                            });

                                            let mut sx = effect
                                                .parameters
                                                .get("size_x")
                                                .copied()
                                                .unwrap_or(0.35);
                                            let mut sy = effect
                                                .parameters
                                                .get("size_y")
                                                .copied()
                                                .unwrap_or(0.35);
                                            ui.horizontal(|ui| {
                                                ui.label("Size X:");
                                                if ui
                                                    .add(egui::Slider::new(&mut sx, 0.01..=1.0))
                                                    .changed()
                                                {
                                                    effect.parameters.insert("size_x".into(), sx);
                                                    trigger_update = true;
                                                }
                                                ui.label("Y:");
                                                if ui
                                                    .add(egui::Slider::new(&mut sy, 0.01..=1.0))
                                                    .changed()
                                                {
                                                    effect.parameters.insert("size_y".into(), sy);
                                                    trigger_update = true;
                                                }
                                            });

                                            let mut feather = effect
                                                .parameters
                                                .get("feather")
                                                .copied()
                                                .unwrap_or(0.05);
                                            let mut invert = effect
                                                .parameters
                                                .get("invert")
                                                .copied()
                                                .unwrap_or(0.0)
                                                >= 0.5;
                                            ui.horizontal(|ui| {
                                                ui.label("Feather:");
                                                if ui
                                                    .add(egui::Slider::new(
                                                        &mut feather,
                                                        0.001..=0.5,
                                                    ))
                                                    .changed()
                                                {
                                                    effect
                                                        .parameters
                                                        .insert("feather".into(), feather);
                                                    trigger_update = true;
                                                }
                                                if ui.checkbox(&mut invert, "Invert").changed() {
                                                    effect.parameters.insert(
                                                        "invert".into(),
                                                        if invert { 1.0 } else { 0.0 },
                                                    );
                                                    trigger_update = true;
                                                }
                                            });

                                            ui.horizontal(|ui| {
                                                if ui.small_button("Center Spot").clicked() {
                                                    effect
                                                        .parameters
                                                        .insert("shape_type".into(), 1.0);
                                                    effect
                                                        .parameters
                                                        .insert("size_x".into(), 0.35);
                                                    effect
                                                        .parameters
                                                        .insert("size_y".into(), 0.35);
                                                    effect
                                                        .parameters
                                                        .insert("feather".into(), 0.08);
                                                    trigger_update = true;
                                                }
                                                if ui.small_button("Letterbox").clicked() {
                                                    effect
                                                        .parameters
                                                        .insert("shape_type".into(), 0.0);
                                                    effect
                                                        .parameters
                                                        .insert("size_x".into(), 0.50);
                                                    effect
                                                        .parameters
                                                        .insert("size_y".into(), 0.38);
                                                    effect
                                                        .parameters
                                                        .insert("feather".into(), 0.01);
                                                    trigger_update = true;
                                                }
                                                if ui.small_button("Vignette").clicked() {
                                                    effect
                                                        .parameters
                                                        .insert("shape_type".into(), 3.0);
                                                    effect
                                                        .parameters
                                                        .insert("size_x".into(), 0.45);
                                                    effect
                                                        .parameters
                                                        .insert("feather".into(), 0.15);
                                                    trigger_update = true;
                                                }
                                            });
                                        } else {
                                            for (k, v) in &mut effect.parameters {
                                                ui.horizontal(|ui| {
                                                    ui.label(k.as_str());
                                                    if ui
                                                        .add(egui::Slider::new(v, -1.0..=1.0))
                                                        .changed()
                                                    {
                                                        trigger_update = true;
                                                    }
                                                    if studio_reset_btn(ui).clicked() {
                                                        *v = 0.0;
                                                        trigger_update = true;
                                                    }
                                                });
                                            }
                                        }
                                    });
                                }
                                if let Some(idx) = to_remove {
                                    clip.effects.remove(idx);
                                }

                                // Keyframe Animator & Automation Section
                                ui.add_space(8.0);
                                ui.separator();
                                ui.label(
                                    egui::RichText::new("KEYFRAME ANIMATOR & AUTOMATION")
                                        .strong()
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(180, 185, 200)),
                                );

                                let anim_params = [
                                    ("opacity", "Opacity (0..1)"),
                                    ("pos_x", "Position X"),
                                    ("pos_y", "Position Y"),
                                    ("scale_x", "Scale X"),
                                    ("scale_y", "Scale Y"),
                                    ("rotation", "Rotation (deg)"),
                                    ("speed", "Speed Multiplier"),
                                    ("volume", "Audio Volume"),
                                    ("size_x", "Mask Size X"),
                                    ("feather", "Mask Feather"),
                                ];

                                ui.horizontal(|ui| {
                                    ui.label("Param:");
                                    egui::ComboBox::from_id_salt("kf_param_combo")
                                        .selected_text(
                                            anim_params
                                                .iter()
                                                .find(|(id, _)| id == &kf_param)
                                                .map(|(_, name)| *name)
                                                .unwrap_or("Opacity"),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (id, name) in &anim_params {
                                                if ui
                                                    .selectable_label(&kf_param == id, *name)
                                                    .clicked()
                                                {
                                                    kf_param = (*id).to_string();
                                                }
                                            }
                                        });
                                });

                                let easing_modes = [
                                    (rd_core::Easing::Linear, "Linear"),
                                    (rd_core::Easing::EaseIn, "Ease In"),
                                    (rd_core::Easing::EaseOut, "Ease Out"),
                                    (rd_core::Easing::EaseInOut, "Ease In-Out"),
                                    (rd_core::Easing::Hold, "Hold"),
                                ];

                                ui.horizontal(|ui| {
                                    ui.label("Easing:");
                                    egui::ComboBox::from_id_salt("kf_easing_combo")
                                        .selected_text(match kf_easing {
                                            rd_core::Easing::Linear => "Linear",
                                            rd_core::Easing::EaseIn => "Ease In",
                                            rd_core::Easing::EaseOut => "Ease Out",
                                            rd_core::Easing::EaseInOut => "Ease In-Out",
                                            rd_core::Easing::Hold => "Hold",
                                            _ => "Custom",
                                        })
                                        .show_ui(ui, |ui| {
                                            for (e, name) in &easing_modes {
                                                if ui
                                                    .selectable_label(&kf_easing == e, *name)
                                                    .clicked()
                                                {
                                                    kf_easing = *e;
                                                }
                                            }
                                        });
                                });

                                let rel_time_us = (playhead_us - clip.position_us).max(0);
                                let cur_val = match kf_param.as_str() {
                                    "opacity" => clip.effective_opacity(playhead_us),
                                    "pos_x" => clip.effective_transform(playhead_us).position.x,
                                    "pos_y" => clip.effective_transform(playhead_us).position.y,
                                    "scale_x" => clip.effective_transform(playhead_us).scale.x,
                                    "scale_y" => clip.effective_transform(playhead_us).scale.y,
                                    "rotation" => clip.effective_transform(playhead_us).rotation,
                                    "speed" => clip.effective_speed(playhead_us) as f32,
                                    "volume" => clip.effective_volume(playhead_us),
                                    "size_x" => clip
                                        .evaluate_keyframe("size_x", playhead_us)
                                        .unwrap_or(0.35),
                                    "feather" => clip
                                        .evaluate_keyframe("feather", playhead_us)
                                        .unwrap_or(0.05),
                                    _ => 1.0,
                                };

                                ui.horizontal(|ui| {
                                    ui.label(format!("At Playhead: {:.2}", cur_val));
                                    if studio_btn(
                                        ui,
                                        "◆ Add Keyframe",
                                        Some(egui::Color32::from_rgb(234, 179, 8)),
                                        true,
                                    )
                                    .on_hover_text(
                                        "Add or update keyframe for selected parameter at current playhead",
                                    )
                                    .clicked()
                                    {
                                        clip.set_keyframe(&kf_param, rel_time_us, cur_val, kf_easing);
                                        trigger_update = true;
                                    }
                                });

                                if let Some(track) = clip.keyframes.get_mut(&kf_param)
                                    && !track.is_empty()
                                {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Active Keyframes ({})",
                                            track.len()
                                        ))
                                        .size(10.5)
                                        .color(egui::Color32::from_rgb(200, 205, 220)),
                                    );
                                    let mut kf_remove_idx = None;
                                    egui::ScrollArea::vertical()
                                        .max_height(100.0)
                                        .show(ui, |ui| {
                                            for (k_idx, kf) in
                                                track.keyframes().iter().enumerate()
                                            {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "◆ {}",
                                                            format_smpte(
                                                                clip.position_us + kf.time_us,
                                                                fps_val
                                                            )
                                                        ))
                                                        .monospace()
                                                        .size(10.5)
                                                        .color(egui::Color32::from_rgb(
                                                            234, 179, 8,
                                                        )),
                                                    );
                                                    ui.label(format!("Val: {:.2}", kf.value));
                                                    if ui
                                                        .small_button("✕")
                                                        .on_hover_text("Remove this keyframe")
                                                        .clicked()
                                                    {
                                                        kf_remove_idx = Some(k_idx);
                                                        trigger_update = true;
                                                    }
                                                });
                                            }
                                        });
                                    if let Some(r_idx) = kf_remove_idx {
                                        track.remove_keyframe(r_idx);
                                    }
                                }

                                break;
                            }
                        }

                        ed.keyframe_param_selected = kf_param;
                        ed.keyframe_easing_selected = kf_easing;
                        if trigger_update {
                            ed.needs_monitor_update = true;
                        }
                    }

                    if !clip_found {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("No clip selected on timeline.\nClick any clip to inspect and adjust parameters.")
                                .color(egui::Color32::from_rgb(110, 115, 130))
                                .size(11.0),
                        );
                    }
                }
                WorkspaceMode::Color => {
                    ui.label(
                        egui::RichText::new("COLOR GRADING (PRIMARY WHEELS & LUT)")
                            .strong()
                            .size(11.5)
                            .color(egui::Color32::from_rgb(220, 225, 235)),
                    );
                    ui.separator();

                    ui.label(
                        egui::RichText::new("PRIMARY TONAL CORRECTION")
                            .strong()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );

                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.color_exposure, -3.0..=3.0)
                                .text("Exposure")
                                .suffix(" EV"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.color_exposure = 0.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.color_contrast, -1.0..=1.0).text("Contrast"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.color_contrast = 0.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.color_saturation, 0.0..=2.0)
                                .text("Saturation"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.color_saturation = 1.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.color_temperature, -100.0..=100.0)
                                .text("Temp (K)"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.color_temperature = 0.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.color_tint, -100.0..=100.0)
                                .text("Tint (M/G)"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.color_tint = 0.0;
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new("3D 33-POINT LUT PROFILE")
                            .strong()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );

                    let lut_presets = [
                        "Rec.709 Standard (Neutral)",
                        "Teal & Orange (Hollywood Blockbuster)",
                        "Golden Hour (Vintage Warm)",
                        "High-Contrast B&W (Noir)",
                        "Kodak 2383 Print Film Emulation",
                        "Fujifilm Eterna 500T",
                    ];

                    let current_lut = lut_presets[ed.color_lut_idx];
                    egui::ComboBox::from_id_salt("color_lut_combo")
                        .selected_text(current_lut)
                        .show_ui(ui, |ui| {
                            for (idx, name) in lut_presets.iter().enumerate() {
                                ui.selectable_value(&mut ed.color_lut_idx, idx, *name);
                            }
                        });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if studio_btn(ui, "Reset Grade", None, false)
                            .on_hover_text("Reset all color grading adjustments to neutral")
                            .clicked()
                        {
                            ed.color_exposure = 0.0;
                            ed.color_contrast = 0.0;
                            ed.color_saturation = 1.0;
                            ed.color_temperature = 0.0;
                            ed.color_tint = 0.0;
                            ed.color_lut_idx = 0;
                            ed.status_message = Some(("Color Grade Reset".into(), Instant::now()));
                        }

                        if let Some(cid) = ed.selected_clip
                            && studio_btn(
                                ui,
                                "Apply to Clip",
                                Some(egui::Color32::from_rgb(37, 99, 235)),
                                true,
                            )
                            .on_hover_text("Bake active color parameters to selected timeline clip")
                            .clicked()
                        {
                            for track in ed.project.timeline.tracks_mut() {
                                if let Some(clip) =
                                    track.clips_mut().iter_mut().find(|c| c.id == cid)
                                {
                                    let mut params = std::collections::HashMap::new();
                                    params.insert("exposure".into(), ed.color_exposure);
                                    params.insert("contrast".into(), ed.color_contrast);
                                    params.insert("saturation".into(), ed.color_saturation);
                                    params.insert("temperature".into(), ed.color_temperature);
                                    params.insert("tint".into(), ed.color_tint);
                                    clip.effects.retain(|e| e.effect_id != "primary_grade");
                                    clip.effects.push(rd_core::ClipEffect {
                                        effect_id: "primary_grade".into(),
                                        parameters: params,
                                        enabled: true,
                                    });
                                    ed.status_message =
                                        Some(("Grade Applied to Clip".into(), Instant::now()));
                                }
                            }
                        }
                    });
                }
                WorkspaceMode::Audio => {
                    ui.label(
                        egui::RichText::new("AUDIO MIXER & MASTER BUS")
                            .strong()
                            .size(11.5)
                            .color(egui::Color32::from_rgb(220, 225, 235)),
                    );
                    ui.separator();

                    ui.label(
                        egui::RichText::new("MASTER BUS FADER")
                            .strong()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Slider::new(&mut ed.master_volume, 0.0..=2.0)
                                    .text("Master Fader"),
                            )
                            .changed()
                        {
                            ed.set_master_volume(ed.master_volume);
                        }
                        if studio_reset_btn(ui).clicked() {
                            ed.set_master_volume(1.0);
                        }
                    });
                    let db = if ed.master_volume <= 0.001 {
                        -60.0
                    } else {
                        20.0 * ed.master_volume.log10()
                    };
                    ui.label(
                        egui::RichText::new(format!("Gain: {:.1} dB", db))
                            .monospace()
                            .size(11.0)
                            .color(if db > 0.0 {
                                egui::Color32::from_rgb(239, 68, 68)
                            } else {
                                egui::Color32::from_rgb(52, 211, 153)
                            }),
                    );

                    ui.add_space(6.0);
                    ui.add(
                        crate::ui::widgets::StudioAudioMeter::new(ed.audio_metering)
                            .height(30.0),
                    );

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new("3-BAND PARAMETRIC EQUALIZER")
                            .strong()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.audio_eq_low, -12.0..=12.0)
                                .text("Low (100 Hz)")
                                .suffix(" dB"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.audio_eq_low = 0.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.audio_eq_mid, -12.0..=12.0)
                                .text("Mid (1 kHz)")
                                .suffix(" dB"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.audio_eq_mid = 0.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.audio_eq_high, -12.0..=12.0)
                                .text("High (10 kHz)")
                                .suffix(" dB"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.audio_eq_high = 0.0;
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new("AUDIO TRACK STRIPS")
                            .strong()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );
                    for track in ed.project.timeline.tracks_mut() {
                        if track.kind == TrackKind::Audio {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut track.enabled, &track.name);
                                    let lock_text = if track.locked { "Locked" } else { "Unlock" };
                                    if studio_btn(
                                        ui,
                                        lock_text,
                                        if track.locked {
                                            Some(egui::Color32::from_rgb(217, 119, 6))
                                        } else {
                                            None
                                        },
                                        track.locked,
                                    )
                                    .clicked()
                                    {
                                        track.locked = !track.locked;
                                    }
                                });
                            });
                        }
                    }
                }
                WorkspaceMode::Titles => {
                    ui.label(
                        egui::RichText::new("TITLE & SUBTITLE DESIGNER")
                            .strong()
                            .size(11.5)
                            .color(egui::Color32::from_rgb(220, 225, 235)),
                    );
                    ui.separator();

                    ui.label(
                        egui::RichText::new("Overlay Text String:")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut ed.title_text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut ed.title_font_size, 14.0..=120.0)
                                .text("Font Size")
                                .suffix(" pt"),
                        );
                        if studio_reset_btn(ui).clicked() {
                            ed.title_font_size = 42.0;
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Layout Presets:")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );
                    ui.horizontal(|ui| {
                        if studio_btn(ui, "Lower Third", None, false).clicked() {
                            ed.title_text = "Featured Speaker | Keynote".into();
                            ed.title_font_size = 32.0;
                        }
                        if studio_btn(ui, "Center Subtitle", None, false).clicked() {
                            ed.title_text = "Broadcast dialogue line caption".into();
                            ed.title_font_size = 28.0;
                        }
                        if studio_btn(ui, "Main Title", None, false).clicked() {
                            ed.title_text = "REDDART CINEMA".into();
                            ed.title_font_size = 56.0;
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    if studio_btn(
                        ui,
                        "+ ADD TITLE CLIP TO SEQUENCE",
                        Some(egui::Color32::from_rgb(217, 119, 6)),
                        true,
                    )
                    .clicked()
                    {
                        let text_track = ed
                            .project
                            .timeline
                            .tracks()
                            .iter()
                            .find(|t| t.kind == TrackKind::Text)
                            .map(|t| t.id);
                        let target_track_id = match text_track {
                            Some(tid) => tid,
                            None => ed.project.timeline.add_track(TrackKind::Text, "Text 1"),
                        };
                        let playhead = ed.project.timeline.playhead_us;
                        let mut clip = Clip::new_text(&ed.title_text, playhead, 4_000_000);
                        if let rd_core::ClipKind::Text(ref mut tclip) = clip.kind {
                            tclip.font_size = ed.title_font_size;
                        }
                        if let Some(track) = ed.project.timeline.track_mut(target_track_id) {
                            track.add_clip(clip);
                            ed.status_message = Some(("Title Clip Added".into(), Instant::now()));
                            ed.needs_monitor_update = true;
                        }
                    }
                }
                WorkspaceMode::Export => {
                    ui.label(
                        egui::RichText::new("DELIVER & EXPORT")
                            .strong()
                            .size(11.5)
                            .color(egui::Color32::from_rgb(220, 225, 235)),
                    );
                    ui.separator();
                    ui.group(|ui| {
                        ui.label(format!(
                            "Resolution: {}x{}",
                            ed.project.resolution.width, ed.project.resolution.height
                        ));
                        ui.label(format!(
                            "Timebase: {:.2} fps Progressive",
                            ed.project.frame_rate.as_f64()
                        ));
                        ui.label(format!(
                            "Sequence Duration: {}",
                            format_smpte(
                                ed.project.timeline.duration_us(),
                                ed.project.frame_rate.as_f64()
                            )
                        ));
                        let active_enc = rd_export::select_encoder("h264");
                        ui.label(format!("Hardware Encoder: {} (Active)", active_enc));
                    });

                    ui.add_space(12.0);
                    if ui
                        .add(
                            crate::ui::widgets::StudioButton::new("Open Render Settings...")
                                .accent(egui::Color32::from_rgb(37, 99, 235))
                                .active(true)
                                .icon(crate::ui::icons::draw_nav_export),
                        )
                        .clicked()
                    {
                        ed.show_export_modal = true;
                    }
                }
            }
        });

    // 4. Central Panel: Video Preview (Program Monitor)
    egui::CentralPanel::default().show(ctx, |ui| {
        // Missing Media Alert Banner
        if !ed.missing_media.is_empty() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "[!] Offline Media: {} file(s) detached",
                        ed.missing_media.len()
                    ))
                    .color(egui::Color32::from_rgb(245, 158, 11))
                    .strong(),
                );
                if studio_btn(
                    ui,
                    "Relink Media Files...",
                    Some(egui::Color32::from_rgb(217, 119, 6)),
                    false,
                )
                .clicked()
                    && let Some(new_file) = rfd::FileDialog::new().pick_file()
                    && let Some(first_missing) = ed.missing_media.first().cloned()
                {
                    let count = ed.project.relink_media(
                        &first_missing.to_string_lossy(),
                        &new_file.to_string_lossy(),
                    );
                    ed.status_message =
                        Some((format!("Relinked {} clip(s)", count), Instant::now()));
                    ed.missing_media = ed.project.find_missing_media();
                }
            });
            ui.separator();
        }

        // Program Monitor Top Bar HUD
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PROGRAM MONITOR")
                    .strong()
                    .size(11.0)
                    .color(egui::Color32::from_rgb(200, 205, 220)),
            );
            ui.label(
                egui::RichText::new("[MASTER OUT]")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(100, 116, 139)),
            );

            // Center status LED
            let (status_text, status_col) = if ed.is_playing {
                ("LIVE 29.97 FPS", egui::Color32::from_rgb(52, 211, 153))
            } else {
                ("PAUSED", egui::Color32::from_rgb(148, 163, 184))
            };
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new(status_text)
                    .monospace()
                    .strong()
                    .size(11.0)
                    .color(status_col),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("1920x1080 16:9 HD")
                        .size(10.5)
                        .color(egui::Color32::from_rgb(140, 145, 160)),
                );
                ui.separator();
                let guide_label = if ed.show_safe_guides {
                    "Guides: ON"
                } else {
                    "Guides: OFF"
                };
                if studio_btn(
                    ui,
                    guide_label,
                    Some(egui::Color32::from_rgb(59, 130, 246)),
                    ed.show_safe_guides,
                )
                .on_hover_text("Toggle Broadcast Safe Area Guidelines (90% Action / 80% Title)")
                .clicked()
                {
                    ed.show_safe_guides = !ed.show_safe_guides;
                }
            });
        });

        let available = ui.available_size();
        let aspect = 16.0 / 9.0;
        let preview_width = (available.x - 20.0)
            .min((available.y - 30.0) * aspect)
            .max(200.0);
        let preview_height = preview_width / aspect;

        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(preview_width, preview_height),
            egui::Sense::click(),
        );

        let painter = ui.painter_at(rect);
        // Video monitor frame background
        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(10, 11, 14));

        // Draw live hardware GPU texture from wgpu
        if let Some(tex_id) = ed.monitor_texture_id {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(tex_id, rect, uv, egui::Color32::WHITE);
        }

        // Precision bezel outline
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(38, 42, 54)),
            egui::StrokeKind::Inside,
        );

        // Draw broadcast safe area guidelines (90% action safe, 80% title safe)
        if ed.show_safe_guides {
            let action_safe = rect.shrink2(egui::vec2(rect.width() * 0.05, rect.height() * 0.05));
            painter.rect_stroke(
                action_safe,
                0.0,
                egui::Stroke::new(
                    0.6_f32,
                    egui::Color32::from_rgba_unmultiplied(70, 85, 110, 120),
                ),
                egui::StrokeKind::Inside,
            );
            let title_safe = rect.shrink2(egui::vec2(rect.width() * 0.10, rect.height() * 0.10));
            painter.rect_stroke(
                title_safe,
                0.0,
                egui::Stroke::new(
                    0.6_f32,
                    egui::Color32::from_rgba_unmultiplied(100, 125, 160, 100),
                ),
                egui::StrokeKind::Inside,
            );
            // Center crosshair
            let center = rect.center();
            painter.line_segment(
                [
                    egui::pos2(center.x - 6.0, center.y),
                    egui::pos2(center.x + 6.0, center.y),
                ],
                egui::Stroke::new(
                    0.75_f32,
                    egui::Color32::from_rgba_unmultiplied(120, 140, 180, 130),
                ),
            );
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y - 6.0),
                    egui::pos2(center.x, center.y + 6.0),
                ],
                egui::Stroke::new(
                    0.75_f32,
                    egui::Color32::from_rgba_unmultiplied(120, 140, 180, 130),
                ),
            );
        }

        // Query active tracks at current playhead for HUD
        let active_tracks = ed
            .project
            .timeline
            .active_tracks_at(ed.project.timeline.playhead_us);
        let active_layer_count = active_tracks.len();

        if active_layer_count > 0 {
            painter.text(
                egui::pos2(rect.left() + 10.0, rect.bottom() - 18.0),
                egui::Align2::LEFT_BOTTOM,
                format!("V1 HARDWARE DECODE ({} ACTIVE)", active_layer_count),
                egui::FontId::monospace(9.5),
                egui::Color32::from_rgb(52, 211, 153),
            );
        } else {
            painter.text(
                egui::pos2(rect.left() + 10.0, rect.bottom() - 18.0),
                egui::Align2::LEFT_BOTTOM,
                "SMPTE BARS (TIMELINE GAP)",
                egui::FontId::monospace(9.5),
                egui::Color32::from_rgb(245, 158, 11),
            );
        }

        // Overlay status badges
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.top() + 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{}x{} | {:.2} fps",
                ed.project.resolution.width,
                ed.project.resolution.height,
                ed.project.frame_rate.as_f64()
            ),
            egui::FontId::monospace(10.0),
            egui::Color32::from_rgb(130, 135, 150),
        );

        painter.text(
            egui::pos2(rect.right() - 10.0, rect.top() + 8.0),
            egui::Align2::RIGHT_TOP,
            "REC.709 COLOR SPACE",
            egui::FontId::monospace(9.5),
            egui::Color32::from_rgb(100, 116, 139),
        );

        // Display current SMPTE timecode in bottom left of monitor
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0),
            egui::Align2::LEFT_BOTTOM,
            format_smpte(
                ed.project.timeline.playhead_us,
                ed.project.frame_rate.as_f64(),
            ),
            egui::FontId::monospace(12.0),
            egui::Color32::from_rgb(220, 225, 235),
        );

        painter.text(
            egui::pos2(rect.right() - 10.0, rect.bottom() - 10.0),
            egui::Align2::RIGHT_BOTTOM,
            "100% FIT",
            egui::FontId::monospace(10.0),
            egui::Color32::from_rgb(100, 116, 139),
        );
    });

    // 5. Bottom Panel: Interactive Timeline
    egui::TopBottomPanel::bottom("timeline_panel")
        .resizable(true)
        .min_height(220.0)
        .default_height(320.0)
        .show(ctx, |ui| {
            // Precision Hardware Transport & Editing Control Deck
            ui.horizontal(|ui| {
                // 1. Hardware Deck Cluster
                if crate::ui::widgets::studio_transport_button(
                    ui,
                    crate::ui::icons::draw_jump_start,
                    28.0,
                    false,
                    None,
                    "First Frame (Home)",
                )
                .clicked()
                {
                    ed.seek_to_time_us(0);
                }
                if crate::ui::widgets::studio_transport_button(
                    ui,
                    crate::ui::icons::draw_shuttle_rev,
                    28.0,
                    false,
                    None,
                    "Step Back 1s (Shift+Left Arrow)",
                )
                .clicked()
                {
                    let target = ed.project.timeline.playhead_us.saturating_sub(1_000_000);
                    ed.seek_to_time_us(target);
                }
                if crate::ui::widgets::studio_transport_button(
                    ui,
                    crate::ui::icons::draw_step_backward,
                    28.0,
                    false,
                    None,
                    "Step Back 1 Frame (Left Arrow)",
                )
                .clicked()
                {
                    let frame_step = (1_000_000.0 / ed.project.frame_rate.as_f64()) as i64;
                    let target = ed.project.timeline.playhead_us.saturating_sub(frame_step);
                    ed.seek_to_time_us(target);
                }

                // Master Play/Pause Hardware Switch
                let is_playing = ed.is_playing;
                let play_col = if is_playing {
                    Some(egui::Color32::from_rgb(220, 38, 38))
                } else {
                    Some(egui::Color32::from_rgb(37, 99, 235))
                };
                if crate::ui::widgets::studio_transport_button(
                    ui,
                    move |p, r, c| {
                        if is_playing {
                            crate::ui::icons::draw_pause(p, r, c);
                        } else {
                            crate::ui::icons::draw_play(p, r, c);
                        }
                    },
                    32.0,
                    is_playing,
                    play_col,
                    "Play / Pause Sequence Playback (Space)",
                )
                .clicked()
                {
                    ed.toggle_playback();
                }

                if crate::ui::widgets::studio_transport_button(
                    ui,
                    crate::ui::icons::draw_step_forward,
                    28.0,
                    false,
                    None,
                    "Step Forward 1 Frame (Right Arrow)",
                )
                .clicked()
                {
                    let frame_step = (1_000_000.0 / ed.project.frame_rate.as_f64()) as i64;
                    let target = ed.project.timeline.playhead_us + frame_step;
                    ed.seek_to_time_us(target);
                }
                if crate::ui::widgets::studio_transport_button(
                    ui,
                    crate::ui::icons::draw_shuttle_fwd,
                    28.0,
                    false,
                    None,
                    "Step Forward 1s (Shift+Right Arrow)",
                )
                .clicked()
                {
                    let target = ed.project.timeline.playhead_us + 1_000_000;
                    ed.seek_to_time_us(target);
                }
                if crate::ui::widgets::studio_transport_button(
                    ui,
                    crate::ui::icons::draw_jump_end,
                    28.0,
                    false,
                    None,
                    "Last Frame (End)",
                )
                .clicked()
                {
                    let target = ed.project.timeline.duration_us();
                    ed.seek_to_time_us(target);
                }

                ui.separator();

                // 2. Dual Digital LCD Displays
                let timecode = format_smpte(
                    ed.project.timeline.playhead_us,
                    ed.project.frame_rate.as_f64(),
                );
                crate::ui::widgets::studio_lcd_box(
                    ui,
                    "PLAYHEAD",
                    &timecode,
                    egui::Color32::from_rgb(56, 189, 248),
                );

                let total_time = format_smpte(
                    ed.project.timeline.duration_us(),
                    ed.project.frame_rate.as_f64(),
                );
                crate::ui::widgets::studio_lcd_box(
                    ui,
                    "SEQUENCE",
                    &total_time,
                    egui::Color32::from_rgb(148, 163, 184),
                );

                ui.separator();

                // 3. Professional NLE Tool Palette
                let active_tool = ed.active_tool;

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Select")
                            .shortcut("V")
                            .active(active_tool == EditTool::Select)
                            .accent(egui::Color32::from_rgb(59, 130, 246))
                            .icon(crate::ui::icons::draw_selection_arrow)
                            .tooltip("Selection & Move Tool (V)"),
                    )
                    .clicked()
                {
                    ed.active_tool = EditTool::Select;
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Razor")
                            .shortcut("C")
                            .active(active_tool == EditTool::Razor)
                            .accent(egui::Color32::from_rgb(217, 119, 6))
                            .icon(crate::ui::icons::draw_razor)
                            .tooltip("Razor Split Tool (C) - Click clip on timeline to split"),
                    )
                    .clicked()
                {
                    ed.active_tool = EditTool::Razor;
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Ripple")
                            .shortcut("B")
                            .active(active_tool == EditTool::RippleTrim)
                            .accent(egui::Color32::from_rgb(168, 85, 247))
                            .icon(|p, r, c| crate::ui::icons::draw_ripple_trim(p, r, true, c))
                            .tooltip("Ripple Trim Tool (B) - Trims clip and ripples downstream sequence"),
                    )
                    .clicked()
                {
                    ed.active_tool = EditTool::RippleTrim;
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Roll")
                            .shortcut("N")
                            .active(active_tool == EditTool::Roll)
                            .accent(egui::Color32::from_rgb(14, 165, 233))
                            .icon(crate::ui::icons::draw_roll_tool)
                            .tooltip("Roll Edit Tool (N) - Adjust cut point between adjacent clips"),
                    )
                    .clicked()
                {
                    ed.active_tool = EditTool::Roll;
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Slip")
                            .shortcut("Y")
                            .active(active_tool == EditTool::Slip)
                            .accent(egui::Color32::from_rgb(16, 185, 129))
                            .icon(crate::ui::icons::draw_slip_tool)
                            .tooltip("Slip Tool (Y) - Shift source in/out while keeping timeline position invariant"),
                    )
                    .clicked()
                {
                    ed.active_tool = EditTool::Slip;
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Slide")
                            .shortcut("U")
                            .active(active_tool == EditTool::Slide)
                            .accent(egui::Color32::from_rgb(234, 179, 8))
                            .icon(crate::ui::icons::draw_slide_tool)
                            .tooltip("Slide Tool (U) - Move clip along timeline while adjusting adjacent trims"),
                    )
                    .clicked()
                {
                    ed.active_tool = EditTool::Slide;
                }

                ui.separator();

                // Work Area Range & Markers: [I] Mark In, [O] Mark Out, [X] Clear, [M] Cue Marker
                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("In")
                            .shortcut("I")
                            .icon(crate::ui::icons::draw_mark_in)
                            .tooltip("Set Work Area In-Point (I)"),
                    )
                    .clicked()
                {
                    let playhead = ed.project.timeline.playhead_us;
                    ed.project.timeline.set_mark_in(playhead);
                    ed.record_mutation(rd_core::JournalMutation::SetMarkInOut {
                        mark_in_us: ed.project.timeline.mark_in_us,
                        mark_out_us: ed.project.timeline.mark_out_us,
                    });
                    ed.status_message = Some(("Work Area: Mark In Set [I]".into(), Instant::now()));
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Out")
                            .shortcut("O")
                            .icon(crate::ui::icons::draw_mark_out)
                            .tooltip("Set Work Area Out-Point (O)"),
                    )
                    .clicked()
                {
                    let playhead = ed.project.timeline.playhead_us;
                    ed.project.timeline.set_mark_out(playhead);
                    ed.record_mutation(rd_core::JournalMutation::SetMarkInOut {
                        mark_in_us: ed.project.timeline.mark_in_us,
                        mark_out_us: ed.project.timeline.mark_out_us,
                    });
                    ed.status_message = Some(("Work Area: Mark Out Set [O]".into(), Instant::now()));
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Clear")
                            .shortcut("X")
                            .tooltip("Clear Work Area In/Out Range (X)"),
                    )
                    .clicked()
                {
                    ed.project.timeline.clear_in_out();
                    ed.record_mutation(rd_core::JournalMutation::SetMarkInOut {
                        mark_in_us: None,
                        mark_out_us: None,
                    });
                    ed.status_message = Some(("Work Area: In/Out Cleared [X]".into(), Instant::now()));
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Marker")
                            .shortcut("M")
                            .accent(egui::Color32::from_rgb(59, 130, 246))
                            .icon(crate::ui::icons::draw_marker)
                            .tooltip("Place Cue Marker at Playhead (M)"),
                    )
                    .clicked()
                {
                    let playhead = ed.project.timeline.playhead_us;
                    let mid = ed.project.timeline.add_marker(playhead, "Cue", [59, 130, 246]);
                    ed.record_mutation(rd_core::JournalMutation::AddMarker {
                        id: mid,
                        time_us: playhead,
                        name: "Cue".into(),
                        color_rgb: [59, 130, 246],
                    });
                    ed.status_message = Some((format!("Marker #{} Placed at Playhead [M]", mid), Instant::now()));
                }

                ui.separator();

                // Quick Edit Action Tools: Duplicate (D) and Ripple Delete (Shift+Del)
                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Dup")
                            .shortcut("D")
                            .tooltip("Duplicate Selected Clip [D]"),
                    )
                    .clicked()
                    && let Some(cid) = ed.selected_clip
                    && let Some(tid) = ed.selected_track
                {
                    let maybe_dup = if let Some(track) = ed.project.timeline.track_mut(tid) {
                        track
                            .duplicate_clip(cid)
                            .and_then(|new_id| track.clip(new_id).cloned().map(|c| (new_id, c)))
                    } else {
                        None
                    };
                    if let Some((new_id, new_clip)) = maybe_dup {
                        ed.record_mutation(rd_core::JournalMutation::AddClip {
                            track_id: tid,
                            clip: new_clip,
                        });
                        ed.selected_clip = Some(new_id);
                        ed.status_message = Some(("Duplicated Selected Clip [D]".into(), Instant::now()));
                    }
                }

                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("RipDel")
                            .shortcut("Shift+Del")
                            .accent(egui::Color32::from_rgb(220, 38, 38))
                            .tooltip("Ripple Delete Selected Clip & Close Gap (Shift+Del)"),
                    )
                    .clicked()
                    && let Some(cid) = ed.selected_clip.take()
                {
                    let mut ripple_tid = None;
                    for track in ed.project.timeline.tracks_mut() {
                        if track.ripple_delete(cid).is_some() {
                            ripple_tid = Some(track.id);
                            break;
                        }
                    }
                    if let Some(tid) = ripple_tid {
                        ed.record_mutation(rd_core::JournalMutation::RippleDelete {
                            track_id: tid,
                            clip_id: cid,
                        });
                        ed.status_message = Some(("Ripple Deleted Clip [Shift+Del]".into(), Instant::now()));
                    }
                }

                // Magnet Snapping Toggle
                let snapping = ed.snapping_enabled;
                let snap_title = if snapping { "Snap: ON" } else { "Snap: OFF" };
                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new(snap_title)
                            .shortcut("S")
                            .accent(egui::Color32::from_rgb(59, 130, 246))
                            .active(snapping)
                            .icon(move |p, r, c| {
                                crate::ui::icons::draw_snapping_magnet(p, r, snapping, c);
                            })
                            .tooltip("Toggle Timeline Magnet Snapping (S)"),
                    )
                    .clicked()
                {
                    ed.snapping_enabled = !ed.snapping_enabled;
                }

                // Zoom controls
                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Zoom -")
                            .tooltip("Zoom Out Timeline"),
                    )
                    .clicked()
                {
                    ed.zoom_px_per_sec = (ed.zoom_px_per_sec / 1.25).max(10.0);
                }
                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Zoom +").tooltip("Zoom In Timeline"),
                    )
                    .clicked()
                {
                    ed.zoom_px_per_sec = (ed.zoom_px_per_sec * 1.25).min(500.0);
                }
                if ui
                    .add(
                        crate::ui::widgets::StudioButton::new("Fit")
                            .shortcut("Shift+Z")
                            .tooltip("Fit Entire Sequence into Timeline Window"),
                    )
                    .clicked()
                {
                    let dur_s = (ed.project.timeline.duration_us() as f32 / 1_000_000.0).max(1.0);
                    ed.zoom_px_per_sec = (800.0 / dur_s).clamp(10.0, 300.0);
                }

                // 4. Right side: Master Audio Meter & Track Adders
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        crate::ui::widgets::StudioAudioMeter::new(ed.audio_metering)
                            .height(24.0)
                            .show_labels(true),
                    );
                    ui.separator();

                    if studio_btn(
                        ui,
                        "+ Text Track",
                        Some(egui::Color32::from_rgb(217, 119, 6)),
                        false,
                    )
                    .on_hover_text("Append a new subtitle / overlay text lane")
                    .clicked()
                    {
                        let count = ed.project.timeline.track_count() + 1;
                        ed.project
                            .timeline
                            .add_track(TrackKind::Text, format!("Text {}", count));
                    }
                    if studio_btn(
                        ui,
                        "+ Audio Track",
                        Some(egui::Color32::from_rgb(16, 185, 129)),
                        false,
                    )
                    .on_hover_text("Append a new stereo audio lane")
                    .clicked()
                    {
                        let count = ed.project.timeline.track_count() + 1;
                        ed.project
                            .timeline
                            .add_track(TrackKind::Audio, format!("Audio {}", count));
                    }
                    if studio_btn(
                        ui,
                        "+ Video Track",
                        Some(egui::Color32::from_rgb(59, 130, 246)),
                        false,
                    )
                    .on_hover_text("Append a new video composite layer")
                    .clicked()
                    {
                        let count = ed.project.timeline.track_count() + 1;
                        ed.project
                            .timeline
                            .add_track(TrackKind::Video, format!("Video {}", count));
                    }
                });
            });

            ui.separator();

            // Timeline Canvas Area
            let available = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);

            // Timeline Background
            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(18, 18, 22));

            let track_header_width = 150.0;
            let timeline_x_start = rect.left() + track_header_width;
            let timeline_width = rect.width() - track_header_width;

            // Handle scrubbing clicks & drags with intelligent snapping
            if (response.clicked() || response.dragged())
                && let Some(pos) = response.interact_pointer_pos()
                && pos.x >= timeline_x_start
            {
                let rel_x = (pos.x - timeline_x_start).max(0.0);
                let raw_time_us = (rel_x / ed.zoom_px_per_sec * 1_000_000.0) as i64;
                let threshold_us = ((12.0 / ed.zoom_px_per_sec) * 1_000_000.0) as i64;
                let target_us = if ed.snapping_enabled {
                    ed.project.timeline.snap_to(raw_time_us, threshold_us)
                } else {
                    raw_time_us
                };
                ed.project.timeline.playhead_us = target_us.max(0);
            }

            // Draw Time Ruler (ticks every 1s / 5s)
            let ruler_height = 24.0;
            let ruler_rect = egui::Rect::from_min_size(
                egui::pos2(timeline_x_start, rect.top()),
                egui::vec2(timeline_width, ruler_height),
            );
            painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(25, 25, 32));

            let mut sec = 0.0;
            while sec * ed.zoom_px_per_sec < timeline_width {
                let x = timeline_x_start + sec * ed.zoom_px_per_sec;
                let is_major = (sec as i64) % 5 == 0;
                let tick_h = if is_major { 12.0 } else { 6.0 };

                painter.line_segment(
                    [
                        egui::pos2(x, ruler_rect.bottom() - tick_h),
                        egui::pos2(x, ruler_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(90, 90, 105)),
                );

                if is_major {
                    painter.text(
                        egui::pos2(x + 2.0, ruler_rect.top() + 2.0),
                        egui::Align2::LEFT_TOP,
                        format!("{:.0}s", sec),
                        egui::FontId::monospace(10.0),
                        egui::Color32::from_rgb(150, 150, 165),
                    );
                }
                sec += 1.0;
            }

            // 1. Draw In / Out Work Area Range on Ruler
            if ed.project.timeline.mark_in_us.is_some() || ed.project.timeline.mark_out_us.is_some() {
                let mark_in = ed.project.timeline.mark_in_us.unwrap_or(0);
                let mark_out = ed
                    .project
                    .timeline
                    .mark_out_us
                    .unwrap_or_else(|| ed.project.timeline.duration_us());
                let in_x = timeline_x_start + (mark_in as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
                let out_x = timeline_x_start + (mark_out as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
                let in_clamped = in_x.max(timeline_x_start);
                let out_clamped = out_x.min(rect.right());
                if out_clamped > in_clamped {
                    let work_area = egui::Rect::from_min_max(
                        egui::pos2(in_clamped, ruler_rect.top()),
                        egui::pos2(out_clamped, ruler_rect.bottom()),
                    );
                    painter.rect_filled(
                        work_area,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(56, 189, 248, 45),
                    );
                }
                // In-point bracket line & flags
                if in_x >= timeline_x_start && in_x <= rect.right() {
                    painter.line_segment(
                        [
                            egui::pos2(in_x, ruler_rect.top()),
                            egui::pos2(in_x, ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(56, 189, 248)),
                    );
                    painter.line_segment(
                        [
                            egui::pos2(in_x, ruler_rect.top()),
                            egui::pos2(in_x + 5.0, ruler_rect.top()),
                        ],
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(56, 189, 248)),
                    );
                    painter.line_segment(
                        [
                            egui::pos2(in_x, ruler_rect.bottom()),
                            egui::pos2(in_x + 5.0, ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(56, 189, 248)),
                    );
                }
                // Out-point bracket line & flags
                if out_x >= timeline_x_start && out_x <= rect.right() {
                    painter.line_segment(
                        [
                            egui::pos2(out_x, ruler_rect.top()),
                            egui::pos2(out_x, ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(56, 189, 248)),
                    );
                    painter.line_segment(
                        [
                            egui::pos2(out_x, ruler_rect.top()),
                            egui::pos2(out_x - 5.0, ruler_rect.top()),
                        ],
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(56, 189, 248)),
                    );
                    painter.line_segment(
                        [
                            egui::pos2(out_x, ruler_rect.bottom()),
                            egui::pos2(out_x - 5.0, ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(56, 189, 248)),
                    );
                }
            }

            // 2. Draw Marker Diamonds on Ruler & Guidelines down timeline
            for m in &ed.project.timeline.markers {
                let mx = timeline_x_start + (m.time_us as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
                if mx >= timeline_x_start && mx <= rect.right() {
                    let m_color = egui::Color32::from_rgb(m.color_rgb[0], m.color_rgb[1], m.color_rgb[2]);
                    let m_rect = egui::Rect::from_center_size(
                        egui::pos2(mx, ruler_rect.center().y),
                        egui::vec2(10.0, 10.0),
                    );
                    crate::ui::icons::draw_marker(&painter, m_rect, m_color);
                    painter.line_segment(
                        [
                            egui::pos2(mx, ruler_rect.bottom()),
                            egui::pos2(mx, rect.bottom()),
                        ],
                        egui::Stroke::new(
                            1.0_f32,
                            egui::Color32::from_rgba_unmultiplied(m.color_rgb[0], m.color_rgb[1], m.color_rgb[2], 80),
                        ),
                    );
                }
            }

            // Contextual studio cursor feedback
            if response.hovered() {
                match ed.active_tool {
                    EditTool::Select => ctx.set_cursor_icon(egui::CursorIcon::Default),
                    EditTool::Razor => ctx.set_cursor_icon(egui::CursorIcon::Crosshair),
                    EditTool::RippleTrim => ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal),
                    EditTool::Roll => ctx.set_cursor_icon(egui::CursorIcon::ResizeColumn),
                    EditTool::Slip | EditTool::Slide => ctx.set_cursor_icon(egui::CursorIcon::Grab),
                }
            }

            // Draw Track Lanes and Clips
            let track_lane_height = 44.0;
            let mut y = rect.top() + ruler_height + 4.0;

            enum PendingClipOp {
                Select(ClipId, TrackId),
                Razor(ClipId, TrackId, i64),
                RippleTrim(ClipId, TrackId, bool),
                Roll(ClipId, TrackId),
                Slip(ClipId, TrackId),
                Slide(ClipId, TrackId),
            }

            let mut pending_clip_op = None;
            let mut clicked_track = None;
            let mut toggled_mute_track = None;
            let mut toggled_solo_track = None;
            let mut toggled_lock_track = None;

            for track in ed.project.timeline.tracks() {
                // Header lane
                let header_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(track_header_width, track_lane_height),
                );
                let is_track_selected = ed.selected_track == Some(track.id);
                let header_bg = if is_track_selected {
                    egui::Color32::from_rgb(32, 36, 48)
                } else {
                    egui::Color32::from_rgb(24, 26, 33)
                };
                painter.rect_filled(header_rect, 2.0, header_bg);
                painter.rect_stroke(
                    header_rect,
                    2.0,
                    egui::Stroke::new(
                        1.0_f32,
                        if is_track_selected {
                            egui::Color32::from_rgb(59, 130, 246)
                        } else {
                            egui::Color32::from_rgb(38, 42, 54)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );

                // Track Identity Badge (V1, A1, T1)
                let (badge_letter, badge_color) = match track.kind {
                    TrackKind::Video => ("V", egui::Color32::from_rgb(37, 99, 235)),
                    TrackKind::Audio => ("A", egui::Color32::from_rgb(16, 185, 129)),
                    TrackKind::Text => ("T", egui::Color32::from_rgb(217, 119, 6)),
                    TrackKind::Overlay => ("O", egui::Color32::from_rgb(168, 85, 247)),
                };
                let badge_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 6.0, y + 11.0),
                    egui::vec2(20.0, 22.0),
                );
                painter.rect_filled(badge_rect, 2.0, badge_color);
                painter.text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    badge_letter,
                    egui::FontId::monospace(11.0),
                    egui::Color32::WHITE,
                );

                // Track Name Label
                painter.text(
                    egui::pos2(rect.left() + 30.0, y + 16.0),
                    egui::Align2::LEFT_CENTER,
                    &track.name,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(215, 220, 230),
                );

                // Track header controls: [M] Mute, [S] Solo, [L] Lock
                let mute_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + track_header_width - 66.0, y + 12.0),
                    egui::vec2(19.0, 20.0),
                );
                let solo_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + track_header_width - 44.0, y + 12.0),
                    egui::vec2(19.0, 20.0),
                );
                let lock_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + track_header_width - 22.0, y + 12.0),
                    egui::vec2(19.0, 20.0),
                );

                let is_muted = !track.enabled;
                let is_solo = ed.solo_track == Some(track.id);
                let is_locked = track.locked;

                studio_pill_btn(
                    &painter,
                    mute_rect,
                    "M",
                    is_muted,
                    egui::Color32::from_rgb(220, 38, 38),
                    egui::Color32::WHITE,
                );
                studio_pill_btn(
                    &painter,
                    solo_rect,
                    "S",
                    is_solo,
                    egui::Color32::from_rgb(234, 179, 8),
                    egui::Color32::BLACK,
                );
                studio_pill_btn(
                    &painter,
                    lock_rect,
                    "L",
                    is_locked,
                    egui::Color32::from_rgb(249, 115, 22),
                    egui::Color32::WHITE,
                );

                if response.clicked()
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    if mute_rect.contains(pos) {
                        toggled_mute_track = Some(track.id);
                    } else if solo_rect.contains(pos) {
                        toggled_solo_track = Some(track.id);
                    } else if lock_rect.contains(pos) {
                        toggled_lock_track = Some(track.id);
                    } else if header_rect.contains(pos) {
                        clicked_track = Some(track.id);
                    }
                }

                // Track content background
                let content_rect = egui::Rect::from_min_size(
                    egui::pos2(timeline_x_start, y),
                    egui::vec2(timeline_width, track_lane_height),
                );
                let bg_color = match track.kind {
                    TrackKind::Video => egui::Color32::from_rgb(20, 26, 36),
                    TrackKind::Audio => egui::Color32::from_rgb(20, 34, 24),
                    TrackKind::Text => egui::Color32::from_rgb(34, 26, 20),
                    TrackKind::Overlay => egui::Color32::from_rgb(30, 22, 34),
                };
                painter.rect_filled(content_rect, 2.0, bg_color);

                // Render Clips
                for clip in track.clips() {
                    let clip_start_x = timeline_x_start
                        + (clip.position_us as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
                    let clip_w =
                        ((clip.duration_us() as f32 / 1_000_000.0) * ed.zoom_px_per_sec).max(6.0);
                    let clip_rect = egui::Rect::from_min_size(
                        egui::pos2(clip_start_x, y + 2.0),
                        egui::vec2(clip_w, track_lane_height - 4.0),
                    );

                    let is_selected = ed.selected_clip == Some(clip.id);
                    let (clip_color, top_accent) = if is_selected {
                        (
                            egui::Color32::from_rgb(37, 99, 235),
                            egui::Color32::from_rgb(96, 165, 250),
                        )
                    } else {
                        match track.kind {
                            TrackKind::Video => (
                                egui::Color32::from_rgb(32, 44, 62),
                                egui::Color32::from_rgb(59, 130, 246),
                            ),
                            TrackKind::Audio => (
                                egui::Color32::from_rgb(26, 48, 36),
                                egui::Color32::from_rgb(16, 185, 129),
                            ),
                            TrackKind::Text => (
                                egui::Color32::from_rgb(52, 40, 26),
                                egui::Color32::from_rgb(245, 158, 11),
                            ),
                            TrackKind::Overlay => (
                                egui::Color32::from_rgb(46, 30, 50),
                                egui::Color32::from_rgb(168, 85, 247),
                            ),
                        }
                    };

                    painter.rect_filled(clip_rect, 2.0, clip_color);
                    // Draw clean 2px top color strip indicating track media class
                    let accent_rect = egui::Rect::from_min_size(
                        clip_rect.left_top(),
                        egui::vec2(clip_rect.width(), 2.0),
                    );
                    painter.rect_filled(accent_rect, 0.0, top_accent);

                    if is_selected {
                        painter.rect_stroke(
                            clip_rect,
                            2.0,
                            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(147, 197, 253)),
                            egui::StrokeKind::Inside,
                        );
                    }

                    // Clip duration label formatted in SMPTE or seconds
                    let label = format_smpte(clip.duration_us(), ed.project.frame_rate.as_f64());
                    painter.text(
                        egui::pos2(clip_start_x + 6.0, y + track_lane_height / 2.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::monospace(10.0),
                        egui::Color32::from_rgb(220, 225, 235),
                    );

                    // Check click on clip based on active studio tool
                    if response.clicked()
                        && let Some(pos) = response.interact_pointer_pos()
                        && clip_rect.contains(pos)
                    {
                        let click_time_us = (clip.position_us as f64
                            + ((pos.x - clip_rect.left()) / ed.zoom_px_per_sec * 1_000_000.0) as f64)
                            as i64;
                        match ed.active_tool {
                            EditTool::Select => {
                                pending_clip_op = Some(PendingClipOp::Select(clip.id, track.id));
                            }
                            EditTool::Razor => {
                                pending_clip_op =
                                    Some(PendingClipOp::Razor(clip.id, track.id, click_time_us));
                            }
                            EditTool::RippleTrim => {
                                let is_head = (pos.x - clip_rect.left()) < (clip_rect.right() - pos.x);
                                pending_clip_op =
                                    Some(PendingClipOp::RippleTrim(clip.id, track.id, is_head));
                            }
                            EditTool::Roll => {
                                pending_clip_op = Some(PendingClipOp::Roll(clip.id, track.id));
                            }
                            EditTool::Slip => {
                                pending_clip_op = Some(PendingClipOp::Slip(clip.id, track.id));
                            }
                            EditTool::Slide => {
                                pending_clip_op = Some(PendingClipOp::Slide(clip.id, track.id));
                            }
                        }
                    }
                }

                y += track_lane_height + 4.0;
            }

            if let Some(tid) = toggled_mute_track
                && let Some(t) = ed.project.timeline.track_mut(tid)
            {
                t.enabled = !t.enabled;
                let st = if t.enabled { "Unmuted" } else { "Muted" };
                ed.status_message = Some((format!("{} {}", t.name, st), Instant::now()));
            }
            if let Some(tid) = toggled_solo_track {
                if ed.solo_track == Some(tid) {
                    ed.solo_track = None;
                    ed.status_message = Some(("Track solo disabled".into(), Instant::now()));
                } else {
                    ed.solo_track = Some(tid);
                    ed.status_message = Some(("Track solo activated".into(), Instant::now()));
                }
            }
            if let Some(tid) = toggled_lock_track
                && let Some(t) = ed.project.timeline.track_mut(tid)
            {
                t.locked = !t.locked;
                let st = if t.locked { "Locked" } else { "Unlocked" };
                ed.status_message = Some((format!("{} {}", t.name, st), Instant::now()));
            }

            if let Some(tid) = clicked_track {
                ed.selected_track = Some(tid);
            }

            // Execute pending clip operations outside the immutable tracks borrow
            if let Some(op) = pending_clip_op {
                let frame_step = (1_000_000.0 / ed.project.frame_rate.as_f64()) as i64;
                match op {
                    PendingClipOp::Select(cid, tid) => {
                        ed.selected_clip = Some(cid);
                        ed.selected_track = Some(tid);
                    }
                    PendingClipOp::Razor(cid, tid, split_time) => {
                        let maybe_split = if let Some(t) = ed.project.timeline.track_mut(tid) {
                            t.split_clip_at(cid, split_time)
                                .and_then(|new_cid| t.clip(new_cid).cloned())
                        } else {
                            None
                        };
                        if let Some(new_clip) = maybe_split {
                            ed.record_mutation(rd_core::JournalMutation::SplitClip {
                                track_id: tid,
                                clip_id: cid,
                                split_time_us: split_time,
                                new_clip,
                            });
                            ed.status_message =
                                Some(("Split Clip with Razor Tool [C]".into(), Instant::now()));
                        }
                    }
                    PendingClipOp::RippleTrim(cid, tid, is_head) => {
                        let res = if let Some(t) = ed.project.timeline.track_mut(tid) {
                            if is_head {
                                t.ripple_trim_in(cid, frame_step).map(|_| true)
                            } else {
                                t.ripple_trim_out(cid, -frame_step).map(|_| false)
                            }
                        } else {
                            Err("No track")
                        };
                        if let Ok(head) = res {
                            if head {
                                ed.record_mutation(rd_core::JournalMutation::RippleTrimIn {
                                    track_id: tid,
                                    clip_id: cid,
                                    delta_us: frame_step,
                                });
                                ed.status_message = Some((
                                    "Ripple Trimmed In-Point (+1 frame) [B]".into(),
                                    Instant::now(),
                                ));
                            } else {
                                ed.record_mutation(rd_core::JournalMutation::RippleTrimOut {
                                    track_id: tid,
                                    clip_id: cid,
                                    delta_us: -frame_step,
                                });
                                ed.status_message = Some((
                                    "Ripple Trimmed Out-Point (-1 frame) [B]".into(),
                                    Instant::now(),
                                ));
                            }
                        }
                    }
                    PendingClipOp::Roll(cid, tid) => {
                        let res = if let Some(t) = ed.project.timeline.track_mut(tid) {
                            if let Some(pos_idx) = t.clips().iter().position(|c| c.id == cid) {
                                if pos_idx + 1 < t.clips().len() {
                                    let next_id = t.clips()[pos_idx + 1].id;
                                    t.roll_edit(cid, next_id, frame_step).map(|_| (cid, next_id))
                                } else if pos_idx > 0 {
                                    let prev_id = t.clips()[pos_idx - 1].id;
                                    t.roll_edit(prev_id, cid, frame_step).map(|_| (prev_id, cid))
                                } else {
                                    Err("No adjacent clip to roll")
                                }
                            } else {
                                Err("Clip not found")
                            }
                        } else {
                            Err("Track not found")
                        };
                        if let Ok((outgoing_id, incoming_id)) = res {
                            ed.record_mutation(rd_core::JournalMutation::RollEdit {
                                track_id: tid,
                                outgoing_id,
                                incoming_id,
                                delta_us: frame_step,
                            });
                            ed.status_message = Some((
                                "Rolled Edit Boundary (+1 frame) [N]".into(),
                                Instant::now(),
                            ));
                        }
                    }
                    PendingClipOp::Slip(cid, tid) => {
                        let ok = if let Some(t) = ed.project.timeline.track_mut(tid) {
                            t.slip_edit(cid, frame_step).is_ok()
                        } else {
                            false
                        };
                        if ok {
                            ed.record_mutation(rd_core::JournalMutation::SlipEdit {
                                track_id: tid,
                                clip_id: cid,
                                delta_us: frame_step,
                            });
                            ed.status_message = Some((
                                "Slipped Media Source (+1 frame) [Y]".into(),
                                Instant::now(),
                            ));
                        }
                    }
                    PendingClipOp::Slide(cid, tid) => {
                        let ok = if let Some(t) = ed.project.timeline.track_mut(tid) {
                            t.slide_edit(cid, frame_step).is_ok()
                        } else {
                            false
                        };
                        if ok {
                            ed.record_mutation(rd_core::JournalMutation::SlideEdit {
                                track_id: tid,
                                clip_id: cid,
                                delta_us: frame_step,
                            });
                            ed.status_message = Some((
                                "Slid Clip on Timeline (+1 frame) [U]".into(),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }

            // Playhead Vertical Line
            let playhead_x = timeline_x_start
                + (ed.project.timeline.playhead_us as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
            if playhead_x >= timeline_x_start && playhead_x <= rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(playhead_x, rect.top()),
                        egui::pos2(playhead_x, rect.bottom()),
                    ],
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(239, 68, 68)),
                );
                // Playhead head handle
                let head_rect = egui::Rect::from_min_size(
                    egui::pos2(playhead_x - 5.0, rect.top()),
                    egui::vec2(10.0, 12.0),
                );
                painter.rect_filled(head_rect, 1.0, egui::Color32::from_rgb(239, 68, 68));
            }
        });

    // 6. Export Modal Window
    if ed.show_export_modal {
        egui::Window::new("Export Media")
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("RENDER & ENCODE SETTINGS")
                        .strong()
                        .size(12.0),
                );
                ui.separator();

                let presets = [
                    rd_export::ExportPreset::youtube_1080p(),
                    rd_export::ExportPreset::vertical_9_16_1080p(),
                    rd_export::ExportPreset::uhd_4k(),
                    rd_export::ExportPreset::instagram_square(),
                ];

                ui.label("Format Preset:");
                let current_preset_name = presets[ed.export_preset_idx].name.clone();
                egui::ComboBox::from_id_salt("export_preset_combo")
                    .selected_text(current_preset_name)
                    .show_ui(ui, |ui| {
                        for (idx, p) in presets.iter().enumerate() {
                            ui.selectable_value(&mut ed.export_preset_idx, idx, &p.name);
                        }
                    });

                let selected_preset = &presets[ed.export_preset_idx];
                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.label(format!(
                        "Resolution: {}x{}",
                        selected_preset.width, selected_preset.height
                    ));
                    ui.label(format!(
                        "Timebase: {:.2} fps Progressive",
                        selected_preset.fps
                    ));
                    ui.label(format!(
                        "Video Bitrate: {} kbps ({})",
                        selected_preset.video_bitrate_kbps, selected_preset.video_codec
                    ));
                    ui.label(format!(
                        "Audio Bitrate: {} kbps ({})",
                        selected_preset.audio_bitrate_kbps, selected_preset.audio_codec
                    ));
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            crate::ui::widgets::StudioButton::new("Browse Destination...")
                                .icon(crate::ui::icons::draw_folder),
                        )
                        .clicked()
                    {
                        let default_name = format!(
                            "{}_{}.mp4",
                            ed.project.name.replace(' ', "_"),
                            selected_preset.width
                        );
                        if let Some(dest) = rfd::FileDialog::new()
                            .set_file_name(&default_name)
                            .add_filter("MP4 Video", &["mp4"])
                            .save_file()
                        {
                            ed.export_destination = Some(dest);
                        }
                    }
                    if let Some(dest) = &ed.export_destination {
                        ui.label(
                            egui::RichText::new(
                                dest.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                            )
                            .monospace()
                            .color(egui::Color32::from_rgb(147, 197, 253)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("No file selected")
                                .color(egui::Color32::from_rgb(130, 130, 140)),
                        );
                    }
                });

                ui.separator();

                // Active export progress
                let mut should_finish = false;
                let mut should_cancel = false;

                if let Some(handle) = &mut ed.active_export {
                    // Drain all pending progress messages from background rendering worker
                    while let Ok(progress) = handle.progress_rx.try_recv() {
                        ed.export_progress = Some(progress);
                    }

                    if let Some(progress) = &ed.export_progress {
                        ui.label(format!(
                            "Encoding Frame {} of {} ({:.1}%)",
                            progress.current_frame,
                            progress.total_frames,
                            progress.fraction * 100.0
                        ));
                        ui.add(egui::ProgressBar::new(progress.fraction).show_percentage());
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "Encoder Speed: {:.1}x real-time",
                                progress.speed_multiplier
                            ));
                            ui.label(format!("ETA: {:.1}s", progress.eta_seconds));
                        });
                    } else {
                        ui.label("Initializing hardware pipeline & mixing audio...");
                        ui.add(egui::ProgressBar::new(0.0).animate(true));
                    }

                    if handle.is_finished() {
                        should_finish = true;
                    }

                    if ui
                        .add(
                            crate::ui::widgets::StudioButton::new("Cancel Render")
                                .accent(egui::Color32::from_rgb(220, 38, 38))
                                .icon(crate::ui::icons::draw_close),
                        )
                        .clicked()
                    {
                        handle.cancel();
                        should_cancel = true;
                    }
                }

                if should_finish {
                    if let Some(handle) = ed.active_export.take() {
                        match handle.join() {
                            Ok(()) => {
                                ed.export_completed = true;
                                ed.status_message = Some((
                                    "Project Export Rendered Successfully".into(),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                ed.export_completed = false;
                                ed.status_message =
                                    Some((format!("Export Render Failed: {}", e), Instant::now()));
                            }
                        }
                    }
                } else if should_cancel {
                    if let Some(handle) = ed.active_export.take() {
                        let _ = handle.join();
                    }
                    ed.status_message = Some(("Export Render Cancelled".into(), Instant::now()));
                } else if ed.export_completed {
                    ui.label(
                        egui::RichText::new("Export completed successfully.")
                            .color(egui::Color32::from_rgb(52, 211, 153))
                            .strong(),
                    );
                    if let Some(dest) = &ed.export_destination {
                        ui.label(
                            egui::RichText::new(format!("Saved to: {}", dest.display()))
                                .size(11.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                    if studio_btn(ui, "Close", None, false).clicked() {
                        ed.show_export_modal = false;
                        ed.export_completed = false;
                        ed.export_progress = None;
                    }
                } else {
                    ui.horizontal(|ui| {
                        let can_export = ed.export_destination.is_some()
                            && ed.project.timeline.duration_us() > 0;
                        if can_export {
                            if ui
                                .add(
                                    crate::ui::widgets::StudioButton::new("Start Render")
                                        .accent(egui::Color32::from_rgb(37, 99, 235))
                                        .active(true)
                                        .icon(crate::ui::icons::draw_play),
                                )
                                .clicked()
                                && let Some(dest) = ed.export_destination.clone()
                            {
                                let grade = rd_export::ColorGradeSettings {
                                    exposure: ed.color_exposure,
                                    contrast: ed.color_contrast,
                                    saturation: ed.color_saturation,
                                };
                                ed.active_export = Some(rd_export::start_export_project(
                                    ed.project.clone(),
                                    dest,
                                    selected_preset.clone(),
                                    grade,
                                ));
                                ed.export_progress = None;
                                ed.export_completed = false;
                            }
                        } else {
                            let _ = ui.add(
                                crate::ui::widgets::StudioButton::new("Start Render (Select File)")
                                    .disabled(true)
                                    .icon(crate::ui::icons::draw_play),
                            );
                        }
                        if studio_btn(ui, "Close", None, false).clicked() {
                            ed.show_export_modal = false;
                        }
                    });
                }
            });
    }

    // 7. Starter Templates Modal Window
    if ed.show_template_modal {
        let mut close_modal = false;
        let mut instantiate_idx: Option<usize> = None;
        let templates = rd_effects::template::TemplateSpec::starter_templates();

        egui::Window::new("Starter Project Templates")
            .collapsible(false)
            .resizable(false)
            .default_width(740.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    crate::ui::icons::draw_template(ui.painter(), icon_rect, egui::Color32::from_rgb(0, 229, 255));
                    ui.label(
                        egui::RichText::new("STARTER PROJECT TEMPLATES & SEQUENCES")
                            .strong()
                            .size(13.0)
                            .color(egui::Color32::from_rgb(220, 225, 235)),
                    );
                });
                ui.label(
                    egui::RichText::new("Choose a production-ready template to preconfigure timeline tracks, duration, aspect ratios, and beat sync points.")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(140, 145, 160)),
                );
                ui.separator();

                ui.columns(2, |cols| {
                    // Left Column: Template Cards List
                    cols[0].label(
                        egui::RichText::new("AVAILABLE TEMPLATES")
                            .strong()
                            .size(10.5)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(340.0)
                        .show(&mut cols[0], |ui| {
                            for (idx, tmpl) in templates.iter().enumerate() {
                                let is_selected = ed.selected_template_idx == idx;
                                let card_bg = if is_selected {
                                    egui::Color32::from_rgb(30, 41, 59)
                                } else {
                                    egui::Color32::from_rgb(24, 26, 32)
                                };
                                let card_stroke = if is_selected {
                                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0, 229, 255))
                                } else {
                                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 48, 58))
                                };

                                egui::Frame::new()
                                    .fill(card_bg)
                                    .stroke(card_stroke)
                                    .corner_radius(4.0)
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        let badge_text = if tmpl.aspect_width < tmpl.aspect_height {
                                            "[9:16 VERTICAL]"
                                        } else if tmpl.aspect_width == tmpl.aspect_height {
                                            "[1:1 SQUARE]"
                                        } else if tmpl.aspect_width >= 3840 {
                                            "[4K UHD WIDE]"
                                        } else {
                                            "[16:9 WIDESCREEN]"
                                        };

                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(badge_text)
                                                    .monospace()
                                                    .size(9.5)
                                                    .color(egui::Color32::from_rgb(0, 229, 255)),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{:.1}s | {} slots",
                                                    tmpl.duration_us as f64 / 1_000_000.0,
                                                    tmpl.slots.len()
                                                ))
                                                .size(9.5)
                                                .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                        });

                                        if ui
                                            .selectable_label(
                                                is_selected,
                                                egui::RichText::new(&tmpl.name)
                                                    .strong()
                                                    .size(11.5)
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .clicked()
                                        {
                                            ed.selected_template_idx = idx;
                                            ed.template_project_name = format!("New {}", tmpl.name);
                                        }

                                        ui.label(
                                            egui::RichText::new(&tmpl.description)
                                                .size(10.0)
                                                .color(egui::Color32::from_rgb(140, 145, 160)),
                                        );
                                    });
                                ui.add_space(4.0);
                            }
                        });

                    // Right Column: Template Inspector & Setup
                    let selected_tmpl = &templates[ed.selected_template_idx.min(templates.len() - 1)];
                    cols[1].label(
                        egui::RichText::new("CONFIGURATION & DETAILS")
                            .strong()
                            .size(10.5)
                            .color(egui::Color32::from_rgb(180, 185, 200)),
                    );

                    cols[1].horizontal(|ui| {
                        ui.label("Project Name:");
                        ui.text_edit_singleline(&mut ed.template_project_name);
                    });
                    cols[1].add_space(4.0);

                    cols[1].group(|ui| {
                        ui.label(
                            egui::RichText::new(&selected_tmpl.name)
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(0, 229, 255)),
                        );
                        ui.add_space(2.0);
                        ui.label(format!(
                            "Resolution: {}x{} Progressive",
                            selected_tmpl.aspect_width, selected_tmpl.aspect_height
                        ));
                        ui.label(format!(
                            "Duration: {:.1} seconds ({} slots)",
                            selected_tmpl.duration_us as f64 / 1_000_000.0,
                            selected_tmpl.slots.len()
                        ));

                        if let Some(music) = &selected_tmpl.music_cue {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Audio Sync: {} rhythm beat cues",
                                    music.beats_us.len()
                                ))
                                .color(egui::Color32::from_rgb(52, 211, 153)),
                            );
                        } else {
                            ui.label("Audio Sync: Standard Timeline Track");
                        }
                    });

                    cols[1].add_space(6.0);
                    cols[1].group(|ui| {
                        ui.label(
                            egui::RichText::new("MEDIA INJECTION")
                                .strong()
                                .size(10.0)
                                .color(egui::Color32::from_rgb(180, 185, 200)),
                        );
                        if ed.media_files.is_empty() {
                            ui.label(
                                egui::RichText::new("Bin is empty. Slots will be initialized with placeholder clips that you can replace anytime.")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(140, 145, 160)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} media file(s) in bin. Will inject into slots in order.",
                                    ed.media_files.len()
                                ))
                                .size(10.0)
                                .color(egui::Color32::from_rgb(52, 211, 153)),
                            );
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui
                        .add(
                            crate::ui::widgets::StudioButton::new("Create Project from Template")
                                .accent(egui::Color32::from_rgb(37, 99, 235))
                                .active(true)
                                .icon(crate::ui::icons::draw_template),
                        )
                        .clicked()
                    {
                        instantiate_idx = Some(ed.selected_template_idx);
                        close_modal = true;
                    }

                    if studio_btn(ui, "Cancel", None, false).clicked() {
                        close_modal = true;
                    }
                });
            });

        if let Some(idx) = instantiate_idx {
            let name = ed.template_project_name.clone();
            ed.instantiate_template(idx, Some(&name));
        }
        if close_modal {
            ed.show_template_modal = false;
        }
    }
}

/// Run the application event loop with an optional project path to open.
pub fn run_with_project(initial_project: Option<PathBuf>) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = ReddartApp::new(initial_project);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Run the application event loop with default project.
pub fn run() -> Result<()> {
    run_with_project(None)
}
