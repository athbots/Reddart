//! Reddart Desktop UI Module.
//!
//! Provides native Windows typography, procedural vector iconography,
//! centralized graphite theme tokens, and custom precision widgets.

pub mod icons;
pub mod theme;
pub mod typography;
pub mod widgets;

pub use theme::StudioTheme;

/// Initialize fonts and apply studio theme tokens to egui.
pub fn init_studio_ui(ctx: &egui::Context) -> StudioTheme {
    typography::setup_windows_fonts(ctx);
    let theme = StudioTheme::default();
    theme::apply_studio_theme(ctx, &theme);
    theme
}
