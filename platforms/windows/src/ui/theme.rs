//! Studio Theme & Design System Tokens for Reddart Desktop.
//!
//! Provides an industrial dark graphite/charcoal design system engineered
//! for high-density professional NLE workflows (DaVinci Resolve / Premiere Pro standard).

use egui::{Color32, Stroke};

/// Centralized Studio Design System Tokens.
#[derive(Debug, Clone)]
pub struct StudioTheme {
    // Background Surfaces
    pub bg_canvas: Color32,
    pub bg_app: Color32,
    pub bg_panel: Color32,
    pub bg_panel_elevated: Color32,
    pub bg_panel_header: Color32,
    pub bg_recessed: Color32,

    // Structural Borders & Dividers
    pub border_subtle: Color32,
    pub border_medium: Color32,
    pub border_strong: Color32,
    pub border_focus: Color32,
    pub border_active: Color32,

    // Typography Text Colors
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub text_accent: Color32,

    // Functional & Semantic Accents
    pub accent_primary: Color32,
    pub accent_primary_hover: Color32,
    pub accent_primary_pressed: Color32,
    pub accent_warning: Color32,
    pub accent_danger: Color32,
    pub accent_success: Color32,
    pub accent_neutral: Color32,

    // Timeline Media Clip Colors
    pub clip_video: Color32,
    pub clip_video_border: Color32,
    pub clip_audio: Color32,
    pub clip_audio_border: Color32,
    pub clip_title: Color32,
    pub clip_title_border: Color32,
    pub clip_selected_border: Color32,

    // Interactive Widget Metrics
    pub rounding_small: f32,
    pub rounding_medium: f32,
    pub button_height_sm: f32,
    pub button_height_md: f32,
    pub header_height: f32,
    pub timecode_box_height: f32,
}

impl Default for StudioTheme {
    fn default() -> Self {
        Self {
            // Background Surfaces
            bg_canvas: Color32::from_rgb(14, 15, 18), // #0E0F12 Deepest background
            bg_app: Color32::from_rgb(18, 20, 24),    // #121418 Master window surface
            bg_panel: Color32::from_rgb(24, 27, 34),  // #181B22 Structural panels
            bg_panel_elevated: Color32::from_rgb(32, 36, 46), // #20242E Raised decks & toolbars
            bg_panel_header: Color32::from_rgb(20, 22, 28), // #14161C Panel headers & tabs
            bg_recessed: Color32::from_rgb(9, 10, 13), // #090A0D Timeline canvas / LCD wells

            // Structural Borders & Dividers
            border_subtle: Color32::from_rgb(38, 42, 54), // #262A36 Inset panel dividers
            border_medium: Color32::from_rgb(52, 58, 74), // #343A4A Inactive button strokes
            border_strong: Color32::from_rgb(72, 80, 102), // #485066 Hovered borders
            border_focus: Color32::from_rgb(59, 130, 246), // #3B82F6 Keyboard focus ring
            border_active: Color32::from_rgb(96, 165, 250), // #60A5FA Selection outline

            // Typography Text Colors
            text_primary: Color32::from_rgb(230, 234, 242), // #E6EAF2 High-contrast labels
            text_secondary: Color32::from_rgb(156, 166, 184), // #9CA6B8 Secondary captions
            text_dim: Color32::from_rgb(100, 110, 130),     // #646E82 Watermarks, shortcuts
            text_disabled: Color32::from_rgb(68, 74, 90),   // #444A5A Disabled items
            text_accent: Color32::from_rgb(56, 189, 248),   // #38BDF8 Highlight blue/cyan

            // Functional & Semantic Accents
            accent_primary: Color32::from_rgb(37, 99, 235), // #2563EB Studio Blue (Selection/Play)
            accent_primary_hover: Color32::from_rgb(59, 130, 246), // #3B82F6
            accent_primary_pressed: Color32::from_rgb(29, 78, 216), // #1D4ED8
            accent_warning: Color32::from_rgb(217, 119, 6), // #D97706 Studio Amber (Caution)
            accent_danger: Color32::from_rgb(220, 38, 38),  // #DC2626 Studio Crimson (Razor/Delete)
            accent_success: Color32::from_rgb(16, 185, 129), // #10B981 Studio Emerald (Export/Safe)
            accent_neutral: Color32::from_rgb(44, 50, 64),  // #2C3240 Default button fill

            // Timeline Media Clip Colors
            clip_video: Color32::from_rgb(38, 64, 102), // #264066 Video track blue-slate
            clip_video_border: Color32::from_rgb(64, 104, 160), // #4068A0
            clip_audio: Color32::from_rgb(30, 84, 70),  // #1E5446 Audio track deep teal
            clip_audio_border: Color32::from_rgb(48, 136, 112), // #308870
            clip_title: Color32::from_rgb(78, 52, 96),  // #4E3460 Title track muted purple
            clip_title_border: Color32::from_rgb(124, 86, 154), // #7C569A
            clip_selected_border: Color32::from_rgb(245, 158, 11), // #F59E0B Active Amber Outline

            // Interactive Widget Metrics
            rounding_small: 2.5,
            rounding_medium: 4.0,
            button_height_sm: 24.0,
            button_height_md: 28.0,
            header_height: 30.0,
            timecode_box_height: 26.0,
        }
    }
}

/// Applies the studio theme styling directly to the egui context.
pub fn apply_studio_theme(ctx: &egui::Context, theme: &StudioTheme) {
    let mut style = (*ctx.style()).clone();

    // Visuals configuration
    style.visuals.dark_mode = true;
    style.visuals.override_text_color = Some(theme.text_primary);
    style.visuals.panel_fill = theme.bg_app;
    style.visuals.window_fill = theme.bg_panel;
    style.visuals.extreme_bg_color = theme.bg_recessed;
    style.visuals.window_stroke = Stroke::new(1.0_f32, theme.border_medium);

    // Inactive widget styling (calm, restrained)
    style.visuals.widgets.inactive.bg_fill = theme.accent_neutral;
    style.visuals.widgets.inactive.weak_bg_fill = theme.accent_neutral;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, theme.border_subtle);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, theme.text_primary);
    style.visuals.widgets.inactive.corner_radius =
        egui::CornerRadius::same(theme.rounding_small as u8);

    // Hovered widget styling (subtle illumination glow)
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 60, 78);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 60, 78);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, theme.border_strong);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    style.visuals.widgets.hovered.corner_radius =
        egui::CornerRadius::same(theme.rounding_small as u8);

    // Active/Pressed widget styling (tactile feedback)
    style.visuals.widgets.active.bg_fill = theme.accent_primary;
    style.visuals.widgets.active.weak_bg_fill = theme.accent_primary;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, theme.border_active);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    style.visuals.widgets.active.corner_radius =
        egui::CornerRadius::same(theme.rounding_small as u8);

    // Open/Expanded dropdown styling
    style.visuals.widgets.open.bg_fill = theme.bg_panel_elevated;
    style.visuals.widgets.open.weak_bg_fill = theme.bg_panel_elevated;
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, theme.border_focus);
    style.visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

    // Non-interactive surfaces
    style.visuals.widgets.noninteractive.bg_fill = theme.bg_panel;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, theme.border_subtle);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, theme.text_secondary);

    // Spacing & Layout
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(6);
    style.spacing.menu_margin = egui::Margin::same(4);

    ctx.set_style(style);
}
