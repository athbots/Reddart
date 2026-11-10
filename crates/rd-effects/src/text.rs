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

    let params = TextOverlayParams {
        text: &text_clip.text,
        pos_x: text_clip.position[0],
        pos_y: text_clip.position[1],
        font_size: text_clip.font_size,
        color,
        bg_color: Some([0.0, 0.0, 0.0, 0.65 * opacity.clamp(0.0, 1.0)]),
        drop_shadow: true,
        alignment: TextAlignment::Center,
    };

    render_text_overlay(buffer, width, height, &params);
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
        let non_zero_pixels = buffer
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count();
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

        let non_zero_pixels = buffer
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count();
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
        };

        render_text_clip_overlay(&mut buffer, width, height, &tclip, 0.9);

        let non_zero_pixels = buffer
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count();
        assert!(non_zero_pixels > 200);
    }
}
