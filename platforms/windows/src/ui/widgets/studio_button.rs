//! Precision Industrial Studio Button Widgets for Reddart Desktop.
//!
//! Provides tactile, machined buttons with GPU-rendered vector icons,
//! keyboard shortcut badges, smooth hover feedback, and active states.

use crate::ui::typography;
use egui::{Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget, vec2};

/// Callback function type for procedural vector icon painting.
pub type IconDrawer<'a> = Box<dyn Fn(&Painter, Rect, Color32) + 'a>;

/// Tactile Studio Push Button.
pub struct StudioButton<'a> {
    label: &'a str,
    shortcut: Option<&'a str>,
    accent: Option<Color32>,
    active: bool,
    disabled: bool,
    min_size: Option<egui::Vec2>,
    icon_drawer: Option<IconDrawer<'a>>,
    tooltip: Option<&'a str>,
}

impl<'a> StudioButton<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            shortcut: None,
            accent: None,
            active: false,
            disabled: false,
            min_size: None,
            icon_drawer: None,
            tooltip: None,
        }
    }

    pub fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    pub fn accent(mut self, accent: Color32) -> Self {
        self.accent = Some(accent);
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn min_size(mut self, size: egui::Vec2) -> Self {
        self.min_size = Some(size);
        self
    }

    pub fn icon<F>(mut self, drawer: F) -> Self
    where
        F: Fn(&Painter, Rect, Color32) + 'a,
    {
        self.icon_drawer = Some(Box::new(drawer));
        self
    }

    pub fn tooltip(mut self, text: &'a str) -> Self {
        self.tooltip = Some(text);
        self
    }
}

impl<'a> Widget for StudioButton<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let padding = vec2(8.0, 4.0);
        let font_id = typography::font_body();
        let text_layout =
            ui.painter()
                .layout_no_wrap(self.label.to_owned(), font_id.clone(), Color32::WHITE);

        let icon_width = if self.icon_drawer.is_some() {
            16.0
        } else {
            0.0
        };
        let icon_spacing = if self.icon_drawer.is_some() && !self.label.is_empty() {
            5.0
        } else {
            0.0
        };

        let shortcut_width = if let Some(sc) = self.shortcut {
            let sc_layout = ui.painter().layout_no_wrap(
                sc.to_owned(),
                typography::font_caption(),
                Color32::GRAY,
            );
            sc_layout.size().x + 8.0
        } else {
            0.0
        };

        let content_width = icon_width + icon_spacing + text_layout.size().x + shortcut_width;
        let mut desired_size = vec2(
            content_width + padding.x * 2.0,
            (text_layout.size().y + padding.y * 2.0).max(24.0),
        );

        if let Some(min) = self.min_size {
            desired_size.x = desired_size.x.max(min.x);
            desired_size.y = desired_size.y.max(min.y);
        }

        let sense = if self.disabled {
            Sense::hover()
        } else {
            Sense::click()
        };

        let (rect, mut response) = ui.allocate_exact_size(desired_size, sense);

        if let Some(tt) = self.tooltip {
            response = response.on_hover_text(tt);
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let is_hovered = response.hovered() && !self.disabled;
            let is_pressed = response.is_pointer_button_down_on() && !self.disabled;

            // Background Fill
            let bg_fill = if self.disabled {
                Color32::from_rgb(20, 22, 28)
            } else if self.active {
                if let Some(acc) = self.accent {
                    acc
                } else {
                    Color32::from_rgb(37, 99, 235) // Studio blue
                }
            } else if is_pressed {
                Color32::from_rgb(18, 20, 26) // Recessed
            } else if is_hovered {
                Color32::from_rgb(44, 50, 66) // Soft illumination
            } else if let Some(acc) = self.accent {
                // Subtle tint of accent color for idle state
                Color32::from_rgb(
                    (acc.r() as f32 * 0.15 + 24.0 * 0.85) as u8,
                    (acc.g() as f32 * 0.15 + 26.0 * 0.85) as u8,
                    (acc.b() as f32 * 0.15 + 34.0 * 0.85) as u8,
                )
            } else {
                Color32::from_rgb(28, 32, 42) // Machined graphite idle
            };

            painter.rect_filled(rect, 2.5, bg_fill);

            // Border Stroke
            let border_stroke = if self.disabled {
                Stroke::new(1.0_f32, Color32::from_rgb(34, 38, 48))
            } else if self.active {
                Stroke::new(1.0_f32, Color32::from_rgb(147, 197, 253))
            } else if is_hovered {
                Stroke::new(1.0_f32, Color32::from_rgb(96, 165, 250))
            } else if let Some(acc) = self.accent {
                Stroke::new(1.0_f32, acc)
            } else {
                Stroke::new(1.0_f32, Color32::from_rgb(48, 54, 70))
            };

            painter.rect_stroke(rect, 2.5, border_stroke, egui::StrokeKind::Inside);

            // Subtle top highlight sheen (machined edge bevel)
            if !self.disabled && !is_pressed && !self.active {
                let sheen_y = rect.top() + 1.0;
                painter.line_segment(
                    [
                        egui::pos2(rect.left() + 2.5, sheen_y),
                        egui::pos2(rect.right() - 2.5, sheen_y),
                    ],
                    Stroke::new(0.8_f32, Color32::from_white_alpha(25)),
                );
            }

            // Foreground Colors
            let fg_color = if self.disabled {
                Color32::from_rgb(88, 96, 112)
            } else if self.active || is_hovered {
                Color32::WHITE
            } else {
                Color32::from_rgb(226, 232, 240)
            };

            // Draw Icon
            let mut cursor_x = rect.left() + padding.x;
            let center_y = rect.center().y;

            if let Some(ref drawer) = self.icon_drawer {
                let icon_rect = Rect::from_center_size(
                    egui::pos2(cursor_x + icon_width * 0.5, center_y),
                    vec2(icon_width, icon_width),
                );
                drawer(painter, icon_rect, fg_color);
                cursor_x += icon_width + icon_spacing;
            }

            // Draw Label
            if !self.label.is_empty() {
                painter.text(
                    egui::pos2(cursor_x, center_y),
                    egui::Align2::LEFT_CENTER,
                    self.label,
                    font_id,
                    fg_color,
                );
            }

            // Draw Shortcut Badge
            if let Some(sc) = self.shortcut {
                let sc_color = if self.disabled {
                    Color32::from_rgb(60, 66, 80)
                } else if self.active {
                    Color32::from_rgb(191, 219, 254)
                } else {
                    Color32::from_rgb(120, 130, 150)
                };

                painter.text(
                    egui::pos2(rect.right() - padding.x, center_y),
                    egui::Align2::RIGHT_CENTER,
                    sc,
                    typography::font_caption(),
                    sc_color,
                );
            }
        }

        response
    }
}

/// Circular or Pill Precision Transport Button.
pub fn studio_transport_button<F>(
    ui: &mut Ui,
    drawer: F,
    size: f32,
    active: bool,
    accent: Option<Color32>,
    tooltip: &str,
) -> Response
where
    F: Fn(&Painter, Rect, Color32),
{
    let desired_size = vec2(size, size);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    let response = if !tooltip.is_empty() {
        response.on_hover_text(tooltip)
    } else {
        response
    };

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let is_hovered = response.hovered();
        let is_pressed = response.is_pointer_button_down_on();

        let bg_fill = if active {
            accent.unwrap_or(Color32::from_rgb(37, 99, 235))
        } else if is_pressed {
            Color32::from_rgb(16, 18, 24)
        } else if is_hovered {
            Color32::from_rgb(46, 52, 68)
        } else {
            Color32::from_rgb(26, 30, 40)
        };

        painter.rect_filled(rect, 3.0, bg_fill);

        let stroke_color = if active {
            Color32::from_rgb(147, 197, 253)
        } else if is_hovered {
            Color32::from_rgb(96, 165, 250)
        } else {
            Color32::from_rgb(46, 52, 68)
        };

        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0_f32, stroke_color),
            egui::StrokeKind::Inside,
        );

        let icon_color = if active || is_hovered {
            Color32::WHITE
        } else {
            Color32::from_rgb(203, 213, 225)
        };

        drawer(painter, rect, icon_color);
    }

    response
}

/// Precision Segmented Workspace Navigation Tab.
/// Modeled after DaVinci Resolve workspace deck with vector icon and active bottom illumination bar.
pub fn studio_workspace_tab<F>(ui: &mut Ui, active: bool, title: &str, icon_drawer: F) -> Response
where
    F: Fn(&Painter, Rect, Color32),
{
    let desired_size = vec2(ui.available_width().clamp(82.0, 102.0), 28.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let is_hovered = response.hovered();

        let bg = if active {
            Color32::from_rgb(28, 34, 48)
        } else if is_hovered {
            Color32::from_rgb(24, 28, 38)
        } else {
            Color32::from_rgb(14, 16, 22)
        };

        painter.rect_filled(rect, 2.0, bg);

        let border_color = if active {
            Color32::from_rgb(59, 130, 246)
        } else if is_hovered {
            Color32::from_rgb(64, 72, 92)
        } else {
            Color32::from_rgb(32, 36, 48)
        };

        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0_f32, border_color),
            egui::StrokeKind::Inside,
        );

        // Active bottom illumination bar
        if active {
            let bar_rect = Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - 2.5),
                vec2(rect.width(), 2.5),
            );
            painter.rect_filled(bar_rect, 0.0, Color32::from_rgb(59, 130, 246));
        }

        let fg_color = if active {
            Color32::WHITE
        } else if is_hovered {
            Color32::from_rgb(226, 232, 240)
        } else {
            Color32::from_rgb(148, 163, 184)
        };

        // Left vector icon
        let icon_box = Rect::from_center_size(
            egui::pos2(rect.left() + 14.0, rect.center().y),
            vec2(16.0, 16.0),
        );
        icon_drawer(painter, icon_box, fg_color);

        // Text label
        painter.text(
            egui::pos2(rect.left() + 26.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            typography::font_heading(),
            fg_color,
        );
    }

    response
}
