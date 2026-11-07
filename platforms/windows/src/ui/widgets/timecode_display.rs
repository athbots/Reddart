//! Recessed LCD Timecode & Numeric Display Widgets for Reddart Desktop.
//!
//! Renders high-contrast, tabular Consolas timecode displays with recessed
//! hardware wells and semantic color coding.

use crate::ui::typography;
use egui::{Color32, Response, Sense, Stroke, Ui, Widget, vec2};

/// Precision Recessed LCD Timecode Box.
pub struct LcdTimecodeBox<'a> {
    tag: &'a str,
    timecode: &'a str,
    color: Color32,
    width: f32,
    tooltip: Option<&'a str>,
}

impl<'a> LcdTimecodeBox<'a> {
    pub fn new(tag: &'a str, timecode: &'a str) -> Self {
        Self {
            tag,
            timecode,
            color: Color32::from_rgb(56, 189, 248), // Precision studio cyan
            width: 140.0,
            tooltip: None,
        }
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn tooltip(mut self, tooltip: &'a str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }
}

impl<'a> Widget for LcdTimecodeBox<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(self.width, 26.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if let Some(tt) = self.tooltip {
            response = response.on_hover_text(tt);
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Recessed well fill
            painter.rect_filled(rect, 2.5, Color32::from_rgb(8, 9, 12));

            // Inner shadow stroke
            painter.rect_stroke(
                rect,
                2.5,
                Stroke::new(1.0_f32, Color32::from_rgb(30, 34, 46)),
                egui::StrokeKind::Inside,
            );

            // Left tag label
            if !self.tag.is_empty() {
                painter.text(
                    egui::pos2(rect.left() + 6.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    self.tag,
                    typography::font_micro_numeric(),
                    Color32::from_rgb(100, 110, 130),
                );
            }

            // Right-aligned tabular timecode
            painter.text(
                egui::pos2(rect.right() - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                self.timecode,
                typography::font_numeric(),
                self.color,
            );
        }

        response
    }
}

/// Helper function for quickly drawing an LCD timecode box.
pub fn studio_lcd_box(ui: &mut Ui, tag: &str, timecode: &str, color: Color32) -> Response {
    ui.add(LcdTimecodeBox::new(tag, timecode).color(color))
}
