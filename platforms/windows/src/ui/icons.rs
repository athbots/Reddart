//! Reusable Procedural 2D Vector Icon Library for Reddart Desktop.
//!
//! Replaces all Unicode emoji/symbols with mathematically defined, GPU-rendered
//! vector shapes drawn via `egui::Painter`.

use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};

// ---------------------------------------------------------------------------
// Transport Vector Icons
// ---------------------------------------------------------------------------

/// Draws a solid, optically centered Play triangle.
pub fn draw_play(painter: &Painter, rect: Rect, color: Color32) {
    let pad_x = rect.width() * 0.28;
    let pad_y = rect.height() * 0.24;

    let p1 = pos2(rect.left() + pad_x, rect.top() + pad_y);
    let p2 = pos2(rect.left() + pad_x, rect.bottom() - pad_y);
    let p3 = pos2(rect.right() - pad_x + 1.0, rect.center().y);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::NONE,
    ));
}

/// Draws two crisp, symmetrically spaced Pause vertical bars.
pub fn draw_pause(painter: &Painter, rect: Rect, color: Color32) {
    let bar_w = (rect.width() * 0.20).clamp(2.0, 5.0);
    let bar_h = rect.height() * 0.54;
    let gap = (rect.width() * 0.16).clamp(2.0, 5.0);

    let center_y = rect.center().y;
    let top_y = center_y - bar_h * 0.5;

    let left_bar = Rect::from_min_size(
        pos2(rect.center().x - gap * 0.5 - bar_w, top_y),
        vec2(bar_w, bar_h),
    );
    let right_bar =
        Rect::from_min_size(pos2(rect.center().x + gap * 0.5, top_y), vec2(bar_w, bar_h));

    painter.rect_filled(left_bar, 1.0, color);
    painter.rect_filled(right_bar, 1.0, color);
}

/// Draws a crisp, centered Stop square.
pub fn draw_stop(painter: &Painter, rect: Rect, color: Color32) {
    let size = (rect.width().min(rect.height()) * 0.50).round();
    let stop_rect = Rect::from_center_size(rect.center(), vec2(size, size));
    painter.rect_filled(stop_rect, 1.5, color);
}

/// Draws a Step Forward control (triangle pointing right + vertical stop bar).
pub fn draw_step_forward(painter: &Painter, rect: Rect, color: Color32) {
    let h = rect.height() * 0.52;
    let top_y = rect.center().y - h * 0.5;
    let bot_y = rect.center().y + h * 0.5;

    let tri_left = rect.left() + rect.width() * 0.22;
    let tri_right = rect.left() + rect.width() * 0.62;

    let p1 = pos2(tri_left, top_y);
    let p2 = pos2(tri_left, bot_y);
    let p3 = pos2(tri_right, rect.center().y);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::NONE,
    ));

    let bar_x = tri_right + 2.0;
    let bar_rect = Rect::from_min_size(pos2(bar_x, top_y), vec2(2.0, h));
    painter.rect_filled(bar_rect, 0.5, color);
}

/// Draws a Step Backward control (vertical stop bar + triangle pointing left).
pub fn draw_step_backward(painter: &Painter, rect: Rect, color: Color32) {
    let h = rect.height() * 0.52;
    let top_y = rect.center().y - h * 0.5;
    let bot_y = rect.center().y + h * 0.5;

    let bar_x = rect.left() + rect.width() * 0.22;
    let bar_rect = Rect::from_min_size(pos2(bar_x, top_y), vec2(2.0, h));
    painter.rect_filled(bar_rect, 0.5, color);

    let tri_left = bar_x + 2.0;
    let tri_right = rect.left() + rect.width() * 0.78;

    let p1 = pos2(tri_right, top_y);
    let p2 = pos2(tri_right, bot_y);
    let p3 = pos2(tri_left, rect.center().y);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::NONE,
    ));
}

/// Draws a Jump to Start control (double chevron left + bar).
pub fn draw_jump_start(painter: &Painter, rect: Rect, color: Color32) {
    let h = rect.height() * 0.54;
    let top_y = rect.center().y - h * 0.5;
    let bot_y = rect.center().y + h * 0.5;

    // Left bar
    let bar_x = rect.left() + rect.width() * 0.18;
    painter.rect_filled(
        Rect::from_min_size(pos2(bar_x, top_y), vec2(2.0, h)),
        0.5,
        color,
    );

    // Left chevron
    let c_mid = bar_x + 3.0;
    let c_right = rect.right() - rect.width() * 0.25;

    let p1 = pos2(c_right, top_y);
    let p2 = pos2(c_right, bot_y);
    let p3 = pos2(c_mid, rect.center().y);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::NONE,
    ));
}

/// Draws a Jump to End control (chevron right + bar).
pub fn draw_jump_end(painter: &Painter, rect: Rect, color: Color32) {
    let h = rect.height() * 0.54;
    let top_y = rect.center().y - h * 0.5;
    let bot_y = rect.center().y + h * 0.5;

    // Right bar
    let bar_x = rect.right() - rect.width() * 0.18 - 2.0;
    painter.rect_filled(
        Rect::from_min_size(pos2(bar_x, top_y), vec2(2.0, h)),
        0.5,
        color,
    );

    // Right chevron
    let c_left = rect.left() + rect.width() * 0.25;
    let c_mid = bar_x - 1.0;

    let p1 = pos2(c_left, top_y);
    let p2 = pos2(c_left, bot_y);
    let p3 = pos2(c_mid, rect.center().y);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3],
        color,
        Stroke::NONE,
    ));
}

/// Draws Shuttle Reverse (double triangles pointing left).
pub fn draw_shuttle_rev(painter: &Painter, rect: Rect, color: Color32) {
    let h = rect.height() * 0.50;
    let top_y = rect.center().y - h * 0.5;
    let bot_y = rect.center().y + h * 0.5;
    let mid_x = rect.center().x;

    // Left triangle
    let t1_left = rect.left() + rect.width() * 0.18;
    let t1_right = mid_x;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(t1_right, top_y),
            pos2(t1_right, bot_y),
            pos2(t1_left, rect.center().y),
        ],
        color,
        Stroke::NONE,
    ));

    // Right triangle
    let t2_left = mid_x;
    let t2_right = rect.right() - rect.width() * 0.18;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(t2_right, top_y),
            pos2(t2_right, bot_y),
            pos2(t2_left, rect.center().y),
        ],
        color,
        Stroke::NONE,
    ));
}

/// Draws Shuttle Forward (double triangles pointing right).
pub fn draw_shuttle_fwd(painter: &Painter, rect: Rect, color: Color32) {
    let h = rect.height() * 0.50;
    let top_y = rect.center().y - h * 0.5;
    let bot_y = rect.center().y + h * 0.5;
    let mid_x = rect.center().x;

    // Left triangle
    let t1_left = rect.left() + rect.width() * 0.18;
    let t1_right = mid_x;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(t1_left, top_y),
            pos2(t1_left, bot_y),
            pos2(t1_right, rect.center().y),
        ],
        color,
        Stroke::NONE,
    ));

    // Right triangle
    let t2_left = mid_x;
    let t2_right = rect.right() - rect.width() * 0.18;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(t2_left, top_y),
            pos2(t2_left, bot_y),
            pos2(t2_right, rect.center().y),
        ],
        color,
        Stroke::NONE,
    ));
}

// ---------------------------------------------------------------------------
// Editing Tool Vector Icons
// ---------------------------------------------------------------------------

/// Draws a sleek Razor / Split blade icon.
pub fn draw_razor(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.3_f32, color);
    let center = rect.center();
    let r = rect.width().min(rect.height()) * 0.36;

    // Angled razor outline
    let p1 = pos2(center.x - r * 0.6, center.y - r * 0.8);
    let p2 = pos2(center.x + r * 0.6, center.y - r * 0.8);
    let p3 = pos2(center.x + r * 0.8, center.y + r * 0.8);
    let p4 = pos2(center.x - r * 0.8, center.y + r * 0.8);

    painter.add(egui::Shape::closed_line(vec![p1, p2, p3, p4], stroke));

    // Center vertical blade slot
    painter.line_segment(
        [
            pos2(center.x, center.y - r * 0.5),
            pos2(center.x, center.y + r * 0.5),
        ],
        Stroke::new(1.5_f32, color),
    );
}

/// Draws the Selection Arrow pointer.
pub fn draw_selection_arrow(painter: &Painter, rect: Rect, color: Color32) {
    let origin = pos2(
        rect.left() + rect.width() * 0.28,
        rect.top() + rect.height() * 0.22,
    );
    let s = rect.width().min(rect.height()) * 0.58;

    let p1 = origin;
    let p2 = pos2(origin.x, origin.y + s);
    let p3 = pos2(origin.x + s * 0.40, origin.y + s * 0.70);
    let p4 = pos2(origin.x + s * 0.70, origin.y + s * 0.95);
    let p5 = pos2(origin.x + s * 0.85, origin.y + s * 0.80);
    let p6 = pos2(origin.x + s * 0.55, origin.y + s * 0.55);
    let p7 = pos2(origin.x + s * 0.85, origin.y + s * 0.40);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3, p4, p5, p6, p7],
        color,
        Stroke::new(0.8_f32, Color32::from_black_alpha(120)),
    ));
}

/// Draws an industrial Horseshoe Snapping Magnet.
pub fn draw_snapping_magnet(painter: &Painter, rect: Rect, active: bool, theme_accent: Color32) {
    let center = rect.center();
    let r = (rect.width().min(rect.height()) * 0.32).clamp(4.0, 9.0);
    let thickness = 2.4;

    let base_color = if active {
        theme_accent
    } else {
        Color32::from_rgb(156, 166, 184)
    };
    let stroke = Stroke::new(thickness, base_color);

    // Bottom semicircular arch
    let mut arch_points = Vec::new();
    for i in 0..=12 {
        let angle = std::f32::consts::PI * (i as f32 / 12.0);
        let x = center.x - r * angle.cos();
        let y = center.y + r * angle.sin();
        arch_points.push(pos2(x, y));
    }
    painter.add(egui::Shape::line(arch_points, stroke));

    // Vertical prongs extending upwards
    let left_top = pos2(center.x - r, center.y - r * 0.5);
    let left_mid = pos2(center.x - r, center.y);
    let right_top = pos2(center.x + r, center.y - r * 0.5);
    let right_mid = pos2(center.x + r, center.y);

    painter.line_segment([left_mid, left_top], stroke);
    painter.line_segment([right_mid, right_top], stroke);

    // Colored pole tips
    let pole_color = if active {
        Color32::from_rgb(239, 68, 68)
    } else {
        Color32::from_rgb(200, 70, 70)
    };
    painter.rect_filled(
        Rect::from_center_size(left_top, vec2(thickness + 1.0, 3.0)),
        0.5,
        pole_color,
    );
    let pole_color_blue = if active {
        Color32::from_rgb(59, 130, 246)
    } else {
        Color32::from_rgb(70, 100, 200)
    };
    painter.rect_filled(
        Rect::from_center_size(right_top, vec2(thickness + 1.0, 3.0)),
        0.5,
        pole_color_blue,
    );
}

/// Draws Ripple Trim (bracket with directional arrow).
pub fn draw_ripple_trim(painter: &Painter, rect: Rect, is_head: bool, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let center = rect.center();
    let h = rect.height() * 0.55;
    let w = rect.width() * 0.40;

    let x_bar = if is_head {
        center.x - w * 0.5
    } else {
        center.x + w * 0.5
    };
    let x_arm = if is_head {
        center.x + w * 0.5
    } else {
        center.x - w * 0.5
    };

    // Bracket
    let top_y = center.y - h * 0.5;
    let bot_y = center.y + h * 0.5;
    painter.line_segment([pos2(x_arm, top_y), pos2(x_bar, top_y)], stroke);
    painter.line_segment([pos2(x_bar, top_y), pos2(x_bar, bot_y)], stroke);
    painter.line_segment([pos2(x_bar, bot_y), pos2(x_arm, bot_y)], stroke);

    // Directional chevron
    let arr_tip = if is_head {
        center.x + w * 0.2
    } else {
        center.x - w * 0.2
    };
    let arr_base = if is_head {
        center.x - w * 0.1
    } else {
        center.x + w * 0.1
    };
    painter.line_segment(
        [pos2(arr_base, center.y - 3.0), pos2(arr_tip, center.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(arr_base, center.y + 3.0), pos2(arr_tip, center.y)],
        stroke,
    );
}

/// Draws Roll Edit (opposing brackets `] [`).
pub fn draw_roll_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.3_f32, color);
    let center = rect.center();
    let h = rect.height() * 0.52;
    let top = center.y - h * 0.5;
    let bot = center.y + h * 0.5;

    // Left bracket `]`
    let l_bar = center.x - 2.0;
    painter.line_segment([pos2(l_bar - 4.0, top), pos2(l_bar, top)], stroke);
    painter.line_segment([pos2(l_bar, top), pos2(l_bar, bot)], stroke);
    painter.line_segment([pos2(l_bar, bot), pos2(l_bar - 4.0, bot)], stroke);

    // Right bracket `[`
    let r_bar = center.x + 2.0;
    painter.line_segment([pos2(r_bar + 4.0, top), pos2(r_bar, top)], stroke);
    painter.line_segment([pos2(r_bar, top), pos2(r_bar, bot)], stroke);
    painter.line_segment([pos2(r_bar, bot), pos2(r_bar + 4.0, bot)], stroke);
}

/// Draws Slip Edit (clip box with double-headed interior horizontal arrow).
pub fn draw_slip_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    let center = rect.center();
    let box_w = rect.width() * 0.65;
    let box_h = rect.height() * 0.50;

    let b_rect = Rect::from_center_size(center, vec2(box_w, box_h));
    painter.rect_stroke(b_rect, 1.0, stroke, egui::StrokeKind::Inside);

    // Double-headed interior horizontal arrow
    let arrow_w = box_w * 0.55;
    let l_x = center.x - arrow_w * 0.5;
    let r_x = center.x + arrow_w * 0.5;
    painter.line_segment([pos2(l_x, center.y), pos2(r_x, center.y)], stroke);
    painter.line_segment(
        [pos2(l_x + 2.5, center.y - 2.5), pos2(l_x, center.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(l_x + 2.5, center.y + 2.5), pos2(l_x, center.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(r_x - 2.5, center.y - 2.5), pos2(r_x, center.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(r_x - 2.5, center.y + 2.5), pos2(r_x, center.y)],
        stroke,
    );
}

/// Draws Slide Edit (solid box with exterior directional arrows).
pub fn draw_slide_tool(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    let center = rect.center();
    let box_w = rect.width() * 0.40;
    let box_h = rect.height() * 0.46;

    let b_rect = Rect::from_center_size(center, vec2(box_w, box_h));
    painter.rect_filled(b_rect, 1.0, color);

    // Exterior left arrow
    let l_arr = center.x - box_w * 0.5 - 3.0;
    painter.line_segment(
        [pos2(l_arr, center.y - 3.0), pos2(l_arr - 2.5, center.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(l_arr, center.y + 3.0), pos2(l_arr - 2.5, center.y)],
        stroke,
    );

    // Exterior right arrow
    let r_arr = center.x + box_w * 0.5 + 3.0;
    painter.line_segment(
        [pos2(r_arr, center.y - 3.0), pos2(r_arr + 2.5, center.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(r_arr, center.y + 3.0), pos2(r_arr + 2.5, center.y)],
        stroke,
    );
}

/// Draws a Cue / Timeline Marker diamond icon.
pub fn draw_marker(painter: &Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let r = 5.0_f32;
    let p1 = pos2(center.x, center.y - r);
    let p2 = pos2(center.x + r * 0.75, center.y);
    let p3 = pos2(center.x, center.y + r);
    let p4 = pos2(center.x - r * 0.75, center.y);
    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3, p4],
        color,
        Stroke::NONE,
    ));
}

/// Draws an In-Point bracket `[`.
pub fn draw_mark_in(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5_f32, color);
    let center = rect.center();
    let h = 8.0_f32;
    let w = 4.0_f32;
    let top = center.y - h * 0.5;
    let bot = center.y + h * 0.5;
    let x = center.x - 1.0;
    painter.line_segment([pos2(x + w, top), pos2(x, top)], stroke);
    painter.line_segment([pos2(x, top), pos2(x, bot)], stroke);
    painter.line_segment([pos2(x, bot), pos2(x + w, bot)], stroke);
}

/// Draws an Out-Point bracket `]`.
pub fn draw_mark_out(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5_f32, color);
    let center = rect.center();
    let h = 8.0_f32;
    let w = 4.0_f32;
    let top = center.y - h * 0.5;
    let bot = center.y + h * 0.5;
    let x = center.x + 1.0;
    painter.line_segment([pos2(x - w, top), pos2(x, top)], stroke);
    painter.line_segment([pos2(x, top), pos2(x, bot)], stroke);
    painter.line_segment([pos2(x, bot), pos2(x - w, bot)], stroke);
}

// ---------------------------------------------------------------------------
// Audio & Track Controls
// ---------------------------------------------------------------------------

/// Draws a crisp Speaker / Mute icon.
pub fn draw_speaker(painter: &Painter, rect: Rect, muted: bool, color: Color32) {
    let center = rect.center();
    let h = rect.height() * 0.50;
    let top = center.y - h * 0.5;
    let bot = center.y + h * 0.5;

    let body_l = center.x - 5.0;
    let body_r = center.x - 2.0;
    let cone_r = center.x + 3.0;

    // Speaker body & cone
    let p1 = pos2(body_l, center.y - 3.0);
    let p2 = pos2(body_r, center.y - 3.0);
    let p3 = pos2(cone_r, top);
    let p4 = pos2(cone_r, bot);
    let p5 = pos2(body_r, center.y + 3.0);
    let p6 = pos2(body_l, center.y + 3.0);

    painter.add(egui::Shape::convex_polygon(
        vec![p1, p2, p3, p4, p5, p6],
        color,
        Stroke::NONE,
    ));

    if muted {
        // Red diagonal strike-through slash
        let strike_stroke = Stroke::new(1.6_f32, Color32::from_rgb(239, 68, 68));
        painter.line_segment(
            [
                pos2(center.x - 6.0, top - 1.0),
                pos2(center.x + 6.0, bot + 1.0),
            ],
            strike_stroke,
        );
    } else {
        // Subtle sound waves
        let stroke = Stroke::new(1.2_f32, color);
        let arc_x = cone_r + 2.5;
        painter.line_segment(
            [pos2(arc_x, center.y - 3.0), pos2(arc_x + 1.5, center.y)],
            stroke,
        );
        painter.line_segment(
            [pos2(arc_x + 1.5, center.y), pos2(arc_x, center.y + 3.0)],
            stroke,
        );
    }
}

/// Draws a Padlock (locked / unlocked).
pub fn draw_lock(painter: &Painter, rect: Rect, locked: bool, color: Color32) {
    let center = rect.center();
    let body_w = 8.0;
    let body_h = 7.0;
    let body_rect = Rect::from_center_size(pos2(center.x, center.y + 2.0), vec2(body_w, body_h));

    // Lock body
    painter.rect_filled(body_rect, 1.0, color);

    // Lock shackle
    let stroke = Stroke::new(1.2_f32, color);
    let shackle_r = 3.0;
    let shackle_top = center.y - 4.5;

    if locked {
        let p_left = pos2(center.x - shackle_r, center.y - 1.5);
        let p_top_l = pos2(center.x - shackle_r, shackle_top);
        let p_top_r = pos2(center.x + shackle_r, shackle_top);
        let p_right = pos2(center.x + shackle_r, center.y - 1.5);
        painter.add(egui::Shape::line(
            vec![p_left, p_top_l, p_top_r, p_right],
            stroke,
        ));
    } else {
        // Open shackle (raised and swung right)
        let p_left = pos2(center.x - shackle_r, center.y - 1.5);
        let p_top_l = pos2(center.x - shackle_r, shackle_top - 2.0);
        let p_top_r = pos2(center.x + shackle_r, shackle_top - 2.0);
        let p_right = pos2(center.x + shackle_r, shackle_top);
        painter.add(egui::Shape::line(
            vec![p_left, p_top_l, p_top_r, p_right],
            stroke,
        ));
    }
}

// ---------------------------------------------------------------------------
// Workspace Navigation Vector Icons
// ---------------------------------------------------------------------------

/// Workspace Nav: EDIT (Filmstrip timeline glyph).
pub fn draw_nav_edit(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    let center = rect.center();
    let w = 12.0;
    let h = 9.0;
    let film_rect = Rect::from_center_size(center, vec2(w, h));

    painter.rect_stroke(film_rect, 1.0, stroke, egui::StrokeKind::Inside);
    // Vertical playhead line
    painter.line_segment(
        [
            pos2(center.x, film_rect.top()),
            pos2(center.x, film_rect.bottom()),
        ],
        Stroke::new(1.2_f32, Color32::from_rgb(59, 130, 246)),
    );
    // Left & right cut marks
    painter.line_segment(
        [
            pos2(film_rect.left() + 3.0, film_rect.top()),
            pos2(film_rect.left() + 3.0, film_rect.bottom()),
        ],
        Stroke::new(0.8_f32, color),
    );
}

/// Workspace Nav: COLOR (3-lobe color aperture).
pub fn draw_nav_color(painter: &Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let r = 4.5;
    // Outer circle
    painter.circle_stroke(center, r, Stroke::new(1.2_f32, color));

    // Internal 3-lobe trifoil
    let d = 2.2;
    painter.circle_filled(
        pos2(center.x, center.y - d),
        1.4,
        Color32::from_rgb(239, 68, 68),
    ); // Red
    painter.circle_filled(
        pos2(center.x - d * 0.86, center.y + d * 0.5),
        1.4,
        Color32::from_rgb(34, 197, 94),
    ); // Green
    painter.circle_filled(
        pos2(center.x + d * 0.86, center.y + d * 0.5),
        1.4,
        Color32::from_rgb(59, 130, 246),
    ); // Blue
}

/// Workspace Nav: AUDIO (Equalizer waveform bars).
pub fn draw_nav_audio(painter: &Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let bar_w = 1.8;
    let gap = 2.4;
    let heights = [4.0, 9.0, 6.0, 11.0, 7.0];

    let start_x = center.x - (heights.len() as f32 * (bar_w + gap) - gap) * 0.5;
    for (i, &h) in heights.iter().enumerate() {
        let x = start_x + i as f32 * (bar_w + gap);
        let r = Rect::from_center_size(pos2(x + bar_w * 0.5, center.y), vec2(bar_w, h));
        painter.rect_filled(r, 0.5, color);
    }
}

/// Workspace Nav: TITLES (Typographic capital 'T' with underline).
pub fn draw_nav_titles(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let center = rect.center();
    let w = 9.0;
    let h = 9.0;

    // Top horizontal crossbar
    painter.line_segment(
        [
            pos2(center.x - w * 0.5, center.y - h * 0.5),
            pos2(center.x + w * 0.5, center.y - h * 0.5),
        ],
        stroke,
    );
    // Vertical stem
    painter.line_segment(
        [
            pos2(center.x, center.y - h * 0.5),
            pos2(center.x, center.y + h * 0.35),
        ],
        stroke,
    );
    // Underline
    painter.line_segment(
        [
            pos2(center.x - w * 0.4, center.y + h * 0.5),
            pos2(center.x + w * 0.4, center.y + h * 0.5),
        ],
        Stroke::new(1.0_f32, color),
    );
}

/// Workspace Nav: EXPORT (Clapperboard delivery icon).
pub fn draw_nav_export(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    let center = rect.center();
    let w = 11.0;
    let h = 8.5;

    let b_rect = Rect::from_center_size(center, vec2(w, h));
    painter.rect_stroke(b_rect, 1.0, stroke, egui::StrokeKind::Inside);

    // Clapperboard slates on top
    let slate_y = b_rect.top() + 3.0;
    painter.line_segment(
        [pos2(b_rect.left(), slate_y), pos2(b_rect.right(), slate_y)],
        stroke,
    );
    painter.line_segment(
        [
            pos2(b_rect.left() + 3.5, b_rect.top()),
            pos2(b_rect.left() + 2.0, slate_y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(b_rect.left() + 7.5, b_rect.top()),
            pos2(b_rect.left() + 6.0, slate_y),
        ],
        stroke,
    );

    // Center delivery arrow pointing up
    painter.line_segment(
        [
            pos2(center.x, b_rect.bottom() - 1.5),
            pos2(center.x, slate_y + 1.5),
        ],
        Stroke::new(1.2_f32, Color32::from_rgb(16, 185, 129)),
    );
}

// ---------------------------------------------------------------------------
// General Utilities (Folder, Close, Check)
// ---------------------------------------------------------------------------

/// Draws a clean Folder icon.
pub fn draw_folder(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    let center = rect.center();
    let w = 12.0;
    let h = 9.0;
    let top = center.y - h * 0.5;
    let bot = center.y + h * 0.5;
    let left = center.x - w * 0.5;
    let right = center.x + w * 0.5;

    // Folder tab
    let tab_right = left + w * 0.45;
    let tab_top = top - 1.8;

    let p1 = pos2(left, bot);
    let p2 = pos2(left, top);
    let p3 = pos2(left + 1.0, tab_top);
    let p4 = pos2(tab_right - 1.0, tab_top);
    let p5 = pos2(tab_right, top);
    let p6 = pos2(right, top);
    let p7 = pos2(right, bot);

    painter.add(egui::Shape::closed_line(
        vec![p1, p2, p3, p4, p5, p6, p7],
        stroke,
    ));
}

/// Draws a crisp, centered 'X' Close icon.
pub fn draw_close(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let center = rect.center();
    let r = 4.0;

    painter.line_segment(
        [
            pos2(center.x - r, center.y - r),
            pos2(center.x + r, center.y + r),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(center.x + r, center.y - r),
            pos2(center.x - r, center.y + r),
        ],
        stroke,
    );
}
