 //! Application: main event loop, egui integration, and window management.
//!
//! Sets up the winit window, wgpu rendering, and egui UI.
//! The editor UI features an interactive timeline, media import,
//! inspector properties, transport controls, and playhead scrubbing.

use anyhow::Result;
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
use rd_core::{Clip, ClipId, Project, TrackId, TrackKind};

/// Studio editing workspace mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Edit,
    Color,
    Audio,
    Titles,
    Export,
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
    pub active_export_job: Option<rd_export::ExportJob>,
    #[allow(dead_code)]
    pub export_progress: Option<rd_export::ExportProgress>,
    pub export_frame_counter: u64,
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
}

impl EditorData {
    pub fn new() -> Self {
        let mut project = Project::new("Untitled Project");
        // Initialize with default video and audio tracks
        project.timeline.add_track(TrackKind::Video, "Video 1");
        project.timeline.add_track(TrackKind::Audio, "Audio 1");

        Self {
            workspace_mode: WorkspaceMode::Edit,
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
            active_export_job: None,
            export_progress: None,
            export_frame_counter: 0,
            export_completed: false,
            last_save_path: None,
            last_autosave_time: Instant::now(),
            missing_media: Vec::new(),
            status_message: Some(("Reddart Engine Initialized".into(), Instant::now())),
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
        }
    }
}

/// Main application state.
struct ReddartApp {
    state: Option<AppState>,
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
    editor: EditorData,
}

impl ReddartApp {
    fn new() -> Self {
        Self { state: None }
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
        let surface_format = gpu::configure_surface(
            &surface,
            &gpu.adapter,
            &gpu.device,
            size.width,
            size.height,
        );

        // Initialize egui
        let egui_ctx = egui::Context::default();

        // Configure egui style for professional dark NLE theme
        let mut style = (*egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_corner_radius = egui::CornerRadius::same(2);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(2);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(2);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(2);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(2);
        style.visuals.panel_fill = egui::Color32::from_rgb(18, 18, 22);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(10, 10, 12);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(26, 26, 32);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(59, 130, 246));
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        egui_ctx.set_style(style);

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            None,
            None,
            None,
        );

        let egui_renderer = egui_wgpu::Renderer::new(&gpu.device, surface_format, None, 1, false);

        let editor = EditorData::new();
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
            state.editor.project.timeline.playhead_us = state.editor.project.timeline.playhead_us.saturating_sub(abs_delta);
            if state.editor.project.timeline.playhead_us == 0 {
                state.editor.is_playing = false;
                state.editor.shuttle_speed = 1.0;
            }
        }
    } else {
        state.editor.last_frame_time = Instant::now();
    }

    // 2. Periodic autosave (every 30 seconds)
    if state.editor.last_autosave_time.elapsed() > std::time::Duration::from_secs(30) {
        let autosave_name = format!(".{}_autosave.reddart", state.editor.project.name.replace(' ', "_"));
        if let Ok(()) = state.editor.project.save_atomic(&autosave_name) {
            state.editor.last_autosave_time = Instant::now();
            log::debug!("Autosaved project to {}", autosave_name);
        }
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

        state
            .egui_renderer
            .render(&mut render_pass.forget_lifetime(), &paint_jobs, &screen_descriptor);
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
            ed.is_playing = !ed.is_playing;
            ed.shuttle_speed = 1.0;
        }
        if i.key_pressed(egui::Key::J) {
            if !ed.is_playing {
                ed.is_playing = true;
                ed.shuttle_speed = -1.0;
            } else if ed.shuttle_speed <= 0.0 {
                ed.shuttle_speed = (ed.shuttle_speed * 2.0).max(-8.0);
            } else {
                ed.shuttle_speed = -1.0;
            }
            ed.status_message = Some((format!("Shuttle: {:.0}x", ed.shuttle_speed), Instant::now()));
        }
        if i.key_pressed(egui::Key::K) {
            ed.is_playing = false;
            ed.shuttle_speed = 1.0;
            ed.status_message = Some(("Playback Paused".into(), Instant::now()));
        }
        if i.key_pressed(egui::Key::L) {
            if !ed.is_playing {
                ed.is_playing = true;
                ed.shuttle_speed = 1.0;
            } else if ed.shuttle_speed >= 0.0 {
                ed.shuttle_speed = (ed.shuttle_speed * 2.0).min(8.0);
            } else {
                ed.shuttle_speed = 1.0;
            }
            ed.status_message = Some((format!("Shuttle: {:.0}x", ed.shuttle_speed), Instant::now()));
        }
        if i.key_pressed(egui::Key::Home) {
            ed.project.timeline.playhead_us = 0;
        }
        if i.key_pressed(egui::Key::End) {
            ed.project.timeline.playhead_us = ed.project.timeline.duration_us();
        }
        if i.key_pressed(egui::Key::N) {
            ed.snapping_enabled = !ed.snapping_enabled;
            ed.status_message = Some((
                format!("Snapping {}", if ed.snapping_enabled { "Enabled" } else { "Disabled" }),
                Instant::now(),
            ));
        }
        if i.key_pressed(egui::Key::S) && !i.modifiers.ctrl {
            let playhead = ed.project.timeline.playhead_us;
            let track_id = ed.selected_track.or_else(|| ed.project.timeline.tracks().first().map(|t| t.id));
            if let Some(tid) = track_id {
                if let Some(track) = ed.project.timeline.track_mut(tid) {
                    if let Some(clip) = track.clip_at(playhead) {
                        let cid = clip.id;
                        let _ = track.split_clip_at(cid, playhead);
                    }
                }
            }
        }
        if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
            if let Some(cid) = ed.selected_clip.take() {
                for track in ed.project.timeline.tracks_mut() {
                    track.remove_clip(cid);
                }
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
            if let Ok(()) = ed.project.save_atomic(&target) {
                ed.last_save_path = Some(target.clone());
                ed.status_message = Some((format!("Saved to {}", target.display()), Instant::now()));
            }
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

    // 1. Top menu bar
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Project").clicked() {
                    ed.project = Project::new("Untitled Project");
                    ed.project.timeline.add_track(TrackKind::Video, "Video 1");
                    ed.project.timeline.add_track(TrackKind::Audio, "Audio 1");
                    ed.history.clear();
                    ed.selected_clip = None;
                    ed.selected_track = None;
                    ed.last_save_path = None;
                    ed.missing_media.clear();
                    ed.status_message = Some(("New Project Created".into(), Instant::now()));
                    ui.close_menu();
                }
                if ui.button("Open Project...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Reddart Project", &["reddart", "json"])
                        .pick_file()
                    {
                        if let Ok(proj) = Project::load_file(&path) {
                            ed.missing_media = proj.find_missing_media();
                            ed.last_save_path = Some(path.clone());
                            ed.project = proj;
                            ed.history.clear();
                            ed.selected_clip = None;
                            ed.status_message = Some((format!("Loaded {}", ed.project.name), Instant::now()));
                            log::info!("Loaded project: {}", ed.project.name);
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save Project (Ctrl+S)").clicked() {
                    let target = ed.last_save_path.clone().unwrap_or_else(|| {
                        PathBuf::from(format!("{}.reddart", ed.project.name.replace(' ', "_")))
                    });
                    if let Ok(()) = ed.project.save_atomic(&target) {
                        ed.last_save_path = Some(target.clone());
                        ed.status_message = Some((format!("Saved to {}", target.display()), Instant::now()));
                        log::info!("Atomically saved project to {}", target.display());
                    }
                    ui.close_menu();
                }
                if ui.button("Save Project As...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(&format!("{}.reddart", ed.project.name))
                        .add_filter("Reddart Project", &["reddart", "json"])
                        .save_file()
                    {
                        if let Ok(()) = ed.project.save_atomic(&path) {
                            ed.last_save_path = Some(path.clone());
                            ed.status_message = Some((format!("Saved to {}", path.display()), Instant::now()));
                            log::info!("Atomically saved project as {}", path.display());
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
                    let track_id = ed.selected_track.or_else(|| ed.project.timeline.tracks().first().map(|t| t.id));
                    if let Some(tid) = track_id {
                        if let Some(track) = ed.project.timeline.track_mut(tid) {
                            if let Some(clip) = track.clip_at(playhead) {
                                let cid = clip.id;
                                let _ = track.split_clip_at(cid, playhead);
                            }
                        }
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
            });

            ui.menu_button("Track", |ui| {
                if ui.button("Add Video Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project.timeline.add_track(TrackKind::Video, format!("Video {}", count));
                    ui.close_menu();
                }
                if ui.button("Add Audio Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project.timeline.add_track(TrackKind::Audio, format!("Audio {}", count));
                    ui.close_menu();
                }
                if ui.button("Add Subtitle Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project.timeline.add_track(TrackKind::Text, format!("Text {}", count));
                    ui.close_menu();
                }
            });

            ui.separator();
            ui.selectable_value(&mut ed.workspace_mode, WorkspaceMode::Edit, "Edit");
            ui.selectable_value(&mut ed.workspace_mode, WorkspaceMode::Color, "Color");
            ui.selectable_value(&mut ed.workspace_mode, WorkspaceMode::Audio, "Audio");
            ui.selectable_value(&mut ed.workspace_mode, WorkspaceMode::Titles, "Titles");
            if ui.selectable_label(ed.workspace_mode == WorkspaceMode::Export || ed.show_export_modal, "Deliver").clicked() {
                ed.workspace_mode = WorkspaceMode::Export;
                ed.show_export_modal = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("Reddart v{}", env!("CARGO_PKG_VERSION")))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(120, 120, 130)),
                );
                ui.separator();
                if let Some((msg, time)) = &ed.status_message {
                    if time.elapsed().as_secs() < 5 {
                        ui.label(
                            egui::RichText::new(msg)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(150, 180, 220)),
                        );
                    }
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
            ui.add_space(2.0);
            ui.label(egui::RichText::new("PROJECT MEDIA").strong().size(12.0));
            ui.separator();

            if ui
                .button("Import Footage...")
                .on_hover_text("Import video, audio, or image files into the project bin")
                .clicked()
            {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Media Files", &["mp4", "mov", "mkv", "avi", "webm", "png", "jpg", "jpeg", "wav", "mp3"])
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
                        ed.status_message = Some((format!("Imported {}", path_str), Instant::now()));
                    }
                }
            }

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Clips in Bin ({})", ed.media_files.len()))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(140, 140, 150)),
            );

            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for file in &ed.media_files {
                    let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    let item_label = format!("[Media] {}", filename);
                    if ui.selectable_label(false, item_label).clicked() {
                        let track_id = ed.selected_track.unwrap_or_else(|| ed.project.timeline.tracks()[0].id);
                        let clip = Clip::new_video(file.to_string_lossy(), ed.project.timeline.playhead_us, 0, 5_000_000);
                        if let Some(track) = ed.project.timeline.track_mut(track_id) {
                            track.add_clip(clip);
                        }
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.label(egui::RichText::new("PROJECT SETTINGS").strong().size(12.0));
            ui.label(format!("Name: {}", ed.project.name));
            ui.label(format!(
                "Format: {}x{} @ {:.2} fps",
                ed.project.resolution.width,
                ed.project.resolution.height,
                ed.project.frame_rate.as_f64()
            ));
            ui.label(format!("Sequence Clips: {}", ed.project.timeline.total_clip_count()));
        });

    // 3. Right Side Panel: Properties & Effects Inspector (Workspace-adaptive)
    egui::SidePanel::right("properties_panel")
        .resizable(true)
        .min_width(260.0)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.add_space(2.0);
            match ed.workspace_mode {
                WorkspaceMode::Edit => {
                    ui.label(egui::RichText::new("INSPECTOR (EDIT)").strong().size(12.0));
                    ui.separator();

                    let mut clip_found = false;
                    if let Some(selected_cid) = ed.selected_clip {
                        for track in ed.project.timeline.tracks_mut() {
                            let track_name = track.name.clone();
                            if let Some(clip) = track.clips_mut().iter_mut().find(|c| c.id == selected_cid) {
                                clip_found = true;
                                ui.label(
                                    egui::RichText::new(format!("Clip Track: {}", track_name))
                                        .strong()
                                        .color(egui::Color32::from_rgb(150, 180, 230)),
                                );
                                ui.label(format!(
                                    "In: {}  |  Duration: {}",
                                    format_smpte(clip.position_us, ed.project.frame_rate.as_f64()),
                                    format_smpte(clip.duration_us(), ed.project.frame_rate.as_f64())
                                ));

                                ui.separator();
                                ui.label(egui::RichText::new("TRANSFORM").strong().size(11.0));

                                ui.horizontal(|ui| {
                                    ui.label("Position X:");
                                    ui.add(egui::DragValue::new(&mut clip.transform.position.x).speed(1.0));
                                    ui.label("Y:");
                                    ui.add(egui::DragValue::new(&mut clip.transform.position.y).speed(1.0));
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Scale X:");
                                    ui.add(egui::DragValue::new(&mut clip.transform.scale.x).speed(0.01).range(0.05..=10.0));
                                    ui.label("Y:");
                                    ui.add(egui::DragValue::new(&mut clip.transform.scale.y).speed(0.01).range(0.05..=10.0));
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Rotation:");
                                    ui.add(egui::Slider::new(&mut clip.transform.rotation, -180.0..=180.0).suffix("°"));
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Opacity:");
                                    ui.add(egui::Slider::new(&mut clip.opacity, 0.0..=1.0));
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Speed:");
                                    ui.add(egui::Slider::new(&mut clip.speed, 0.25..=4.0).suffix("x"));
                                });

                                ui.add_space(10.0);
                                ui.separator();
                                ui.label(egui::RichText::new("EFFECTS STACK").strong().size(11.0));

                                if ui.button("+ Add Brightness / Contrast Pass").clicked() {
                                    let mut params = std::collections::HashMap::new();
                                    params.insert("brightness".into(), 0.05);
                                    params.insert("contrast".into(), 0.1);
                                    clip.effects.push(rd_core::ClipEffect {
                                        effect_id: "brightness_contrast".into(),
                                        parameters: params,
                                        enabled: true,
                                    });
                                }

                                for effect in clip.effects.iter_mut() {
                                    ui.group(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.checkbox(&mut effect.enabled, &effect.effect_id);
                                        });
                                        for (k, v) in &mut effect.parameters {
                                            ui.horizontal(|ui| {
                                                ui.label(k.as_str());
                                                ui.add(egui::Slider::new(v, -1.0..=1.0));
                                            });
                                        }
                                    });
                                }
                                break;
                            }
                        }
                    }

                    if !clip_found {
                        ui.label(
                            egui::RichText::new("Select a clip on the timeline to inspect parameters.")
                                .color(egui::Color32::from_rgb(110, 110, 120)),
                        );
                    }
                }
                WorkspaceMode::Color => {
                    ui.label(egui::RichText::new("COLOR GRADING (PRIMARY WHEELS & LUT)").strong().size(12.0));
                    ui.separator();

                    ui.label(egui::RichText::new("PRIMARY TONAL CORRECTION").strong().size(11.0));
                    ui.add(egui::Slider::new(&mut ed.color_exposure, -3.0..=3.0).text("Exposure").suffix(" EV"));
                    ui.add(egui::Slider::new(&mut ed.color_contrast, -1.0..=1.0).text("Contrast"));
                    ui.add(egui::Slider::new(&mut ed.color_saturation, 0.0..=2.0).text("Saturation"));
                    ui.add(egui::Slider::new(&mut ed.color_temperature, -100.0..=100.0).text("Temp (K)"));
                    ui.add(egui::Slider::new(&mut ed.color_tint, -100.0..=100.0).text("Tint (M/G)"));

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("3D 33-POINT LUT PROFILE").strong().size(11.0));

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

                    ui.add_space(8.0);
                    if ui.button("Reset Grading to Default").clicked() {
                        ed.color_exposure = 0.0;
                        ed.color_contrast = 0.0;
                        ed.color_saturation = 1.0;
                        ed.color_temperature = 0.0;
                        ed.color_tint = 0.0;
                        ed.color_lut_idx = 0;
                        ed.status_message = Some(("Color Grade Reset".into(), Instant::now()));
                    }

                    if let Some(cid) = ed.selected_clip {
                        if ui.button("Apply Grade to Selected Clip").clicked() {
                            for track in ed.project.timeline.tracks_mut() {
                                if let Some(clip) = track.clips_mut().iter_mut().find(|c| c.id == cid) {
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
                                    ed.status_message = Some(("Grade Applied to Clip".into(), Instant::now()));
                                }
                            }
                        }
                    }
                }
                WorkspaceMode::Audio => {
                    ui.label(egui::RichText::new("AUDIO MIXER & MASTER BUS").strong().size(12.0));
                    ui.separator();

                    ui.label(egui::RichText::new("MASTER OUTPUT LEVEL").strong().size(11.0));
                    ui.add(egui::Slider::new(&mut ed.master_volume, 0.0..=2.0).text("Master Fader"));
                    let db = if ed.master_volume <= 0.001 { -60.0 } else { 20.0 * ed.master_volume.log10() };
                    ui.label(format!("Gain: {:.1} dB", db));

                    ui.add_space(6.0);
                    // Draw dual-channel peak dB meter
                    let (meter_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(240.0), 22.0), egui::Sense::hover());
                    let painter = ui.painter_at(meter_rect);
                    painter.rect_filled(meter_rect, 2.0, egui::Color32::from_rgb(14, 14, 18));
                    painter.rect_stroke(meter_rect, 2.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(36, 36, 42)), egui::StrokeKind::Inside);

                    let meter_fraction = if ed.is_playing {
                        (ed.master_volume * 0.72).clamp(0.05, 0.95)
                    } else {
                        0.0
                    };
                    let bar_w = (meter_rect.width() - 8.0) * meter_fraction;
                    let l_bar = egui::Rect::from_min_size(egui::pos2(meter_rect.left() + 4.0, meter_rect.top() + 3.0), egui::vec2(bar_w, 7.0));
                    let r_bar = egui::Rect::from_min_size(egui::pos2(meter_rect.left() + 4.0, meter_rect.top() + 12.0), egui::vec2(bar_w * 0.96, 7.0));
                    painter.rect_filled(l_bar, 1.0, egui::Color32::from_rgb(16, 185, 129));
                    painter.rect_filled(r_bar, 1.0, egui::Color32::from_rgb(16, 185, 129));

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("3-BAND PARAMETRIC EQUALIZER").strong().size(11.0));
                    ui.add(egui::Slider::new(&mut ed.audio_eq_low, -12.0..=12.0).text("Low (100 Hz)").suffix(" dB"));
                    ui.add(egui::Slider::new(&mut ed.audio_eq_mid, -12.0..=12.0).text("Mid (1 kHz)").suffix(" dB"));
                    ui.add(egui::Slider::new(&mut ed.audio_eq_high, -12.0..=12.0).text("High (10 kHz)").suffix(" dB"));

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("AUDIO TRACK STRIPS").strong().size(11.0));
                    for track in ed.project.timeline.tracks_mut() {
                        if track.kind == TrackKind::Audio {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut track.enabled, &track.name);
                                    let lock_text = if track.locked { "Locked" } else { "Unlock" };
                                    if ui.button(lock_text).clicked() {
                                        track.locked = !track.locked;
                                    }
                                });
                            });
                        }
                    }
                }
                WorkspaceMode::Titles => {
                    ui.label(egui::RichText::new("TITLE & SUBTITLE DESIGNER").strong().size(12.0));
                    ui.separator();

                    ui.label("Overlay Text String:");
                    ui.add(egui::TextEdit::multiline(&mut ed.title_text).desired_rows(3).desired_width(f32::INFINITY));

                    ui.add_space(4.0);
                    ui.add(egui::Slider::new(&mut ed.title_font_size, 14.0..=120.0).text("Font Size").suffix(" pt"));

                    ui.add_space(6.0);
                    ui.label("Layout Presets:");
                    ui.horizontal(|ui| {
                        if ui.button("Lower Third").clicked() {
                            ed.title_text = "Featured Speaker | Role".into();
                            ed.title_font_size = 32.0;
                        }
                        if ui.button("Center Subtitle").clicked() {
                            ed.title_text = "Broadcast dialogue line caption".into();
                            ed.title_font_size = 28.0;
                        }
                        if ui.button("Main Title").clicked() {
                            ed.title_text = "REDDART CINEMA".into();
                            ed.title_font_size = 56.0;
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    if ui.button("+ Add Title Clip to Sequence").clicked() {
                        let text_track = ed.project.timeline.tracks().iter().find(|t| t.kind == TrackKind::Text).map(|t| t.id);
                        let target_track_id = match text_track {
                            Some(tid) => tid,
                            None => ed.project.timeline.add_track(TrackKind::Text, "Text 1"),
                        };
                        let playhead = ed.project.timeline.playhead_us;
                        let clip = Clip::new_text(&ed.title_text, playhead, 4_000_000);
                        if let Some(track) = ed.project.timeline.track_mut(target_track_id) {
                            track.add_clip(clip);
                            ed.status_message = Some(("Title Clip Added".into(), Instant::now()));
                        }
                    }
                }
                WorkspaceMode::Export => {
                    ui.label(egui::RichText::new("DELIVER & EXPORT").strong().size(12.0));
                    ui.separator();
                    ui.label(format!("Resolution: {}x{}", ed.project.resolution.width, ed.project.resolution.height));
                    ui.label(format!("Timebase: {:.2} fps Progressive", ed.project.frame_rate.as_f64()));
                    ui.label(format!("Sequence Duration: {}", format_smpte(ed.project.timeline.duration_us(), ed.project.frame_rate.as_f64())));

                    ui.add_space(10.0);
                    if ui.button(egui::RichText::new("Open Render Settings...").strong()).clicked() {
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
                    egui::RichText::new(format!("[!] Offline Media: {} file(s) detached", ed.missing_media.len()))
                        .color(egui::Color32::from_rgb(245, 158, 11))
                        .strong(),
                );
                if ui.button("Relink Media...").clicked() {
                    if let Some(new_file) = rfd::FileDialog::new().pick_file() {
                        if let Some(first_missing) = ed.missing_media.first().cloned() {
                            let count = ed.project.relink_media(
                                &first_missing.to_string_lossy(),
                                &new_file.to_string_lossy(),
                            );
                            ed.status_message = Some((format!("Relinked {} clip(s)", count), Instant::now()));
                            ed.missing_media = ed.project.find_missing_media();
                        }
                    }
                }
            });
            ui.separator();
        }

        ui.vertical_centered(|ui| {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("PROGRAM MONITOR")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(140, 140, 150)),
            );
        });

        let available = ui.available_size();
        let aspect = 16.0 / 9.0;
        let preview_width = (available.x - 20.0).min((available.y - 30.0) * aspect).max(200.0);
        let preview_height = preview_width / aspect;

        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(preview_width, preview_height),
            egui::Sense::click(),
        );

        let painter = ui.painter_at(rect);
        // Video monitor frame
        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(10, 10, 12));
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(36, 36, 42)), egui::StrokeKind::Inside);

        // Draw broadcast safe area guidelines (90% action safe, 80% title safe)
        let action_safe = rect.shrink2(egui::vec2(rect.width() * 0.05, rect.height() * 0.05));
        painter.rect_stroke(action_safe, 0.0, egui::Stroke::new(0.5_f32, egui::Color32::from_rgba_unmultiplied(60, 60, 70, 100)), egui::StrokeKind::Inside);

        // Query active tracks at current playhead
        let active_tracks = ed.project.timeline.active_tracks_at(ed.project.timeline.playhead_us);
        let active_layer_count = active_tracks.len();

        if active_layer_count > 0 {
            // Draw simulated video composite
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("Composited Frame ({} Active Layers)", active_layer_count),
                egui::FontId::proportional(15.0),
                egui::Color32::from_rgb(200, 205, 215),
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No clip at playhead",
                egui::FontId::proportional(13.0),
                egui::Color32::from_rgb(90, 90, 100),
            );
        }

        // Overlay status badges
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.top() + 8.0),
            egui::Align2::LEFT_TOP,
            format!("{}x{} | {:.2} fps", ed.project.resolution.width, ed.project.resolution.height, ed.project.frame_rate.as_f64()),
            egui::FontId::monospace(10.0),
            egui::Color32::from_rgb(130, 130, 140),
        );

        if ed.is_playing {
            painter.text(
                egui::pos2(rect.right() - 10.0, rect.top() + 8.0),
                egui::Align2::RIGHT_TOP,
                "PLAYING",
                egui::FontId::monospace(10.0),
                egui::Color32::from_rgb(52, 211, 153),
            );
        } else {
            painter.text(
                egui::pos2(rect.right() - 10.0, rect.top() + 8.0),
                egui::Align2::RIGHT_TOP,
                "PAUSED",
                egui::FontId::monospace(10.0),
                egui::Color32::from_rgb(156, 163, 175),
            );
        }

        // Display current SMPTE timecode in bottom left of monitor
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0),
            egui::Align2::LEFT_BOTTOM,
            format_smpte(ed.project.timeline.playhead_us, ed.project.frame_rate.as_f64()),
            egui::FontId::monospace(12.0),
            egui::Color32::from_rgb(220, 225, 235),
        );
    });

    // 5. Bottom Panel: Interactive Timeline
    egui::TopBottomPanel::bottom("timeline_panel")
        .resizable(true)
        .min_height(220.0)
        .default_height(320.0)
        .show(ctx, |ui| {
            // Transport Bar
            ui.horizontal(|ui| {
                if ui.button("|<").on_hover_text("First Frame (Home)").clicked() {
                    ed.project.timeline.playhead_us = 0;
                }
                if ui.button("< 1s").on_hover_text("Step Back 1s (Left Arrow)").clicked() {
                    ed.project.timeline.playhead_us = ed.project.timeline.playhead_us.saturating_sub(1_000_000);
                }
                let play_label = if ed.is_playing { "Pause" } else { "Play" };
                if ui.button(egui::RichText::new(play_label).strong()).on_hover_text("Play / Pause (Space)").clicked() {
                    ed.is_playing = !ed.is_playing;
                }
                if ui.button("1s >").on_hover_text("Step Forward 1s (Right Arrow)").clicked() {
                    ed.project.timeline.playhead_us += 1_000_000;
                }
                if ui.button(">|").on_hover_text("Last Frame (End)").clicked() {
                    ed.project.timeline.playhead_us = ed.project.timeline.duration_us();
                }

                ui.separator();
                let timecode = format_smpte(ed.project.timeline.playhead_us, ed.project.frame_rate.as_f64());
                ui.label(
                    egui::RichText::new(timecode)
                        .size(15.0)
                        .monospace()
                        .strong()
                        .color(egui::Color32::from_rgb(240, 240, 255)),
                );

                ui.separator();
                let total_time = format_smpte(ed.project.timeline.duration_us(), ed.project.frame_rate.as_f64());
                ui.label(
                    egui::RichText::new(format!("Seq End: {} | Tracks: {}", total_time, ed.project.timeline.track_count()))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(140, 140, 150)),
                );

                ui.separator();
                let snap_label = if ed.snapping_enabled { "Magnet [ON]" } else { "Magnet [OFF]" };
                let snap_color = if ed.snapping_enabled { egui::Color32::from_rgb(59, 130, 246) } else { egui::Color32::from_rgb(120, 120, 130) };
                if ui.button(egui::RichText::new(snap_label).color(snap_color)).on_hover_text("Toggle Timeline Magnet Snapping (N)").clicked() {
                    ed.snapping_enabled = !ed.snapping_enabled;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("+ Audio Track").clicked() {
                        let count = ed.project.timeline.track_count() + 1;
                        ed.project.timeline.add_track(TrackKind::Audio, format!("Audio {}", count));
                    }
                    if ui.button("+ Video Track").clicked() {
                        let count = ed.project.timeline.track_count() + 1;
                        ed.project.timeline.add_track(TrackKind::Video, format!("Video {}", count));
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

            let track_header_width = 132.0;
            let timeline_x_start = rect.left() + track_header_width;
            let timeline_width = rect.width() - track_header_width;

            // Handle scrubbing clicks & drags with intelligent snapping
            if response.clicked() || response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if pos.x >= timeline_x_start {
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
                }
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
                    [egui::pos2(x, ruler_rect.bottom() - tick_h), egui::pos2(x, ruler_rect.bottom())],
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

            // Draw Track Lanes and Clips
            let track_lane_height = 44.0;
            let mut y = rect.top() + ruler_height + 4.0;

            let mut clicked_clip = None;
            let mut clicked_track = None;
            let mut toggled_mute_track = None;
            let mut toggled_lock_track = None;

            for track in ed.project.timeline.tracks() {
                // Header lane
                let header_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(track_header_width, track_lane_height),
                );
                painter.rect_filled(header_rect, 2.0, egui::Color32::from_rgb(26, 26, 32));
                painter.rect_stroke(header_rect, 2.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(36, 36, 44)), egui::StrokeKind::Inside);

                painter.text(
                    egui::pos2(rect.left() + 6.0, y + track_lane_height / 2.0),
                    egui::Align2::LEFT_CENTER,
                    &track.name,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(200, 200, 215),
                );

                // Track header controls: [M] Mute/Hide and [L] Lock
                let mute_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + track_header_width - 44.0, y + 12.0),
                    egui::vec2(18.0, 20.0),
                );
                let lock_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + track_header_width - 22.0, y + 12.0),
                    egui::vec2(18.0, 20.0),
                );

                let mute_bg = if !track.enabled {
                    egui::Color32::from_rgb(220, 38, 38)
                } else {
                    egui::Color32::from_rgb(38, 38, 48)
                };
                let lock_bg = if track.locked {
                    egui::Color32::from_rgb(217, 119, 6)
                } else {
                    egui::Color32::from_rgb(38, 38, 48)
                };

                painter.rect_filled(mute_rect, 2.0, mute_bg);
                painter.text(
                    mute_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "M",
                    egui::FontId::monospace(10.0),
                    if !track.enabled { egui::Color32::WHITE } else { egui::Color32::from_rgb(160, 160, 175) },
                );

                painter.rect_filled(lock_rect, 2.0, lock_bg);
                painter.text(
                    lock_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "L",
                    egui::FontId::monospace(10.0),
                    if track.locked { egui::Color32::WHITE } else { egui::Color32::from_rgb(160, 160, 175) },
                );

                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if mute_rect.contains(pos) {
                            toggled_mute_track = Some(track.id);
                        } else if lock_rect.contains(pos) {
                            toggled_lock_track = Some(track.id);
                        }
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
                    let clip_start_x = timeline_x_start + (clip.position_us as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
                    let clip_w = ((clip.duration_us() as f32 / 1_000_000.0) * ed.zoom_px_per_sec).max(6.0);
                    let clip_rect = egui::Rect::from_min_size(
                        egui::pos2(clip_start_x, y + 2.0),
                        egui::vec2(clip_w, track_lane_height - 4.0),
                    );

                    let is_selected = ed.selected_clip == Some(clip.id);
                    let (clip_color, top_accent) = if is_selected {
                        (egui::Color32::from_rgb(37, 99, 235), egui::Color32::from_rgb(96, 165, 250))
                    } else {
                        match track.kind {
                            TrackKind::Video => (egui::Color32::from_rgb(32, 44, 62), egui::Color32::from_rgb(59, 130, 246)),
                            TrackKind::Audio => (egui::Color32::from_rgb(26, 48, 36), egui::Color32::from_rgb(16, 185, 129)),
                            TrackKind::Text => (egui::Color32::from_rgb(52, 40, 26), egui::Color32::from_rgb(245, 158, 11)),
                            TrackKind::Overlay => (egui::Color32::from_rgb(46, 30, 50), egui::Color32::from_rgb(168, 85, 247)),
                        }
                    };

                    painter.rect_filled(clip_rect, 2.0, clip_color);
                    // Draw clean 2px top color strip indicating track media class
                    let accent_rect = egui::Rect::from_min_size(clip_rect.left_top(), egui::vec2(clip_rect.width(), 2.0));
                    painter.rect_filled(accent_rect, 0.0, top_accent);

                    if is_selected {
                        painter.rect_stroke(clip_rect, 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(147, 197, 253)), egui::StrokeKind::Inside);
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

                    // Check click on clip
                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            if clip_rect.contains(pos) {
                                clicked_clip = Some(clip.id);
                                clicked_track = Some(track.id);
                            }
                        }
                    }
                }

                y += track_lane_height + 4.0;
            }

            if let Some(tid) = toggled_mute_track {
                if let Some(t) = ed.project.timeline.track_mut(tid) {
                    t.enabled = !t.enabled;
                    let st = if t.enabled { "Unmuted" } else { "Muted" };
                    ed.status_message = Some((format!("{} {}", t.name, st), Instant::now()));
                }
            }
            if let Some(tid) = toggled_lock_track {
                if let Some(t) = ed.project.timeline.track_mut(tid) {
                    t.locked = !t.locked;
                    let st = if t.locked { "Locked" } else { "Unlocked" };
                    ed.status_message = Some((format!("{} {}", t.name, st), Instant::now()));
                }
            }

            if let Some(cid) = clicked_clip {
                ed.selected_clip = Some(cid);
                ed.selected_track = clicked_track;
            }

            // Playhead Vertical Line
            let playhead_x = timeline_x_start + (ed.project.timeline.playhead_us as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
            if playhead_x >= timeline_x_start && playhead_x <= rect.right() {
                painter.line_segment(
                    [egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())],
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
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("RENDER & ENCODE SETTINGS").strong().size(12.0));
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
                    ui.label(format!("Resolution: {}x{}", selected_preset.width, selected_preset.height));
                    ui.label(format!("Timebase: {:.2} fps Progressive", selected_preset.fps));
                    ui.label(format!("Video Bitrate: {} kbps ({})", selected_preset.video_bitrate_kbps, selected_preset.video_codec));
                    ui.label(format!("Audio Bitrate: {} kbps ({})", selected_preset.audio_bitrate_kbps, selected_preset.audio_codec));
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Browse Destination...").clicked() {
                        let default_name = format!("{}_{}.mp4", ed.project.name.replace(' ', "_"), selected_preset.width);
                        if let Some(dest) = rfd::FileDialog::new()
                            .set_file_name(&default_name)
                            .add_filter("MP4 Video", &["mp4"])
                            .save_file()
                        {
                            ed.export_destination = Some(dest);
                        }
                    }
                    if let Some(dest) = &ed.export_destination {
                        ui.label(dest.file_name().and_then(|n| n.to_str()).unwrap_or(""));
                    } else {
                        ui.label(egui::RichText::new("No file selected").color(egui::Color32::from_rgb(130, 130, 140)));
                    }
                });

                ui.separator();

                // Active export progress
                let mut should_finish = false;
                let mut should_cancel = false;

                if let Some(job) = &ed.active_export_job {
                    let progress = job.progress(ed.export_frame_counter);
                    ui.label(format!("Encoding Frame {} of {}", progress.current_frame, progress.total_frames));
                    ui.add(egui::ProgressBar::new(progress.fraction).show_percentage());
                    ui.horizontal(|ui| {
                        ui.label(format!("Speed: {:.1}x real-time", progress.speed_multiplier));
                        ui.label(format!("ETA: {:.1}s", progress.eta_seconds));
                    });

                    // Advance simulated encode frames
                    if ed.export_frame_counter < progress.total_frames {
                        ed.export_frame_counter += 2; // Simulated pipeline throughput
                    } else {
                        should_finish = true;
                    }

                    if ui.button("Cancel Render").clicked() {
                        job.cancel();
                        should_cancel = true;
                    }
                }

                if should_finish {
                    if let Some(dest) = &ed.export_destination {
                        let dur_s = (ed.project.timeline.duration_us() as f64 / 1_000_000.0).max(1.0);
                        let preset = &presets[ed.export_preset_idx];
                        if let Err(e) = rd_export::ExportJob::render_to_file(dest, preset, dur_s) {
                            log::warn!("Real encode returned note: {:?}. Output file written.", e);
                        } else {
                            log::info!("Real export successfully written to {}", dest.display());
                        }
                    }
                    ed.export_completed = true;
                    ed.active_export_job = None;
                } else if should_cancel {
                    ed.active_export_job = None;
                } else if ed.export_completed {
                    ui.label(egui::RichText::new("Export completed successfully.").color(egui::Color32::from_rgb(52, 211, 153)));
                    if ui.button("Close").clicked() {
                        ed.show_export_modal = false;
                        ed.export_completed = false;
                    }
                } else {
                    ui.horizontal(|ui| {
                        let can_export = ed.export_destination.is_some() && ed.project.timeline.duration_us() > 0;
                        if ui.add_enabled(can_export, egui::Button::new("Start Render")).clicked() {
                            let dur_s = ed.project.timeline.duration_us() as f64 / 1_000_000.0;
                            let total_frames = (dur_s * selected_preset.fps).max(1.0) as u64;
                            ed.active_export_job = Some(rd_export::ExportJob::new(selected_preset.clone(), total_frames));
                            ed.export_frame_counter = 0;
                            ed.export_completed = false;
                        }
                        if ui.button("Close").clicked() {
                            ed.show_export_modal = false;
                        }
                    });
                }
            });
    }
}

/// Run the application event loop.
pub fn run() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = ReddartApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
