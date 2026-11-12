//! Precision Real-Time Spectrum Analyzer & Parametric EQ Visualizer Widget.
//!
//! Renders dual-layer visualization:
//! 1. High-resolution Real-Time Spectrum Analyzer (RTA) with 31-band ISO bars, peak-hold needles,
//!    and continuous logarithmic glowing spectral fill.
//! 2. Parametric EQ transfer function curve with interactive draggable control nodes
//!    for frequency ($f_0$), gain ($\pm 18 \text{ dB}$), and bandwidth ($Q$).

use egui::{Color32, FontId, Pos2, Rect, Stroke, Ui, pos2, vec2};
use rd_audio::eq::{FilterType, ParametricEqualizer};
use rd_audio::spectrum::{ISO_31_FREQUENCIES, SpectrumAnalyzer};

/// Visual styling and interaction state for the Spectrum & EQ Visualizer.
pub struct SpectrumEqVisualizer<'a> {
    eq: &'a mut ParametricEqualizer,
    spectrum_bands: &'a [f32; 31],
    spectrum_curve: &'a [f32],
    height: f32,
    selected_band: &'a mut Option<usize>,
}

impl<'a> SpectrumEqVisualizer<'a> {
    pub fn new(
        eq: &'a mut ParametricEqualizer,
        spectrum_bands: &'a [f32; 31],
        spectrum_curve: &'a [f32],
        selected_band: &'a mut Option<usize>,
    ) -> Self {
        Self {
            eq,
            spectrum_bands,
            spectrum_curve,
            height: 180.0,
            selected_band,
        }
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Color assignment for individual EQ bands.
    pub fn band_color(index: usize) -> Color32 {
        match index {
            0 => Color32::from_rgb(14, 165, 233),  // Sky Blue (Low Cut)
            1 => Color32::from_rgb(16, 185, 129),  // Emerald (Low Shelf)
            2 => Color32::from_rgb(245, 158, 11),  // Amber (Parametric Mid)
            3 => Color32::from_rgb(168, 85, 247),  // Purple (High Shelf)
            4 => Color32::from_rgb(244, 63, 94),   // Rose (High Cut)
            _ => Color32::from_rgb(209, 213, 219), // Neutral
        }
    }

    /// Render interactive widget to egui Ui.
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let available_w = ui.available_width().max(240.0);
        let (rect, mut response) = ui.allocate_exact_size(
            vec2(available_w, self.height),
            egui::Sense::click_and_drag(),
        );

        let painter = ui.painter_at(rect);

        // 1. Machined graphite chassis background
        painter.rect_filled(rect, 4.0, Color32::from_rgb(12, 14, 18));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, Color32::from_rgb(34, 38, 50)),
            egui::StrokeKind::Inside,
        );

        let graph_rect = rect.shrink2(vec2(8.0, 8.0));
        let gw = graph_rect.width();
        let gh = graph_rect.height();

        // 2. Frequency Grid Lines & Numerical Labels
        let freq_marks: &[(f32, &str)] = &[
            (50.0, "50"),
            (100.0, "100"),
            (250.0, "250"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2500.0, "2.5k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
            (20000.0, "20k"),
        ];

        for &(f, label) in freq_marks {
            let frac = SpectrumAnalyzer::freq_to_fraction(f);
            let x = graph_rect.left() + frac * gw;
            painter.line_segment(
                [pos2(x, graph_rect.top()), pos2(x, graph_rect.bottom())],
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 12)),
            );
            painter.text(
                pos2(x, graph_rect.bottom() - 10.0),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.0),
                Color32::from_rgba_unmultiplied(156, 163, 175, 160),
            );
        }

        // 3. Decibel Grid Lines & Labels (+12 dB to -18 dB)
        let db_marks: &[(f32, &str)] = &[
            (12.0, "+12"),
            (6.0, "+6"),
            (0.0, "0"),
            (-6.0, "-6"),
            (-12.0, "-12"),
            (-18.0, "-18"),
        ];

        for &(db, label) in db_marks {
            let y_frac = (18.0 - db) / 36.0;
            let y = graph_rect.top() + y_frac.clamp(0.0, 1.0) * gh;
            let stroke = if db == 0.0 {
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 45))
            } else {
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 12))
            };
            painter.line_segment(
                [pos2(graph_rect.left(), y), pos2(graph_rect.right(), y)],
                stroke,
            );
            painter.text(
                pos2(graph_rect.left() + 14.0, y),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace(8.0),
                Color32::from_rgba_unmultiplied(156, 163, 175, 140),
            );
        }

        // 4. Real-Time Spectrum Analyzer RTA Fill / Bars
        if !self.spectrum_curve.is_empty() {
            let num_pts = self.spectrum_curve.len();
            let mut fill_pts = Vec::with_capacity(num_pts + 2);
            fill_pts.push(pos2(graph_rect.left(), graph_rect.bottom()));

            for (i, &dbfs) in self.spectrum_curve.iter().enumerate() {
                let frac_x = i as f32 / ((num_pts - 1) as f32);
                let x = graph_rect.left() + frac_x * gw;

                // Map -72 dBFS .. 0 dBFS to graph_rect height
                let norm_y = (-dbfs).clamp(0.0, 72.0) / 72.0;
                let y = graph_rect.top() + norm_y * gh;
                fill_pts.push(pos2(x, y));
            }
            fill_pts.push(pos2(graph_rect.right(), graph_rect.bottom()));

            if fill_pts.len() >= 3 {
                painter.add(egui::Shape::convex_polygon(
                    fill_pts,
                    Color32::from_rgba_unmultiplied(6, 182, 212, 38), // Neon cyan translucent aura
                    Stroke::NONE,
                ));
            }
        }

        // 31-Band Peak Caps
        for (idx, &f_c) in ISO_31_FREQUENCIES.iter().enumerate() {
            let dbfs = self.spectrum_bands[idx];
            if dbfs > -70.0 {
                let frac_x = SpectrumAnalyzer::freq_to_fraction(f_c);
                let x = graph_rect.left() + frac_x * gw;
                let norm_y = (-dbfs).clamp(0.0, 72.0) / 72.0;
                let y = graph_rect.top() + norm_y * gh;

                painter.line_segment(
                    [pos2(x - 2.5, y), pos2(x + 2.5, y)],
                    Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(103, 232, 249, 180)),
                );
            }
        }

        // 5. Parametric EQ Analytical Transfer Function Composite Curve
        let curve_points = 128;
        let composite = self.eq.compute_composite_curve(curve_points);
        let mut curve_coords: Vec<Pos2> = Vec::with_capacity(curve_points);

        for (i, &db) in composite.iter().enumerate() {
            let frac_x = i as f32 / ((curve_points - 1) as f32);
            let x = graph_rect.left() + frac_x * gw;
            let y_frac = (18.0 - db.clamp(-18.0, 18.0)) / 36.0;
            let y = graph_rect.top() + y_frac * gh;
            curve_coords.push(pos2(x, y));
        }

        let eq_stroke_color = if self.eq.enabled {
            Color32::from_rgb(245, 158, 11) // Luminous Gold
        } else {
            Color32::from_rgb(107, 114, 128) // Bypassed Slate Gray
        };

        if curve_coords.len() >= 2 {
            painter.add(egui::Shape::line(
                curve_coords,
                Stroke::new(2.5_f32, eq_stroke_color),
            ));
        }

        // 6. Interactive EQ Band Node Handles
        let mut dragging_band = None;
        let mut hovered_band = None;
        let mut reset_band_idx = None;

        for (band_idx, band) in self.eq.bands.iter().enumerate() {
            let frac_x = SpectrumAnalyzer::freq_to_fraction(band.frequency_hz);
            let x = graph_rect.left() + frac_x * gw;

            let y_gain = match band.filter_type {
                FilterType::LowCut | FilterType::HighCut => 0.0,
                _ => band.gain_db,
            };
            let y_frac = (18.0 - y_gain.clamp(-18.0, 18.0)) / 36.0;
            let y = graph_rect.top() + y_frac * gh;
            let node_pos = pos2(x, y);

            let node_radius = 6.5;
            let node_rect =
                Rect::from_center_size(node_pos, vec2(node_radius * 2.0, node_radius * 2.0));

            let is_hovered = ui.rect_contains_pointer(node_rect);
            if is_hovered {
                hovered_band = Some(band_idx);
            }

            let is_selected = *self.selected_band == Some(band_idx);

            let color = if band.enabled {
                Self::band_color(band_idx)
            } else {
                Color32::from_rgb(75, 85, 99)
            };

            // Draw outer glow and node circle
            if is_selected || is_hovered {
                painter.circle_stroke(node_pos, node_radius + 3.0, Stroke::new(1.5_f32, color));
            }
            painter.circle_filled(node_pos, node_radius, color);
            painter.circle_filled(node_pos, node_radius - 2.5, Color32::from_rgb(12, 14, 18));
            painter.text(
                node_pos,
                egui::Align2::CENTER_CENTER,
                format!("{}", band_idx + 1),
                FontId::proportional(8.0),
                Color32::WHITE,
            );

            // Node Dragging Interaction
            if response.dragged() && response.interact_pointer_pos().is_some() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if is_selected || is_hovered {
                        dragging_band = Some((band_idx, pos));
                    }
                }
            }

            // Click selection
            if response.clicked() && is_hovered {
                *self.selected_band = Some(band_idx);
                response.mark_changed();
            }

            // Double click to reset gain
            if response.double_clicked() && is_hovered {
                reset_band_idx = Some(band_idx);
                response.mark_changed();
            }
        }

        if let Some(idx) = reset_band_idx {
            if let Some(band) = self.eq.bands.get_mut(idx) {
                band.gain_db = 0.0;
                band.recompute(self.eq.sample_rate);
            }
        }

        // Process active dragging
        if let Some((idx, mouse_pos)) = dragging_band {
            *self.selected_band = Some(idx);
            let rel_x = ((mouse_pos.x - graph_rect.left()) / gw).clamp(0.0, 1.0);
            let rel_y = ((mouse_pos.y - graph_rect.top()) / gh).clamp(0.0, 1.0);

            let new_freq = SpectrumAnalyzer::fraction_to_freq(rel_x);
            let new_gain = 18.0 - rel_y * 36.0;

            if let Some(band) = self.eq.bands.get_mut(idx) {
                band.frequency_hz = new_freq.clamp(20.0, 20000.0);
                if band.filter_type != FilterType::LowCut && band.filter_type != FilterType::HighCut
                {
                    band.gain_db = (new_gain * 2.0).round() / 2.0; // 0.5 dB step snapping
                }
                band.recompute(self.eq.sample_rate);
                response.mark_changed();
            }
        }

        // Tooltip for hovered band
        if let Some(idx) = hovered_band {
            let band = &self.eq.bands[idx];
            let filter_name = match band.filter_type {
                FilterType::LowCut => "Low Cut (HPF)",
                FilterType::LowShelf => "Low Shelf",
                FilterType::Bell => "Parametric Bell",
                FilterType::HighShelf => "High Shelf",
                FilterType::HighCut => "High Cut (LPF)",
                FilterType::Notch => "Notch Filter",
            };

            let freq_text = if band.frequency_hz >= 1000.0 {
                format!("{:.2} kHz", band.frequency_hz / 1000.0)
            } else {
                format!("{:.0} Hz", band.frequency_hz)
            };

            let tooltip = format!(
                "Band {} [{}]: {}\nGain: {:+.1} dB | Q: {:.2}\n(Drag to shape, double-click to zero)",
                idx + 1,
                filter_name,
                freq_text,
                band.gain_db,
                band.q
            );
            response = response.on_hover_text(tooltip);
        }

        response
    }
}

impl egui::Widget for SpectrumEqVisualizer<'_> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        self.show(ui)
    }
}
