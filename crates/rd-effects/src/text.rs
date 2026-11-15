//! In-engine bitmap text generator and title/subtitle overlay rasterizer.
//!
//! Provides deterministic, zero-dependency RGBA text rendering for titles,
//! lower-thirds, and broadcast subtitle overlays across timeline tracks.

use rd_core::TextClip;

/// Horizontal alignment for multi-line and single-line text overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlignment {
    Left,
    #[default]
    Center,
    Right,
}

/// Parameters configuring in-engine title / subtitle overlay rasterization.
#[derive(Debug, Clone)]
pub struct TextOverlayParams<'a> {
    /// String to render (supports newlines `\n`).
    pub text: &'a str,
    /// Normalized horizontal anchor (0.0 = left edge, 0.5 = center, 1.0 = right edge).
    pub pos_x: f32,
    /// Normalized vertical anchor (0.0 = top edge, 0.5 = center, 0.85 = lower third, 1.0 = bottom edge).
    pub pos_y: f32,
    /// Font size in points (scaled relative to standard 8x8 bitmap glyphs).
    pub font_size: f32,
    /// Text color as normalized RGBA `[r, g, b, a]` in range 0.0..1.0.
    pub color: [f32; 4],
    /// Optional background bounding box/pill color with padding behind text.
    pub bg_color: Option<[f32; 4]>,
    /// Whether to render a drop shadow offset by (scale, scale) for broadcast legibility.
    pub drop_shadow: bool,
    /// Text alignment.
    pub alignment: TextAlignment,
}

impl Default for TextOverlayParams<'_> {
    fn default() -> Self {
        Self {
            text: "",
            pos_x: 0.5,
            pos_y: 0.85,
            font_size: 42.0,
            color: [1.0, 1.0, 1.0, 1.0],
            bg_color: Some([0.0, 0.0, 0.0, 0.70]),
            drop_shadow: true,
            alignment: TextAlignment::Center,
        }
    }
}

/// Parameters for vector-sharp Signed Distance Field (SDF) text rendering with rich typography controls.
#[derive(Debug, Clone)]
pub struct SdfTypographyParams<'a> {
    /// Text content to render (supports multiple lines delimited by `\n`).
    pub text: &'a str,
    /// Normalized horizontal anchor (0.0 = left edge, 0.5 = center, 1.0 = right edge).
    pub pos_x: f32,
    /// Normalized vertical anchor (0.0 = top edge, 0.5 = center, 0.85 = lower third, 1.0 = bottom edge).
    pub pos_y: f32,
    /// Font size in points.
    pub font_size: f32,
    /// Letter spacing / tracking adjustment in normalized em units (0.0 = default spacing).
    pub tracking: f32,
    /// Line spacing / leading multiplier (1.0 = tight, 1.35 = standard multi-line).
    pub leading: f32,
    /// Text alignment.
    pub alignment: TextAlignment,
    /// Text interior color as normalized RGBA `[r, g, b, a]` in range 0.0..1.0.
    pub color: [f32; 4],
    /// Stroke outline color as normalized RGBA.
    pub outline_color: [f32; 4],
    /// Stroke outline width (0.0 = no outline, up to 0.3 in distance units).
    pub outline_width: f32,
    /// Whether drop shadow is enabled.
    pub drop_shadow: bool,
    /// Drop shadow color as normalized RGBA.
    pub shadow_color: [f32; 4],
    /// Drop shadow offset [dx, dy] normalized to UV coordinate space.
    pub shadow_offset: [f32; 2],
    /// Antialiasing edge smoothing width (typically 0.02..0.05).
    pub smoothing: f32,
    /// Drop shadow edge softness smoothing width (typically 0.05..0.15).
    pub shadow_smoothing: f32,
}

impl Default for SdfTypographyParams<'_> {
    fn default() -> Self {
        Self {
            text: "",
            pos_x: 0.5,
            pos_y: 0.85,
            font_size: 48.0,
            tracking: 0.0,
            leading: 1.35,
            alignment: TextAlignment::Center,
            color: [1.0, 1.0, 1.0, 1.0],
            outline_color: [0.0, 0.0, 0.0, 1.0],
            outline_width: 0.0,
            drop_shadow: false,
            shadow_color: [0.0, 0.0, 0.0, 0.75],
            shadow_offset: [0.003, 0.003],
            smoothing: 0.04,
            shadow_smoothing: 0.08,
        }
    }
}

/// Complete 8x8 monochrome glyph definition.
pub fn get_glyph_8x8(c: char) -> [u8; 8] {
    match c {
        'A' => [0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00],
        'B' => [0x7C, 0x42, 0x42, 0x7C, 0x42, 0x42, 0x7C, 0x00],
        'C' => [0x3C, 0x42, 0x40, 0x40, 0x40, 0x42, 0x3C, 0x00],
        'D' => [0x78, 0x44, 0x42, 0x42, 0x42, 0x44, 0x78, 0x00],
        'E' => [0x7E, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x7E, 0x00],
        'F' => [0x7E, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x40, 0x00],
        'G' => [0x3C, 0x42, 0x40, 0x4E, 0x42, 0x42, 0x3C, 0x00],
        'H' => [0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'J' => [0x1E, 0x06, 0x06, 0x06, 0x06, 0x46, 0x3C, 0x00],
        'K' => [0x42, 0x44, 0x48, 0x70, 0x48, 0x44, 0x42, 0x00],
        'L' => [0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x7E, 0x00],
        'M' => [0x81, 0xC3, 0xA5, 0x99, 0x81, 0x81, 0x81, 0x00],
        'N' => [0x42, 0x62, 0x52, 0x4A, 0x46, 0x42, 0x42, 0x00],
        'O' => [0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'P' => [0x7C, 0x42, 0x42, 0x7C, 0x40, 0x40, 0x40, 0x00],
        'Q' => [0x3C, 0x42, 0x42, 0x42, 0x4A, 0x44, 0x3A, 0x00],
        'R' => [0x7C, 0x42, 0x42, 0x7C, 0x48, 0x44, 0x42, 0x00],
        'S' => [0x3C, 0x42, 0x40, 0x3C, 0x02, 0x42, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'V' => [0x42, 0x42, 0x42, 0x42, 0x24, 0x24, 0x18, 0x00],
        'W' => [0x81, 0x81, 0x81, 0x99, 0xA5, 0xC3, 0x81, 0x00],
        'X' => [0x42, 0x24, 0x18, 0x18, 0x18, 0x24, 0x42, 0x00],
        'Y' => [0x42, 0x24, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'Z' => [0x7E, 0x04, 0x08, 0x10, 0x20, 0x40, 0x7E, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x02, 0x3E, 0x42, 0x3E, 0x00],
        'b' => [0x40, 0x40, 0x7C, 0x42, 0x42, 0x42, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x40, 0x40, 0x40, 0x3C, 0x00],
        'd' => [0x02, 0x02, 0x3E, 0x42, 0x42, 0x42, 0x3E, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x42, 0x7E, 0x40, 0x3C, 0x00],
        'f' => [0x0C, 0x12, 0x10, 0x3C, 0x10, 0x10, 0x10, 0x00],
        'g' => [0x00, 0x00, 0x3E, 0x42, 0x42, 0x3E, 0x02, 0x3C],
        'h' => [0x40, 0x40, 0x7C, 0x42, 0x42, 0x42, 0x42, 0x00],
        'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'j' => [0x06, 0x00, 0x06, 0x06, 0x06, 0x06, 0x46, 0x3C],
        'k' => [0x40, 0x40, 0x44, 0x48, 0x70, 0x48, 0x44, 0x00],
        'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0xEC, 0x92, 0x92, 0x92, 0x92, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x42, 0x42, 0x42, 0x42, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'p' => [0x00, 0x00, 0x7C, 0x42, 0x42, 0x7C, 0x40, 0x40],
        'q' => [0x00, 0x00, 0x3E, 0x42, 0x42, 0x3E, 0x02, 0x02],
        'r' => [0x00, 0x00, 0x5C, 0x62, 0x40, 0x40, 0x40, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x40, 0x3C, 0x02, 0x7C, 0x00],
        't' => [0x10, 0x10, 0x7C, 0x10, 0x10, 0x10, 0x0C, 0x00],
        'u' => [0x00, 0x00, 0x42, 0x42, 0x42, 0x46, 0x3A, 0x00],
        'v' => [0x00, 0x00, 0x42, 0x42, 0x24, 0x24, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x81, 0x99, 0x99, 0xA5, 0x42, 0x00],
        'x' => [0x00, 0x00, 0x42, 0x24, 0x18, 0x24, 0x42, 0x00],
        'y' => [0x00, 0x00, 0x42, 0x42, 0x3E, 0x02, 0x3C, 0x00],
        'z' => [0x00, 0x00, 0x7E, 0x04, 0x18, 0x20, 0x7E, 0x00],
        '0' => [0x3C, 0x46, 0x4A, 0x52, 0x62, 0x42, 0x3C, 0x00],
        '1' => [0x18, 0x28, 0x08, 0x08, 0x08, 0x08, 0x3E, 0x00],
        '2' => [0x3C, 0x42, 0x02, 0x0C, 0x30, 0x40, 0x7E, 0x00],
        '3' => [0x3C, 0x42, 0x02, 0x1C, 0x02, 0x42, 0x3C, 0x00],
        '4' => [0x08, 0x18, 0x28, 0x48, 0x7E, 0x08, 0x08, 0x00],
        '5' => [0x7E, 0x40, 0x7C, 0x02, 0x02, 0x42, 0x3C, 0x00],
        '6' => [0x3C, 0x40, 0x7C, 0x42, 0x42, 0x42, 0x3C, 0x00],
        '7' => [0x7E, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00],
        '8' => [0x3C, 0x42, 0x42, 0x3C, 0x42, 0x42, 0x3C, 0x00],
        '9' => [0x3C, 0x42, 0x42, 0x3E, 0x02, 0x02, 0x3C, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        ';' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x08, 0x10, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x08, 0x10],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
        '=' => [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
        '/' => [0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00],
        '\\' => [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x00],
        '[' => [0x3C, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3C, 0x00],
        ']' => [0x3C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x3C, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        '{' => [0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00],
        '}' => [0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00],
        '|' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        '>' => [0x20, 0x10, 0x08, 0x04, 0x08, 0x10, 0x20, 0x00],
        '<' => [0x04, 0x08, 0x10, 0x20, 0x10, 0x08, 0x04, 0x00],
        '%' => [0x62, 0x64, 0x08, 0x10, 0x20, 0x4C, 0x8C, 0x00],
        '?' => [0x3C, 0x42, 0x02, 0x0C, 0x10, 0x00, 0x10, 0x00],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
        '$' => [0x10, 0x7C, 0x90, 0x7C, 0x12, 0x7C, 0x10, 0x00],
        '*' => [0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
        '\'' => [0x18, 0x18, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00],
        '"' => [0x24, 0x24, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00],
        '@' => [0x3C, 0x42, 0x4E, 0x5A, 0x52, 0x40, 0x3C, 0x00],
        '#' => [0x24, 0x7E, 0x24, 0x24, 0x7E, 0x24, 0x00, 0x00],
        '&' => [0x30, 0x48, 0x30, 0x4C, 0x82, 0x86, 0x7A, 0x00],
        '~' => [0x00, 0x32, 0x4C, 0x00, 0x00, 0x00, 0x00, 0x00],
        '`' => [0x18, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00],
        '^' => [0x10, 0x28, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

/// Blends a source RGBA pixel onto a target RGBA pixel in-place.
#[inline(always)]
fn blend_pixel(dst: &mut [u8], src_r: u8, src_g: u8, src_b: u8, src_a: u8) {
    if src_a == 0 {
        return;
    }
    if src_a == 255 {
        dst[0] = src_r;
        dst[1] = src_g;
        dst[2] = src_b;
        dst[3] = 255;
        return;
    }

    let sa = src_a as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);

    if out_a > 0.0001 {
        let inv_out_a = 1.0 / out_a;
        let r = (src_r as f32 * sa + dst[0] as f32 * da * (1.0 - sa)) * inv_out_a;
        let g = (src_g as f32 * sa + dst[1] as f32 * da * (1.0 - sa)) * inv_out_a;
        let b = (src_b as f32 * sa + dst[2] as f32 * da * (1.0 - sa)) * inv_out_a;

        dst[0] = (r.clamp(0.0, 255.0)) as u8;
        dst[1] = (g.clamp(0.0, 255.0)) as u8;
        dst[2] = (b.clamp(0.0, 255.0)) as u8;
        dst[3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
    }
}

/// Draws a filled rectangle onto an RGBA frame buffer with alpha blending.
pub fn blend_rect(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    rect: (i32, i32, u32, u32),
    color: [u8; 4],
) {
    let (x, y, w, h) = rect;
    let [r, g, b, a] = color;
    if a == 0 || w == 0 || h == 0 {
        return;
    }

    let start_x = x.max(0) as u32;
    let end_x = ((x + w as i32).max(0) as u32).min(width);
    let start_y = y.max(0) as u32;
    let end_y = ((y + h as i32).max(0) as u32).min(height);

    if start_x >= end_x || start_y >= end_y {
        return;
    }

    for py in start_y..end_y {
        let row_start = (py * width) as usize * 4;
        for px in start_x..end_x {
            let idx = row_start + (px as usize * 4);
            if idx + 4 <= buffer.len() {
                blend_pixel(&mut buffer[idx..idx + 4], r, g, b, a);
            }
        }
    }
}

/// Renders a text overlay onto an RGBA buffer based on provided `TextOverlayParams`.
pub fn render_text_overlay(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    params: &TextOverlayParams<'_>,
) {
    if params.text.trim().is_empty() || width == 0 || height == 0 {
        return;
    }

    // Determine pixel scaling factor from font size (base glyph is 8x8)
    // Sizing: 16pt -> scale 2 (16x16 px), 32pt -> scale 4 (32x32 px), 48pt -> scale 6 (48x48 px)
    let scale = ((params.font_size / 8.0).round() as u32).clamp(1, 16);
    let char_w = 8 * scale;
    let char_h = 8 * scale;
    let line_height = (char_h as f32 * 1.35).round() as u32;

    let lines: Vec<&str> = params.text.lines().collect();
    if lines.is_empty() {
        return;
    }

    // Calculate maximum bounding width and total block height
    let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let total_w = (max_line_len as u32) * char_w;
    let total_h = if lines.len() == 1 {
        char_h
    } else {
        (lines.len() as u32 - 1) * line_height + char_h
    };

    // Calculate anchor origin (top-left of text block)
    let anchor_x = (params.pos_x * width as f32).round() as i32;
    let anchor_y = (params.pos_y * height as f32).round() as i32;

    let block_top_y = anchor_y - (total_h as i32 / 2);

    // Optional background pill/box for subtitle readability
    if let Some(bg) = params.bg_color {
        let pad_x = (scale * 6).max(8) as i32;
        let pad_y = (scale * 4).max(6) as i32;

        let pill_x = match params.alignment {
            TextAlignment::Left => anchor_x - pad_x,
            TextAlignment::Center => anchor_x - (total_w as i32 / 2) - pad_x,
            TextAlignment::Right => anchor_x - total_w as i32 - pad_x,
        };
        let pill_y = block_top_y - pad_y;
        let pill_w = total_w + (pad_x as u32 * 2);
        let pill_h = total_h + (pad_y as u32 * 2);

        let bg_r = (bg[0].clamp(0.0, 1.0) * 255.0) as u8;
        let bg_g = (bg[1].clamp(0.0, 1.0) * 255.0) as u8;
        let bg_b = (bg[2].clamp(0.0, 1.0) * 255.0) as u8;
        let bg_a = (bg[3].clamp(0.0, 1.0) * 255.0) as u8;

        blend_rect(
            buffer,
            width,
            height,
            (pill_x, pill_y, pill_w, pill_h),
            [bg_r, bg_g, bg_b, bg_a],
        );
    }

    let text_r = (params.color[0].clamp(0.0, 1.0) * 255.0) as u8;
    let text_g = (params.color[1].clamp(0.0, 1.0) * 255.0) as u8;
    let text_b = (params.color[2].clamp(0.0, 1.0) * 255.0) as u8;
    let text_a = (params.color[3].clamp(0.0, 1.0) * 255.0) as u8;

    // Render drop shadow pass first if requested
    if params.drop_shadow {
        let shadow_offset = (scale as i32).max(1);
        let shadow_a = ((text_a as f32) * 0.75).round() as u8;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_w = (line.chars().count() as u32) * char_w;
            let line_x = match params.alignment {
                TextAlignment::Left => anchor_x,
                TextAlignment::Center => anchor_x - (line_w as i32 / 2),
                TextAlignment::Right => anchor_x - line_w as i32,
            };
            let line_y = block_top_y + (line_idx as u32 * line_height) as i32;

            draw_text_line(
                buffer,
                width,
                height,
                line,
                (line_x + shadow_offset, line_y + shadow_offset),
                scale,
                [0, 0, 0, shadow_a],
            );
        }
    }

    // Render foreground text pass
    for (line_idx, line) in lines.iter().enumerate() {
        let line_w = (line.chars().count() as u32) * char_w;
        let line_x = match params.alignment {
            TextAlignment::Left => anchor_x,
            TextAlignment::Center => anchor_x - (line_w as i32 / 2),
            TextAlignment::Right => anchor_x - line_w as i32,
        };
        let line_y = block_top_y + (line_idx as u32 * line_height) as i32;

        draw_text_line(
            buffer,
            width,
            height,
            line,
            (line_x, line_y),
            scale,
            [text_r, text_g, text_b, text_a],
        );
    }
}

/// Helper function drawing a single line of text with the 8x8 font.
fn draw_text_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    text: &str,
    origin: (i32, i32),
    scale: u32,
    color: [u8; 4],
) {
    let (start_x, start_y) = origin;
    let [r, g, b, a] = color;
    let mut cur_x = start_x;
    for c in text.chars() {
        if c == ' ' {
            cur_x += 8 * scale as i32;
            continue;
        }

        let glyph = get_glyph_8x8(c);
        for (row_idx, &row_bits) in glyph.iter().enumerate() {
            if row_bits == 0 {
                continue;
            }
            let gy = start_y + (row_idx as u32 * scale) as i32;

            for col in 0..8 {
                if (row_bits & (1 << (7 - col))) != 0 {
                    let gx = cur_x + (col * scale) as i32;
                    blend_rect(buffer, width, height, (gx, gy, scale, scale), [r, g, b, a]);
                }
            }
        }
        cur_x += 8 * scale as i32;
    }
}

/// Smoothstep function for antialiased SDF edge evaluation.
#[inline(always)]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let denom = edge1 - edge0;
    if denom.abs() < 1e-6 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / denom).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1D squared Euclidean Distance Transform (Felzenszwalb & Huttenlocher).
/// Computes D[q] = min_p ((q - p)^2 + f[p]) in linear O(n) time.
pub fn edt_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32], n: usize) {
    if n == 0 {
        return;
    }
    if n == 1 {
        d[0] = f[0];
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;

    for q in 1..n {
        let mut s = ((f[q] + (q * q) as f32) - (f[v[k]] + (v[k] * v[k]) as f32))
            / (2.0 * (q as f32 - v[k] as f32));
        while k > 0 && s <= z[k] {
            k -= 1;
            s = ((f[q] + (q * q) as f32) - (f[v[k]] + (v[k] * v[k]) as f32))
                / (2.0 * (q as f32 - v[k] as f32));
        }
        if s <= z[0] {
            v[0] = q;
            z[1] = f32::INFINITY;
        } else {
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::INFINITY;
        }
    }

    k = 0;
    for (q, d_val) in d[..n].iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let dx = q as f32 - v[k] as f32;
        *d_val = dx * dx + f[v[k]];
    }
}

/// 2D Euclidean Distance Transform on a binary grid.
/// Returns Euclidean distance to the nearest pixel matching `target_value`.
pub fn edt_2d(grid: &[bool], width: usize, height: usize, target_value: bool) -> Vec<f32> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    const INF: f32 = 1e10;
    let mut f = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            f[idx] = if grid[idx] == target_value { 0.0 } else { INF };
        }
    }

    let mut d = vec![0.0f32; width * height];
    let max_dim = width.max(height);
    let mut v = vec![0usize; max_dim + 1];
    let mut z = vec![0.0f32; max_dim + 2];

    // Transform along columns
    let mut col_in = vec![0.0f32; height];
    let mut col_out = vec![0.0f32; height];
    for x in 0..width {
        for y in 0..height {
            col_in[y] = f[y * width + x];
        }
        edt_1d(&col_in, &mut col_out, &mut v, &mut z, height);
        for y in 0..height {
            d[y * width + x] = col_out[y];
        }
    }

    // Transform along rows
    let mut row_in = vec![0.0f32; width];
    let mut row_out = vec![0.0f32; width];
    for y in 0..height {
        let row_start = y * width;
        row_in.copy_from_slice(&d[row_start..row_start + width]);
        edt_1d(&row_in, &mut row_out, &mut v, &mut z, width);
        for x in 0..width {
            d[row_start + x] = row_out[x].sqrt();
        }
    }

    d
}

/// Compute a normalized Signed Distance Field (SDF) in range [0.0, 1.0].
/// Boundary is at 0.5. Interior > 0.5, exterior < 0.5.
pub fn compute_signed_distance_field(
    mask: &[bool],
    width: usize,
    height: usize,
    spread: f32,
) -> Vec<f32> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    // Distance from outside to inside (distance to true pixels)
    let dist_outside = edt_2d(mask, width, height, true);
    // Distance from inside to outside (distance to false pixels)
    let dist_inside = edt_2d(mask, width, height, false);

    let mut sdf = vec![0.5f32; width * height];
    let radius = spread.max(1.0);

    for i in 0..mask.len() {
        let signed_dist = if mask[i] {
            dist_inside[i]
        } else {
            -dist_outside[i]
        };
        let normalized = 0.5 + (signed_dist / (2.0 * radius));
        sdf[i] = normalized.clamp(0.0, 1.0);
    }

    sdf
}

/// Generates a normalized Signed Distance Field (SDF) buffer for a text string with typography parameters.
pub fn generate_text_sdf_field(
    params: &SdfTypographyParams<'_>,
    width: usize,
    height: usize,
) -> Vec<f32> {
    if width == 0 || height == 0 || params.text.trim().is_empty() {
        return vec![0.0f32; width * height];
    }

    let mut mask = vec![false; width * height];
    let lines: Vec<&str> = params.text.lines().collect();
    if lines.is_empty() {
        return vec![0.0f32; width * height];
    }

    let scale = ((params.font_size / 8.0).round() as u32).clamp(1, 64);
    let char_w = 8 * scale;
    let char_h = 8 * scale;
    let tracking_px = (char_w as f32 * params.tracking).round() as i32;
    let advance_x = (char_w as i32 + tracking_px).max(1) as u32;
    let line_height = ((char_h as f32 * params.leading).round() as u32).max(1);

    let mut line_widths = Vec::with_capacity(lines.len());
    for line in &lines {
        let count = line.chars().count();
        let w = if count == 0 {
            0
        } else {
            (count as u32 - 1) * advance_x + char_w
        };
        line_widths.push(w);
    }

    let max_line_w = line_widths.iter().copied().max().unwrap_or(0);
    let total_h = if lines.len() <= 1 {
        char_h
    } else {
        (lines.len() as u32 - 1) * line_height + char_h
    };

    let origin_x = (params.pos_x * width as f32).round() as i32 - (max_line_w as i32 / 2);
    let origin_y = (params.pos_y * height as f32).round() as i32 - (total_h as i32 / 2);

    for (line_idx, line) in lines.iter().enumerate() {
        let line_w = line_widths[line_idx];
        let line_start_x = match params.alignment {
            TextAlignment::Left => origin_x,
            TextAlignment::Center => origin_x + (max_line_w as i32 - line_w as i32) / 2,
            TextAlignment::Right => origin_x + (max_line_w as i32 - line_w as i32),
        };
        let line_y = origin_y + (line_idx as u32 * line_height) as i32;

        let mut cur_x = line_start_x;
        for c in line.chars() {
            if c != ' ' {
                let glyph = get_glyph_8x8(c);
                for (row_idx, &row_bits) in glyph.iter().enumerate() {
                    if row_bits == 0 {
                        continue;
                    }
                    let gy = line_y + (row_idx as u32 * scale) as i32;
                    for col in 0..8 {
                        if (row_bits & (1 << (7 - col))) != 0 {
                            let gx = cur_x + (col * scale) as i32;
                            for sy in 0..scale as i32 {
                                let py = gy + sy;
                                if py < 0 || py >= height as i32 {
                                    continue;
                                }
                                for sx in 0..scale as i32 {
                                    let px = gx + sx;
                                    if px >= 0 && px < width as i32 {
                                        mask[py as usize * width + px as usize] = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            cur_x += advance_x as i32;
        }
    }

    // Spread of 8.0 pixels creates an optimal gradient band for antialiasing & outlines
    compute_signed_distance_field(&mask, width, height, 8.0)
}

/// Software rasterizer for vector-sharp SDF text overlays with stroke outlines and drop shadows.
pub fn render_sdf_text_overlay(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    params: &SdfTypographyParams<'_>,
) {
    if params.text.trim().is_empty() || width == 0 || height == 0 {
        return;
    }

    let sdf = generate_text_sdf_field(params, width as usize, height as usize);
    let shadow_dx = (params.shadow_offset[0] * width as f32).round() as i32;
    let shadow_dy = (params.shadow_offset[1] * height as f32).round() as i32;

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let idx = y as usize * width as usize + x as usize;
            let dist = sdf[idx];

            // 1. Text core interior alpha
            let text_alpha =
                smoothstep(0.5 - params.smoothing, 0.5 + params.smoothing, dist) * params.color[3];

            // 2. Stroke outline alpha
            let mut outline_alpha = 0.0;
            if params.outline_width > 0.0 {
                let outline_thresh = 0.5 - params.outline_width;
                let total_coverage = smoothstep(
                    outline_thresh - params.smoothing,
                    outline_thresh + params.smoothing,
                    dist,
                );
                let inner_norm = text_alpha / params.color[3].max(0.0001);
                outline_alpha = (total_coverage - inner_norm).max(0.0) * params.outline_color[3];
            }

            // 3. Drop shadow alpha (sample at offset)
            let mut shadow_alpha = 0.0;
            if params.drop_shadow {
                let sx = x - shadow_dx;
                let sy = y - shadow_dy;
                if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                    let s_idx = sy as usize * width as usize + sx as usize;
                    let s_dist = sdf[s_idx];
                    let s_thresh = 0.5 - params.outline_width;
                    shadow_alpha = smoothstep(
                        s_thresh - params.shadow_smoothing,
                        s_thresh + params.shadow_smoothing,
                        s_dist,
                    ) * params.shadow_color[3];
                }
            }

            // Porter-Duff back-to-front composite: Shadow -> Outline -> Interior
            // Layer 1: Shadow
            let mut acc_rgb = [
                params.shadow_color[0],
                params.shadow_color[1],
                params.shadow_color[2],
            ];
            let mut acc_a = shadow_alpha;

            // Layer 2: Outline
            let out_a = outline_alpha + acc_a * (1.0 - outline_alpha);
            if out_a > 0.0001 {
                for (c, slot) in acc_rgb.iter_mut().enumerate() {
                    *slot = (params.outline_color[c] * outline_alpha
                        + *slot * acc_a * (1.0 - outline_alpha))
                        / out_a;
                }
                acc_a = out_a;
            }

            // Layer 3: Text Interior
            let final_a = text_alpha + acc_a * (1.0 - text_alpha);
            let mut final_rgb = acc_rgb;
            if final_a > 0.0001 {
                for (c, slot) in final_rgb.iter_mut().enumerate() {
                    *slot = (params.color[c] * text_alpha + *slot * acc_a * (1.0 - text_alpha))
                        / final_a;
                }
            }

            if final_a > 0.001 {
                let p_idx = idx * 4;
                blend_pixel(
                    &mut buffer[p_idx..p_idx + 4],
                    (final_rgb[0] * 255.0).clamp(0.0, 255.0) as u8,
                    (final_rgb[1] * 255.0).clamp(0.0, 255.0) as u8,
                    (final_rgb[2] * 255.0).clamp(0.0, 255.0) as u8,
                    (final_a * 255.0).clamp(0.0, 255.0) as u8,
                );
            }
        }
    }
}

/// Render a `TextClip` from `rd-core` directly onto an RGBA frame buffer.
pub fn render_text_clip_overlay(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    text_clip: &TextClip,
    opacity: f32,
) {
    let mut color = text_clip.color;
    color[3] = (color[3] * opacity).clamp(0.0, 1.0);

    let mut outline_color = text_clip.outline_color;
    outline_color[3] = (outline_color[3] * opacity).clamp(0.0, 1.0);

    let mut shadow_color = text_clip.shadow_color;
    shadow_color[3] = (shadow_color[3] * opacity).clamp(0.0, 1.0);

    let params = SdfTypographyParams {
        text: &text_clip.text,
        pos_x: text_clip.position[0],
        pos_y: text_clip.position[1],
        font_size: text_clip.font_size,
        tracking: text_clip.tracking,
        leading: text_clip.leading,
        alignment: TextAlignment::Center,
        color,
        outline_color,
        outline_width: text_clip.outline_width,
        drop_shadow: text_clip.drop_shadow,
        shadow_color,
        shadow_offset: text_clip.shadow_offset,
        smoothing: 0.04,
        shadow_smoothing: 0.08,
    };

    render_sdf_text_overlay(buffer, width, height, &params);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_lookup() {
        let glyph_a = get_glyph_8x8('A');
        assert_ne!(glyph_a, [0; 8]);
        let glyph_space = get_glyph_8x8(' ');
        assert_eq!(glyph_space, [0; 8]);
    }

    #[test]
    fn test_render_text_overlay_basic() {
        let width = 320;
        let height = 180;
        let mut buffer = vec![0u8; (width * height * 4) as usize];

        let params = TextOverlayParams {
            text: "REDDART CINEMA",
            pos_x: 0.5,
            pos_y: 0.5,
            font_size: 24.0,
            color: [1.0, 1.0, 1.0, 1.0],
            bg_color: None,
            drop_shadow: false,
            alignment: TextAlignment::Center,
        };

        render_text_overlay(&mut buffer, width, height, &params);

        // Verify that some pixels in buffer were turned white
        let non_zero_pixels = buffer.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(
            non_zero_pixels > 50,
            "Text should render non-empty glyph pixels"
        );
    }

    #[test]
    fn test_render_multiline_with_background_pill() {
        let width = 640;
        let height = 360;
        let mut buffer = vec![0u8; (width * height * 4) as usize];

        let params = TextOverlayParams {
            text: "First Line\nSecond Line Subtitle",
            pos_x: 0.5,
            pos_y: 0.8,
            font_size: 32.0,
            color: [1.0, 0.8, 0.2, 1.0],
            bg_color: Some([0.0, 0.0, 0.0, 0.8]),
            drop_shadow: true,
            alignment: TextAlignment::Center,
        };

        render_text_overlay(&mut buffer, width, height, &params);

        let non_zero_pixels = buffer.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(
            non_zero_pixels > 500,
            "Pill and text should cover significant pixel area"
        );
    }

    #[test]
    fn test_render_text_clip_helper() {
        let width = 1920;
        let height = 1080;
        let mut buffer = vec![0u8; (width * height * 4) as usize];

        let tclip = TextClip {
            text: "DIRECTOR'S CUT".into(),
            font_family: "Inter".into(),
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
            position: [0.5, 0.5],
            ..Default::default()
        };

        render_text_clip_overlay(&mut buffer, width, height, &tclip, 0.9);

        let non_zero_pixels = buffer.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(non_zero_pixels > 200);
    }

    #[test]
    fn test_edt_1d_and_2d_euclidean() {
        // Test 1D distance from single true point at index 2
        let grid_1d = [false, false, true, false, false];
        let d_1d = edt_2d(&grid_1d, 5, 1, true);
        assert_eq!(d_1d.len(), 5);
        assert!((d_1d[2] - 0.0).abs() < 1e-4);
        assert!((d_1d[1] - 1.0).abs() < 1e-4);
        assert!((d_1d[0] - 2.0).abs() < 1e-4);
        assert!((d_1d[3] - 1.0).abs() < 1e-4);
        assert!((d_1d[4] - 2.0).abs() < 1e-4);

        // Test 2D distance from single true point at (1, 1) in 3x3
        let grid_2d = [false, false, false, false, true, false, false, false, false];
        let d_2d = edt_2d(&grid_2d, 3, 3, true);
        assert!((d_2d[4] - 0.0).abs() < 1e-4); // Center (1, 1)
        assert!((d_2d[1] - 1.0).abs() < 1e-4); // (1, 0)
        assert!((d_2d[3] - 1.0).abs() < 1e-4); // (0, 1)
        assert!((d_2d[0] - std::f32::consts::SQRT_2).abs() < 1e-4); // (0, 0)
    }

    #[test]
    fn test_compute_signed_distance_field_gradient() {
        let mask = [
            false, false, false, false, false, true, true, false, false, true, true, false, false,
            false, false, false,
        ];
        let sdf = compute_signed_distance_field(&mask, 4, 4, 4.0);
        assert_eq!(sdf.len(), 16);

        // Interior points (1,1), (2,1), (1,2), (2,2) must be > 0.5
        assert!(sdf[5] > 0.5);
        assert!(sdf[6] > 0.5);
        assert!(sdf[9] > 0.5);
        assert!(sdf[10] > 0.5);

        // Corner points (0,0), (3,3) must be strictly < 0.5
        assert!(sdf[0] < 0.5);
        assert!(sdf[15] < 0.5);
    }

    #[test]
    fn test_sdf_typography_tracking_and_leading() {
        let w = 240;
        let h = 120;
        let params_default = SdfTypographyParams {
            text: "TEST\nLINE",
            pos_x: 0.5,
            pos_y: 0.5,
            font_size: 16.0,
            tracking: 0.0,
            leading: 1.0,
            ..Default::default()
        };
        let sdf_default = generate_text_sdf_field(&params_default, w, h);

        let params_tracked = SdfTypographyParams {
            text: "TEST\nLINE",
            pos_x: 0.5,
            pos_y: 0.5,
            font_size: 16.0,
            tracking: 0.5, // 50% extra character spacing
            leading: 1.0,
            ..Default::default()
        };
        let sdf_tracked = generate_text_sdf_field(&params_tracked, w, h);

        // Measure horizontal bounding span of text core (sdf > 0.5)
        let find_x_span = |sdf: &[f32]| {
            let mut min_x = usize::MAX;
            let mut max_x = 0;
            for y in 0..h {
                for x in 0..w {
                    if sdf[y * w + x] > 0.5 {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                    }
                }
            }
            max_x.saturating_sub(min_x)
        };

        let span_default = find_x_span(&sdf_default);
        let span_tracked = find_x_span(&sdf_tracked);
        assert!(
            span_tracked > span_default,
            "Expanded tracking should produce wider character layout (tracked: {}, default: {})",
            span_tracked,
            span_default
        );
    }

    #[test]
    fn test_sdf_stroke_outline_and_drop_shadow_raster() {
        let width = 160;
        let height = 80;
        let mut buffer_plain = vec![0u8; (width * height * 4) as usize];
        let mut buffer_outline = vec![0u8; (width * height * 4) as usize];
        let mut buffer_shadow = vec![0u8; (width * height * 4) as usize];

        let base_params = SdfTypographyParams {
            text: "SDF",
            pos_x: 0.5,
            pos_y: 0.5,
            font_size: 24.0,
            color: [1.0, 1.0, 1.0, 1.0],
            outline_width: 0.0,
            drop_shadow: false,
            ..Default::default()
        };

        let mut outline_params = base_params.clone();
        outline_params.outline_color = [1.0, 0.0, 0.0, 1.0];
        outline_params.outline_width = 0.15;

        let mut shadow_params = base_params.clone();
        shadow_params.drop_shadow = true;
        shadow_params.shadow_offset = [0.05, 0.05];

        render_sdf_text_overlay(&mut buffer_plain, width, height, &base_params);
        render_sdf_text_overlay(&mut buffer_outline, width, height, &outline_params);
        render_sdf_text_overlay(&mut buffer_shadow, width, height, &shadow_params);

        let count_plain = buffer_plain.chunks_exact(4).filter(|p| p[3] > 0).count();
        let count_outline = buffer_outline.chunks_exact(4).filter(|p| p[3] > 0).count();
        let count_shadow = buffer_shadow.chunks_exact(4).filter(|p| p[3] > 0).count();

        assert!(count_plain > 50);
        assert!(
            count_outline > count_plain,
            "Stroke outline should expand total visible pixel area (outline: {}, plain: {})",
            count_outline,
            count_plain
        );
        assert!(
            count_shadow > count_plain,
            "Drop shadow should expand total visible pixel area (shadow: {}, plain: {})",
            count_shadow,
            count_plain
        );
    }
}
