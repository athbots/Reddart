//! Native Windows Typography System for Reddart Desktop.
//!
//! Loads Segoe UI for interface labels and Consolas for precision timecode/numeric data.
//! Falls back gracefully to bundled fonts if Windows font paths are unavailable.

use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// Configures native Windows fonts on the egui Context.
pub fn setup_windows_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // 1. Attempt to load Segoe UI (Windows primary desktop UI font)
    let segoe_paths = [
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\SegoeUI.ttf",
    ];

    let mut loaded_segoe = false;
    for path in &segoe_paths {
        if let Ok(font_bytes) = std::fs::read(path) {
            log::info!("Loaded native Windows UI font from: {}", path);
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                Arc::new(FontData::from_owned(font_bytes)),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "segoe_ui".to_owned());
            loaded_segoe = true;
            break;
        }
    }

    if !loaded_segoe {
        log::warn!("Native Segoe UI font not found; using egui proportional fallback.");
    }

    // 2. Attempt to load Consolas (Windows precision tabular monospace font)
    let consolas_paths = [
        "C:\\Windows\\Fonts\\consola.ttf",
        "C:\\Windows\\Fonts\\Consola.ttf",
        "C:\\Windows\\Fonts\\consolas.ttf",
    ];

    let mut loaded_consolas = false;
    for path in &consolas_paths {
        if let Ok(font_bytes) = std::fs::read(path) {
            log::info!("Loaded native Windows Monospace font from: {}", path);
            fonts.font_data.insert(
                "consolas".to_owned(),
                Arc::new(FontData::from_owned(font_bytes)),
            );
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .insert(0, "consolas".to_owned());
            loaded_consolas = true;
            break;
        }
    }

    if !loaded_consolas {
        log::warn!("Native Consolas font not found; using egui monospace fallback.");
    }

    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------------------
// Standard Optical Typographic Scale Helpers
// ---------------------------------------------------------------------------

/// Section header & dialog title font (13.0 pt Proportional).
#[inline]
pub fn font_title() -> FontId {
    FontId::proportional(13.0)
}

/// Panel header & prominent label font (11.5 pt Proportional).
#[inline]
pub fn font_heading() -> FontId {
    FontId::proportional(11.5)
}

/// Standard interface body & button font (11.0 pt Proportional).
#[inline]
pub fn font_body() -> FontId {
    FontId::proportional(11.0)
}

/// Secondary caption & metadata font (9.5 pt Proportional).
#[inline]
pub fn font_caption() -> FontId {
    FontId::proportional(9.5)
}

/// Large tabular timecode display font (13.0 pt Monospace Consolas).
#[inline]
pub fn font_timecode() -> FontId {
    FontId::monospace(13.0)
}

/// Precision numeric property font (10.5 pt Monospace Consolas).
#[inline]
pub fn font_numeric() -> FontId {
    FontId::monospace(10.5)
}

/// Micro numeric & meter scale font (8.5 pt Monospace Consolas).
#[inline]
pub fn font_micro_numeric() -> FontId {
    FontId::monospace(8.5)
}
