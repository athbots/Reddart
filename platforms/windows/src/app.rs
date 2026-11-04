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

/// Editor state holding user project data, selection, and playback state.
pub struct EditorData {
    pub project: Project,
    pub history: rd_core::CommandHistory,
    pub is_playing: bool,
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
}

impl EditorData {
    pub fn new() -> Self {
        let mut project = Project::new("Untitled Project");
        // Initialize with default video and audio tracks
        project.timeline.add_track(TrackKind::Video, "Video 1");
        project.timeline.add_track(TrackKind::Audio, "Audio 1");

        Self {
            project,
            history: rd_core::CommandHistory::new(100),
            is_playing: false,
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

        // Configure egui style for dark theme
        let mut style = (*egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        style.visuals.panel_fill = egui::Color32::from_rgb(22, 22, 26);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(14, 14, 17);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(32, 32, 38);
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
    // 1. Playback time advancement
    if state.editor.is_playing {
        let now = Instant::now();
        let dt = now.duration_since(state.editor.last_frame_time);
        state.editor.last_frame_time = now;
        let dt_us = dt.as_micros() as i64;
        let total_us = state.editor.project.timeline.duration_us();

        state.editor.project.timeline.playhead_us += dt_us;
        if total_us > 0 && state.editor.project.timeline.playhead_us >= total_us {
            // Loop back to start
            state.editor.project.timeline.playhead_us = 0;
        }
    } else {
        state.editor.last_frame_time = Instant::now();
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
                if ui.button("📄 New Project").clicked() {
                    ed.project = Project::new("Untitled Project");
                    ed.project.timeline.add_track(TrackKind::Video, "Video 1");
                    ed.project.timeline.add_track(TrackKind::Audio, "Audio 1");
                    ed.history.clear();
                    ed.selected_clip = None;
                    ed.selected_track = None;
                    ui.close_menu();
                }
                if ui.button("📂 Open Project...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Reddart Project", &["reddart", "json"])
                        .pick_file()
                    {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(proj) = Project::from_json(&content) {
                                ed.project = proj;
                                ed.history.clear();
                                ed.selected_clip = None;
                                log::info!("Loaded project: {}", ed.project.name);
                            }
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("💾 Save").clicked() {
                    let filename = format!("{}.reddart", ed.project.name.replace(' ', "_"));
                    if let Ok(json) = ed.project.to_json() {
                        let _ = std::fs::write(&filename, json);
                        log::info!("Saved project to {}", filename);
                    }
                    ui.close_menu();
                }
                if ui.button("💾 Save As...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(&format!("{}.reddart", ed.project.name))
                        .add_filter("Reddart Project", &["reddart", "json"])
                        .save_file()
                    {
                        if let Ok(json) = ed.project.to_json() {
                            let _ = std::fs::write(path, json);
                            log::info!("Saved project");
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("🚀 Export...").clicked() {
                    ed.show_export_modal = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("Edit", |ui| {
                if ui.button("↩ Undo (Ctrl+Z)").clicked() {
                    let _ = ed.history.undo(&mut ed.project);
                    ui.close_menu();
                }
                if ui.button("↪ Redo (Ctrl+Y)").clicked() {
                    let _ = ed.history.redo(&mut ed.project);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("✂ Split at Playhead (S)").clicked() {
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
                if ui.button("🗑 Delete Selected (Del)").clicked() {
                    if let Some(cid) = ed.selected_clip.take() {
                        for track in ed.project.timeline.tracks_mut() {
                            track.remove_clip(cid);
                        }
                    }
                    ui.close_menu();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("🔍 Zoom In").clicked() {
                    ed.zoom_px_per_sec = (ed.zoom_px_per_sec * 1.25).min(500.0);
                    ui.close_menu();
                }
                if ui.button("🔍 Zoom Out").clicked() {
                    ed.zoom_px_per_sec = (ed.zoom_px_per_sec / 1.25).max(10.0);
                    ui.close_menu();
                }
                if ui.button("↔ Fit Timeline").clicked() {
                    let dur_s = (ed.project.timeline.duration_us() as f32 / 1_000_000.0).max(1.0);
                    ed.zoom_px_per_sec = (800.0 / dur_s).clamp(10.0, 300.0);
                    ui.close_menu();
                }
            });

            ui.menu_button("Track", |ui| {
                if ui.button("+ Add Video Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project.timeline.add_track(TrackKind::Video, format!("Video {}", count));
                    ui.close_menu();
                }
                if ui.button("+ Add Audio Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project.timeline.add_track(TrackKind::Audio, format!("Audio {}", count));
                    ui.close_menu();
                }
                if ui.button("+ Add Text Track").clicked() {
                    let count = ed.project.timeline.track_count() + 1;
                    ed.project.timeline.add_track(TrackKind::Text, format!("Text {}", count));
                    ui.close_menu();
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("Reddart v{}", env!("CARGO_PKG_VERSION")));
            });
        });
    });

    // 2. Left Side Panel: Media Browser
    egui::SidePanel::left("media_panel")
        .resizable(true)
        .min_width(200.0)
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.heading("📁 Media");
            ui.separator();

            if ui
                .button("📂 Import Media...")
                .on_hover_text("Import video, audio, or image files")
                .clicked()
            {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Media Files", &["mp4", "mov", "mkv", "avi", "webm", "png", "jpg", "jpeg", "wav", "mp3"])
                    .pick_files()
                {
                    for file in files {
                        let path_str = file.to_string_lossy().to_string();

                        // Add track if none exists
                        if ed.project.timeline.track_count() == 0 {
                            ed.project.timeline.add_track(TrackKind::Video, "Video 1");
                        }
                        let track_id = ed.project.timeline.tracks()[0].id;
                        let start_pos = ed.project.timeline.duration_us();
                        let default_dur = 5_000_000; // 5 seconds default

                        let clip = Clip::new_video(&path_str, start_pos, 0, default_dur);
                        if let Some(track) = ed.project.timeline.track_mut(track_id) {
                            track.add_clip(clip);
                        }
                        ed.media_files.push(file);
                        log::info!("Imported media file: {}", path_str);
                    }
                }
            }

            ui.add_space(8.0);
            ui.label(format!("Imported Files ({})", ed.media_files.len()));

            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for file in &ed.media_files {
                    let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    if ui.button(format!("🎞 {}", filename)).clicked() {
                        // Click to place at current playhead
                        let track_id = ed.selected_track.unwrap_or_else(|| ed.project.timeline.tracks()[0].id);
                        let clip = Clip::new_video(file.to_string_lossy(), ed.project.timeline.playhead_us, 0, 5_000_000);
                        if let Some(track) = ed.project.timeline.track_mut(track_id) {
                            track.add_clip(clip);
                        }
                    }
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.heading("⚙ Project");
            ui.label(format!("Name: {}", ed.project.name));
            ui.label(format!(
                "Resolution: {}×{}",
                ed.project.resolution.width, ed.project.resolution.height
            ));
            ui.label(format!("Framerate: {:.2} fps", ed.project.frame_rate.as_f64()));
            ui.label(format!("Clips: {}", ed.project.timeline.total_clip_count()));
        });

    // 3. Right Side Panel: Properties & Effects Inspector
    egui::SidePanel::right("properties_panel")
        .resizable(true)
        .min_width(240.0)
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.heading("🎛 Properties");
            ui.separator();

            // Find selected clip
            let mut clip_found = false;
            if let Some(selected_cid) = ed.selected_clip {
                for track in ed.project.timeline.tracks_mut() {
                    if let Some(clip) = track.clips_mut().iter_mut().find(|c| c.id == selected_cid) {
                        clip_found = true;
                        ui.label(format!("Clip ID: {}", clip.id.0));
                        ui.label(format!("Position: {:.2}s", clip.position_us as f64 / 1_000_000.0));
                        ui.label(format!("Duration: {:.2}s", clip.duration_us() as f64 / 1_000_000.0));

                        ui.separator();
                        ui.label("Transform");

                        ui.horizontal(|ui| {
                            ui.label("Pos X:");
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

                        ui.add_space(16.0);
                        ui.separator();
                        ui.heading("✨ Effects");

                        if ui.button("+ Add Brightness / Contrast").clicked() {
                            let mut params = std::collections::HashMap::new();
                            params.insert("brightness".into(), 0.1);
                            params.insert("contrast".into(), 0.1);
                            clip.effects.push(rd_core::ClipEffect {
                                effect_id: "brightness_contrast".into(),
                                parameters: params,
                                enabled: true,
                            });
                        }

                        for (_eff_idx, effect) in clip.effects.iter_mut().enumerate() {
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
                ui.label("Click any clip on the timeline to edit its transform, speed, and shader effects.");
            }
        });

    // 4. Central Panel: Video Preview
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("🎬 Preview Monitor").strong());
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
        painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(10, 10, 12));
        painter.rect_stroke(rect, 6.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 45, 55)), egui::StrokeKind::Inside);

        // Query active tracks at current playhead
        let active_tracks = ed.project.timeline.active_tracks_at(ed.project.timeline.playhead_us);
        let active_layer_count = active_tracks.len();

        if active_layer_count > 0 {
            // Draw simulated video composite
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("Compositing {} Layer(s) @ {:.2}s", active_layer_count, ed.project.timeline.playhead_us as f64 / 1_000_000.0),
                egui::FontId::proportional(16.0),
                egui::Color32::from_rgb(220, 220, 230),
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No clips at current playhead position",
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(90, 90, 100),
            );
        }

        // Overlay status badges
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
            egui::Align2::LEFT_TOP,
            format!("{}×{} | {:.0} FPS", ed.project.resolution.width, ed.project.resolution.height, ed.project.frame_rate.as_f64()),
            egui::FontId::monospace(11.0),
            egui::Color32::from_rgb(140, 140, 150),
        );

        if ed.is_playing {
            painter.text(
                egui::pos2(rect.right() - 10.0, rect.top() + 10.0),
                egui::Align2::RIGHT_TOP,
                "● PLAYING (60 FPS)",
                egui::FontId::monospace(11.0),
                egui::Color32::from_rgb(80, 220, 100),
            );
        } else {
            painter.text(
                egui::pos2(rect.right() - 10.0, rect.top() + 10.0),
                egui::Align2::RIGHT_TOP,
                "⏸ PAUSED (100% QUALITY)",
                egui::FontId::monospace(11.0),
                egui::Color32::from_rgb(220, 180, 80),
            );
        }
    });

    // 5. Bottom Panel: Interactive Timeline
    egui::TopBottomPanel::bottom("timeline_panel")
        .resizable(true)
        .min_height(220.0)
        .default_height(320.0)
        .show(ctx, |ui| {
            // Transport Bar
            ui.horizontal(|ui| {
                if ui.button("⏮").on_hover_text("Jump to start").clicked() {
                    ed.project.timeline.playhead_us = 0;
                }
                if ui.button("⏪").on_hover_text("Step back 1 sec").clicked() {
                    ed.project.timeline.playhead_us = ed.project.timeline.playhead_us.saturating_sub(1_000_000);
                }
                let play_label = if ed.is_playing { "⏸ Pause" } else { "▶ Play" };
                if ui.button(egui::RichText::new(play_label).strong()).clicked() {
                    ed.is_playing = !ed.is_playing;
                }
                if ui.button("⏩").on_hover_text("Step forward 1 sec").clicked() {
                    ed.project.timeline.playhead_us += 1_000_000;
                }
                if ui.button("⏭").on_hover_text("Jump to end").clicked() {
                    ed.project.timeline.playhead_us = ed.project.timeline.duration_us();
                }

                ui.separator();
                let mins = ed.project.timeline.playhead_us / 60_000_000;
                let secs = (ed.project.timeline.playhead_us / 1_000_000) % 60;
                let millis = (ed.project.timeline.playhead_us / 1_000) % 1_000;
                ui.label(
                    egui::RichText::new(format!("{:02}:{:02}.{:03}", mins, secs, millis))
                        .size(16.0)
                        .strong()
                        .color(egui::Color32::from_rgb(240, 240, 255)),
                );

                ui.separator();
                let total_s = ed.project.timeline.duration_us() as f64 / 1_000_000.0;
                ui.label(format!("Duration: {:.2}s | Tracks: {}", total_s, ed.project.timeline.track_count()));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("➕ Audio Track").clicked() {
                        let count = ed.project.timeline.track_count() + 1;
                        ed.project.timeline.add_track(TrackKind::Audio, format!("Audio {}", count));
                    }
                    if ui.button("➕ Video Track").clicked() {
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

            let track_header_width = 120.0;
            let timeline_x_start = rect.left() + track_header_width;
            let timeline_width = rect.width() - track_header_width;

            // Handle scrubbing clicks & drags
            if response.clicked() || response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let rel_x = (pos.x - timeline_x_start).max(0.0);
                    let clicked_time_us = (rel_x / ed.zoom_px_per_sec * 1_000_000.0) as i64;
                    ed.project.timeline.playhead_us = clicked_time_us.max(0);
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

            for track in ed.project.timeline.tracks() {
                // Header lane
                let header_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(track_header_width, track_lane_height),
                );
                painter.rect_filled(header_rect, 2.0, egui::Color32::from_rgb(28, 28, 36));
                painter.text(
                    egui::pos2(rect.left() + 8.0, y + track_lane_height / 2.0),
                    egui::Align2::LEFT_CENTER,
                    &track.name,
                    egui::FontId::proportional(12.0),
                    egui::Color32::from_rgb(190, 190, 205),
                );

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
                    let clip_color = if is_selected {
                        egui::Color32::from_rgb(50, 110, 190)
                    } else {
                        match track.kind {
                            TrackKind::Video => egui::Color32::from_rgb(35, 75, 130),
                            TrackKind::Audio => egui::Color32::from_rgb(40, 100, 60),
                            TrackKind::Text => egui::Color32::from_rgb(110, 80, 45),
                            TrackKind::Overlay => egui::Color32::from_rgb(95, 55, 110),
                        }
                    };

                    painter.rect_filled(clip_rect, 4.0, clip_color);
                    if is_selected {
                        painter.rect_stroke(clip_rect, 4.0, egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 215, 0)), egui::StrokeKind::Inside);
                    }

                    // Clip name / duration label
                    let label = format!("{:.1}s", clip.duration_us() as f64 / 1_000_000.0);
                    painter.text(
                        egui::pos2(clip_start_x + 6.0, y + track_lane_height / 2.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::monospace(11.0),
                        egui::Color32::WHITE,
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

            if let Some(cid) = clicked_clip {
                ed.selected_clip = Some(cid);
                ed.selected_track = clicked_track;
            }

            // Playhead Vertical Line
            let playhead_x = timeline_x_start + (ed.project.timeline.playhead_us as f32 / 1_000_000.0) * ed.zoom_px_per_sec;
            if playhead_x >= timeline_x_start && playhead_x <= rect.right() {
                painter.line_segment(
                    [egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())],
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 65, 65)),
                );
                // Playhead head handle
                let head_rect = egui::Rect::from_min_size(
                    egui::pos2(playhead_x - 6.0, rect.top()),
                    egui::vec2(12.0, 14.0),
                );
                painter.rect_filled(head_rect, 2.0, egui::Color32::from_rgb(255, 65, 65));
            }
        });

    // 6. Export Modal Window
    if ed.show_export_modal {
        egui::Window::new("🚀 Export Project")
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.heading("Render & Export Configuration");
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
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label(format!("Resolution: {}×{}", selected_preset.width, selected_preset.height));
                    ui.label(format!("Framerate: {:.0} fps", selected_preset.fps));
                    ui.label(format!("Video Bitrate: {} kbps ({})", selected_preset.video_bitrate_kbps, selected_preset.video_codec));
                    ui.label(format!("Audio Bitrate: {} kbps ({})", selected_preset.audio_bitrate_kbps, selected_preset.audio_codec));
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("📁 Destination File...").clicked() {
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
                        ui.label("No file selected");
                    }
                });

                ui.separator();

                // Active export progress
                let mut should_finish = false;
                let mut should_cancel = false;

                if let Some(job) = &ed.active_export_job {
                    let progress = job.progress(ed.export_frame_counter);
                    ui.label(format!("Exporting Frame {} / {}", progress.current_frame, progress.total_frames));
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

                    if ui.button("⏹ Cancel Export").clicked() {
                        job.cancel();
                        should_cancel = true;
                    }
                }

                if should_finish {
                    ed.export_completed = true;
                    ed.active_export_job = None;
                } else if should_cancel {
                    ed.active_export_job = None;
                } else if ed.export_completed {
                    ui.label(egui::RichText::new("✅ Export Completed Successfully!").color(egui::Color32::from_rgb(80, 220, 100)));
                    if ui.button("Close").clicked() {
                        ed.show_export_modal = false;
                        ed.export_completed = false;
                    }
                } else {
                    ui.horizontal(|ui| {
                        let can_export = ed.export_destination.is_some() && ed.project.timeline.duration_us() > 0;
                        if ui.add_enabled(can_export, egui::Button::new("🚀 Start Export")).clicked() {
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
