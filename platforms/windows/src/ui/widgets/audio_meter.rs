//! Precision Dual-Channel Hardware dBFS Ladder Audio Meter.
//!
//! Provides broadcast-standard True Peak and RMS decibel metering
//! with segmented LED ballistics, peak-hold needles, clip detection,
//! and Consolas tabular readout.

use egui::{Color32, FontId, Painter, Rect, Stroke, Ui, pos2, vec2};
use rd_audio::MeteringState;

/// Convert a decibel value (-60.0 to +3.0) to a normalized fraction (0.0 to 1.0).
pub fn dbfs_to_fraction(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else if db >= 3.0 {
        1.0
    } else {
        (db + 60.0) / 63.0
    }
}

/// Broadcast-standard color for a given decibel level.
pub fn color_for_dbfs(db: f32) -> Color32 {
    if db >= -3.0 {
        Color32::from_rgb(239, 68, 68) // Crimson Overload / Clip
    } else if db >= -18.0 {
        Color32::from_rgb(245, 158, 11) // Amber Transients / Headroom
    } else {
        Color32::from_rgb(16, 185, 129) // Emerald Nominal Program
    }
}

/// Precision Dual-Channel Hardware Audio Meter Widget.
pub struct StudioAudioMeter {
    metering: MeteringState,
    height: f32,
    show_labels: bool,
}

impl StudioAudioMeter {
    pub fn new(metering: MeteringState) -> Self {
        Self {
            metering,
            height: 28.0,
            show_labels: true,
        }
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn show_labels(mut self, show: bool) -> Self {
        self.show_labels = show;
        self
    }

    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let available_w = ui.available_width().max(160.0);
        let (rect, response) =
            ui.allocate_exact_size(vec2(available_w, self.height), egui::Sense::hover());

        let painter = ui.painter_at(rect);

        // Machined recessed dark graphite bezel
        painter.rect_filled(rect, 2.0, Color32::from_rgb(10, 11, 14));
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0_f32, Color32::from_rgb(34, 38, 50)),
            egui::StrokeKind::Inside,
        );

        let pad_x = 6.0;
        let pad_y = 4.0;
        let meter_w = rect.width() - (pad_x * 2.0);
        let channel_h = 7.0;

        let l_top = rect.top() + pad_y;
        let r_top = l_top + channel_h + 3.0;

        let l_rect =
            Rect::from_min_size(pos2(rect.left() + pad_x, l_top), vec2(meter_w, channel_h));
        let r_rect =
            Rect::from_min_size(pos2(rect.left() + pad_x, r_top), vec2(meter_w, channel_h));

        // Draw track background slots
        painter.rect_filled(l_rect, 1.0, Color32::from_rgb(18, 20, 26));
        painter.rect_filled(r_rect, 1.0, Color32::from_rgb(18, 20, 26));

        // Render Left & Right Segmented Ladders
        draw_channel_ladder(
            &painter,
            l_rect,
            self.metering.peak_l_dbfs,
            self.metering.rms_l_dbfs,
            self.metering.peak_hold_l_dbfs,
            self.metering.clipped_l,
        );

        draw_channel_ladder(
            &painter,
            r_rect,
            self.metering.peak_r_dbfs,
            self.metering.rms_r_dbfs,
            self.metering.peak_hold_r_dbfs,
            self.metering.clipped_r,
        );

        // Numeric Readout Overlay
        if self.show_labels && self.height >= 26.0 {
            let label_text = format!(
                "L {:>5.1}  R {:>5.1} dBFS",
                self.metering.peak_l_dbfs, self.metering.peak_r_dbfs
            );
            painter.text(
                pos2(rect.right() - pad_x, rect.bottom() - 2.0),
                egui::Align2::RIGHT_BOTTOM,
                label_text,
                FontId::monospace(8.5),
                Color32::from_rgb(148, 163, 184),
            );

            // Channel indicators
            painter.text(
                pos2(rect.left() + 2.0, l_top + channel_h / 2.0),
                egui::Align2::RIGHT_CENTER,
                "L",
                FontId::monospace(7.5),
                Color32::from_rgb(100, 116, 139),
            );
            painter.text(
                pos2(rect.left() + 2.0, r_top + channel_h / 2.0),
                egui::Align2::RIGHT_CENTER,
                "R",
                FontId::monospace(7.5),
                Color32::from_rgb(100, 116, 139),
            );
        }

        response
    }
}

impl egui::Widget for StudioAudioMeter {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        self.show(ui)
    }
}

/// Draw a single channel's segmented LED ladder, RMS bar, peak needle, and clip indicator.
fn draw_channel_ladder(
    painter: &Painter,
    rect: Rect,
    peak_db: f32,
    rms_db: f32,
    hold_db: f32,
    clipped: bool,
) {
    let total_w = rect.width();
    let num_segments = 36usize;
    let seg_pitch = total_w / num_segments as f32;
    let seg_w = (seg_pitch - 1.0).max(1.0);

    let peak_frac = dbfs_to_fraction(peak_db);
    let rms_frac = dbfs_to_fraction(rms_db);
    let active_peak_segments = (peak_frac * num_segments as f32).round() as usize;
    let active_rms_segments = (rms_frac * num_segments as f32).round() as usize;

    for i in 0..num_segments {
        let seg_x = rect.left() + (i as f32 * seg_pitch);
        let seg_rect = Rect::from_min_size(pos2(seg_x, rect.top()), vec2(seg_w, rect.height()));

        // Approximate decibel center of this segment
        let seg_db = -60.0 + (i as f32 / num_segments as f32) * 63.0;
        let base_col = color_for_dbfs(seg_db);

        if i < active_peak_segments {
            // Peak bar illuminated
            let col = if i < active_rms_segments {
                base_col // Fully bright for RMS core
            } else {
                base_col.gamma_multiply(0.85) // Slightly softer for dynamic peak transients
            };
            painter.rect_filled(seg_rect, 0.5, col);
        } else {
            // Dark inactive segment notch
            painter.rect_filled(seg_rect, 0.5, Color32::from_rgb(18, 20, 26));
        }
    }

    // Peak-hold indicator needle
    let hold_frac = dbfs_to_fraction(hold_db);
    if hold_frac > 0.02 {
        let needle_x = (rect.left() + (hold_frac * total_w) - 1.0).min(rect.right() - 2.0);
        let needle_rect = Rect::from_min_size(pos2(needle_x, rect.top()), vec2(2.0, rect.height()));
        painter.rect_filled(needle_rect, 0.5, Color32::from_rgb(240, 245, 255));
    }

    // Clip Warning LED at the far right
    if clipped || peak_db >= 0.0 {
        let clip_rect = Rect::from_min_size(
            pos2(rect.right() - 4.0, rect.top()),
            vec2(4.0, rect.height()),
        );
        painter.rect_filled(clip_rect, 0.5, Color32::from_rgb(255, 30, 30));
    }
}
