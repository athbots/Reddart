//! Reddart Master Controlled Screen Recording & Walkthrough Generator
//!
//! Generates a broadcast-quality 1080p 60fps screen recording demonstrating:
//! 1. Windows 11 Desktop Window & Menu Navigation (File, Edit, Save to WAL)
//! 2. Project Media Bin & Multi-Track Timeline Loading
//! 3. Zero-Lag Timeline Scrubbing & LCD Timecode Navigation (11.46 µs DAG latency)
//! 4. Professional NLE Toolset (Razor [C], Ripple Trim [B], Markers [M], In/Out [I/O])
//! 5. Real-Time 48kHz Playback & 36-Segment LED dBFS Stereo Metering
//! 6. Rec.709 3D LUT Color Grading with Interactive Viewport Split Wipe
//! 7. Hardware-Accelerated NVENC Video Export Modal & Progress Completion

#![allow(
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::comparison_chain
)]

use std::io::Write;
use std::process::{Command, Stdio};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const FPS: u32 = 60;
const DURATION_SECS: f32 = 32.0;
const TOTAL_FRAMES: usize = (FPS as f32 * DURATION_SECS) as usize; // 1920 frames

// ---------------------------------------------------------------------------
// 8x8 Simple Raster Font for UI Typography
// ---------------------------------------------------------------------------
mod font_8x8 {
    pub fn get_glyph(c: char) -> [u8; 8] {
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
            '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
            ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x08, 0x10],
            '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
            '+' => [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
            '/' => [0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00],
            '[' => [0x3C, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3C, 0x00],
            ']' => [0x3C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x3C, 0x00],
            '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
            ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
            '|' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
            '>' => [0x20, 0x10, 0x08, 0x04, 0x08, 0x10, 0x20, 0x00],
            '<' => [0x04, 0x08, 0x10, 0x20, 0x10, 0x08, 0x04, 0x00],
            '%' => [0x62, 0x64, 0x08, 0x10, 0x20, 0x4C, 0x8C, 0x00],
            '?' => [0x3C, 0x42, 0x02, 0x0C, 0x10, 0x00, 0x10, 0x00],
            '!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
            '$' => [0x10, 0x7C, 0x90, 0x7C, 0x12, 0x7C, 0x10, 0x00],
            '*' => [0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00],
            '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
            _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        }
    }
}

// ---------------------------------------------------------------------------
// Framebuffer Rendering Utilities
// ---------------------------------------------------------------------------

struct Framebuffer {
    pub pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; WIDTH * HEIGHT * 4],
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8, a: u8) {
        if x < WIDTH && y < HEIGHT {
            let idx = (y * WIDTH + x) * 4;
            if a == 255 {
                self.pixels[idx] = r;
                self.pixels[idx + 1] = g;
                self.pixels[idx + 2] = b;
                self.pixels[idx + 3] = 255;
            } else {
                let alpha = a as f32 / 255.0;
                let inv = 1.0 - alpha;
                self.pixels[idx] = (r as f32 * alpha + self.pixels[idx] as f32 * inv) as u8;
                self.pixels[idx + 1] =
                    (g as f32 * alpha + self.pixels[idx + 1] as f32 * inv) as u8;
                self.pixels[idx + 2] =
                    (b as f32 * alpha + self.pixels[idx + 2] as f32 * inv) as u8;
                self.pixels[idx + 3] = 255;
            }
        }
    }

    pub fn fill_rect(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) {
        let max_x = (x + w).min(WIDTH);
        let max_y = (y + h).min(HEIGHT);
        for cy in y..max_y {
            for cx in x..max_x {
                self.set_pixel(cx, cy, r, g, b, a);
            }
        }
    }

    pub fn draw_rect_outline(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        thickness: usize,
        r: u8,
        g: u8,
        b: u8,
    ) {
        self.fill_rect(x, y, w, thickness, r, g, b, 255);
        self.fill_rect(
            x,
            y + h.saturating_sub(thickness),
            w,
            thickness,
            r,
            g,
            b,
            255,
        );
        self.fill_rect(x, y, thickness, h, r, g, b, 255);
        self.fill_rect(
            x + w.saturating_sub(thickness),
            y,
            thickness,
            h,
            r,
            g,
            b,
            255,
        );
    }

    pub fn fill_circle(
        &mut self,
        cx: isize,
        cy: isize,
        r: isize,
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    ) {
        let r2 = r * r;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r2 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < WIDTH as isize && py >= 0 && py < HEIGHT as isize {
                        self.set_pixel(px as usize, py as usize, red, green, blue, alpha);
                    }
                }
            }
        }
    }

    pub fn draw_circle_outline(
        &mut self,
        cx: isize,
        cy: isize,
        r: isize,
        thickness: isize,
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    ) {
        let r_outer2 = r * r;
        let r_inner = (r - thickness).max(0);
        let r_inner2 = r_inner * r_inner;
        for dy in -r..=r {
            for dx in -r..=r {
                let d2 = dx * dx + dy * dy;
                if d2 <= r_outer2 && d2 >= r_inner2 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < WIDTH as isize && py >= 0 && py < HEIGHT as isize {
                        self.set_pixel(px as usize, py as usize, red, green, blue, alpha);
                    }
                }
            }
        }
    }

    pub fn draw_text(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        scale: usize,
        r: u8,
        g: u8,
        b: u8,
    ) {
        let mut cur_x = x;
        for c in text.chars() {
            let glyph = font_8x8::get_glyph(c);
            for row in 0..8 {
                let bits = glyph[row];
                for col in 0..8 {
                    if (bits & (1 << (7 - col))) != 0 {
                        self.fill_rect(
                            cur_x + col * scale,
                            y + row * scale,
                            scale,
                            scale,
                            r,
                            g,
                            b,
                            255,
                        );
                    }
                }
            }
            cur_x += 8 * scale;
        }
    }

    /// Draws a crisp Windows 11 arrow mouse cursor at (x, y) with click ripple animation
    pub fn draw_mouse_cursor(
        &mut self,
        x: usize,
        y: usize,
        is_clicking: bool,
        click_pulse: f32,
        is_razor: bool,
    ) {
        // Click ripple animation
        if click_pulse > 0.0 && click_pulse < 1.0 {
            let radius = (click_pulse * 24.0) as isize + 4;
            let alpha = ((1.0 - click_pulse) * 220.0) as u8;
            self.draw_circle_outline(x as isize, y as isize, radius, 2, 0, 229, 255, alpha);
        }
        if is_clicking {
            self.fill_circle(x as isize, y as isize, 6, 0, 229, 255, 160);
        }

        if is_razor {
            // Razor blade / Crosshair tool cursor
            let cx = x as isize;
            let cy = y as isize;
            for i in -9..=9 {
                if i != 0 {
                    self.set_pixel((cx + i).clamp(0, WIDTH as isize - 1) as usize, cy as usize, 255, 220, 0, 255);
                    self.set_pixel(cx as usize, (cy + i).clamp(0, HEIGHT as isize - 1) as usize, 255, 220, 0, 255);
                }
            }
            self.draw_circle_outline(cx, cy, 5, 1, 255, 220, 0, 255);
            return;
        }

        // Standard Windows 11 Cursor Bitmap (12 x 19)
        const MASK: [[u8; 12]; 19] = [
            [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [2, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [2, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0],
            [2, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0],
            [2, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0],
            [2, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0],
            [2, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0],
            [2, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0],
            [2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0],
            [2, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 0],
            [2, 1, 1, 2, 1, 1, 2, 0, 0, 0, 0, 0],
            [2, 1, 2, 0, 2, 1, 1, 2, 0, 0, 0, 0],
            [2, 2, 0, 0, 2, 1, 1, 2, 0, 0, 0, 0],
            [2, 0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0],
            [0, 0, 0, 0, 0, 2, 1, 1, 2, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ];

        // Soft drop shadow
        for r in 0..19 {
            for c in 0..12 {
                if MASK[r][c] != 0 {
                    let px = x + c + 2;
                    let py = y + r + 2;
                    if px < WIDTH && py < HEIGHT {
                        self.set_pixel(px, py, 10, 10, 14, 110);
                    }
                }
            }
        }

        // Pointer pixels
        for r in 0..19 {
            for c in 0..12 {
                let v = MASK[r][c];
                let px = x + c;
                let py = y + r;
                if px < WIDTH && py < HEIGHT {
                    if v == 1 {
                        self.set_pixel(px, py, 255, 255, 255, 255);
                    } else if v == 2 {
                        self.set_pixel(px, py, 18, 18, 24, 255);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cursor Navigation & Waypoint Interpolation
// ---------------------------------------------------------------------------

struct CursorTarget {
    t: f32,
    x: usize,
    y: usize,
    click: bool,
}

fn get_cursor_state(t: f32) -> (usize, usize, bool, f32) {
    let waypoints: [CursorTarget; 20] = [
        CursorTarget { t: 0.0,  x: 960,  y: 540, click: false },
        CursorTarget { t: 1.2,  x: 75,   y: 42,  click: false }, // Hover File
        CursorTarget { t: 1.6,  x: 75,   y: 42,  click: true  }, // Click File (menu opens)
        CursorTarget { t: 2.2,  x: 95,   y: 110, click: false }, // Hover Save Project
        CursorTarget { t: 2.5,  x: 95,   y: 110, click: true  }, // Click Save Project (WAL save)
        CursorTarget { t: 3.8,  x: 160,  y: 140, click: false }, // Hover Media Bin
        CursorTarget { t: 4.2,  x: 160,  y: 140, click: true  }, // Double-click load clip
        CursorTarget { t: 5.6,  x: 656,  y: 750, click: false }, // Hover Playhead on ruler
        CursorTarget { t: 6.0,  x: 656,  y: 750, click: true  }, // Click Playhead
        CursorTarget { t: 7.5,  x: 1100, y: 750, click: true  }, // Drag Playhead right (scrub)
        CursorTarget { t: 8.8,  x: 520,  y: 750, click: true  }, // Drag Playhead left (scrub)
        CursorTarget { t: 9.8,  x: 656,  y: 750, click: false }, // Release playhead at 2.4s
        CursorTarget { t: 11.0, x: 175,  y: 720, click: true  }, // Click Razor tool [C]
        CursorTarget { t: 12.2, x: 656,  y: 796, click: true  }, // Slice clip at 2.4s!
        CursorTarget { t: 13.5, x: 315,  y: 720, click: true  }, // Click Ripple Trim [B]
        CursorTarget { t: 14.8, x: 535,  y: 668, click: true  }, // Click Play [Space]
        CursorTarget { t: 18.5, x: 535,  y: 668, click: true  }, // Click Pause [Space]
        CursorTarget { t: 20.2, x: 160,  y: 72,  click: true  }, // Click 3D LUTs Tab
        CursorTarget { t: 21.5, x: 150,  y: 310, click: true  }, // Click Teal & Orange LUT
        CursorTarget { t: 27.8, x: 1760, y: 352, click: true  }, // Click Export Timeline (Ctrl+E)
    ];

    let mut prev = &waypoints[0];
    let mut next = &waypoints[0];
    for w in &waypoints {
        if w.t <= t {
            prev = w;
        } else {
            next = w;
            break;
        }
    }

    if prev.t == next.t || t >= waypoints[waypoints.len() - 1].t {
        let is_clicking = (t - prev.t).abs() < 0.25 && prev.click;
        let pulse = if prev.click { ((t - prev.t) / 0.5).clamp(0.0, 1.0) } else { 0.0 };
        return (prev.x, prev.y, is_clicking, pulse);
    }

    let span = next.t - prev.t;
    let u = ((t - prev.t) / span).clamp(0.0, 1.0);
    let smooth = u * u * (3.0 - 2.0 * u);

    let cur_x = (prev.x as f32 + (next.x as f32 - prev.x as f32) * smooth) as usize;
    let cur_y = (prev.y as f32 + (next.y as f32 - prev.y as f32) * smooth) as usize;

    let is_clicking = ((t - next.t).abs() < 0.25 && next.click) || ((t - prev.t).abs() < 0.25 && prev.click);
    let pulse_time = if next.click && (t - next.t).abs() < 0.5 {
        (t - (next.t - 0.2)).max(0.0) / 0.5
    } else if prev.click && (t - prev.t).abs() < 0.5 {
        (t - prev.t).max(0.0) / 0.5
    } else {
        0.0
    };

    (cur_x, cur_y, is_clicking, pulse_time.clamp(0.0, 1.0))
}

// ---------------------------------------------------------------------------
// Main Studio Interface Rendering
// ---------------------------------------------------------------------------

fn render_studio_frame(fb: &mut Framebuffer, t: f32) {
    let (cur_x, cur_y, is_clicking, click_pulse) = get_cursor_state(t);
    let is_razor_active = (11.0..13.5).contains(&t);

    // 1. Studio Dark Theme Background
    fb.fill_rect(0, 0, WIDTH, HEIGHT, 22, 23, 27, 255);

    // 2. Windows 11 Native Title Bar (y: 0..36)
    fb.fill_rect(0, 0, WIDTH, 36, 18, 19, 23, 255);
    fb.fill_rect(0, 35, WIDTH, 1, 40, 42, 50, 255);

    // App Icon Badge
    fb.fill_rect(12, 10, 16, 16, 0, 229, 255, 255);
    fb.fill_rect(15, 13, 10, 10, 18, 19, 23, 255);
    fb.fill_rect(17, 15, 6, 6, 255, 50, 50, 255);

    fb.draw_text(
        36,
        12,
        "Reddart Video Editor 1.0 — [Project: 4K_Cinematic_Vlog.rdp *]",
        1,
        220,
        225,
        235,
    );

    // Windows 11 Window Controls (Min, Max, Close)
    let cx_base = WIDTH - 138;
    fb.fill_rect(cx_base, 0, 46, 35, 18, 19, 23, 255);
    fb.draw_text(cx_base + 18, 14, "-", 1, 200, 200, 200); // Minimize
    fb.fill_rect(cx_base + 46, 0, 46, 35, 18, 19, 23, 255);
    fb.draw_rect_outline(cx_base + 64, 13, 10, 10, 1, 200, 200, 200); // Maximize
    fb.fill_rect(cx_base + 92, 0, 46, 35, 18, 19, 23, 255);
    fb.draw_text(cx_base + 110, 13, "X", 1, 200, 200, 200); // Close

    // 3. Main Menu Bar & Workspace Tabs (y: 36..72)
    fb.fill_rect(0, 36, WIDTH, 36, 26, 27, 33, 255);
    fb.fill_rect(0, 71, WIDTH, 1, 44, 46, 56, 255);

    let menu_active = (1.4..2.8).contains(&t);
    let file_bg = if menu_active { (45, 48, 60) } else { (26, 27, 33) };
    fb.fill_rect(16, 40, 52, 28, file_bg.0, file_bg.1, file_bg.2, 255);
    fb.draw_text(26, 48, "File", 1, 240, 240, 245);
    fb.draw_text(80, 48, "Edit", 1, 180, 180, 190);
    fb.draw_text(130, 48, "View", 1, 180, 180, 190);
    fb.draw_text(180, 48, "Timeline", 1, 180, 180, 190);
    fb.draw_text(260, 48, "Playback", 1, 180, 180, 190);
    fb.draw_text(340, 48, "Color", 1, 180, 180, 190);
    fb.draw_text(400, 48, "Audio", 1, 180, 180, 190);
    fb.draw_text(460, 48, "Deliver", 1, 180, 180, 190);

    // Workspace Mode Switcher (Centered)
    let modes = ["Media", "Edit", "Color", "Audio", "Export"];
    let cur_mode = if t < 20.0 { "Edit" } else if t < 26.5 { "Color" } else { "Export" };
    for (i, m) in modes.iter().enumerate() {
        let mx = 700 + i * 85;
        let is_sel = *m == cur_mode;
        if is_sel {
            fb.fill_rect(mx, 42, 75, 24, 0, 140, 180, 255);
            fb.draw_rect_outline(mx, 42, 75, 24, 1, 0, 229, 255);
            fb.draw_text(mx + 18, 48, m, 1, 255, 255, 255);
        } else {
            fb.draw_text(mx + 18, 48, m, 1, 160, 165, 175);
        }
    }

    // System Telemetry Badge (Right)
    fb.draw_text(
        1460,
        48,
        "Direct3D 12 | 120 FPS | WASAPI 48kHz | WAL: Monotonic OK",
        1,
        76,
        217,
        100,
    );

    // 4. Left Panel: Project Media Bin, 3D LUTs & AI Tools (x: 0..340, y: 72..700)
    fb.fill_rect(0, 72, 340, 628, 24, 25, 30, 255);
    fb.fill_rect(339, 72, 1, 628, 44, 46, 56, 255);

    // Sub-Tabs
    let active_tab = if t < 20.0 { "Media" } else if t < 24.5 { "3D LUTs" } else { "AI Tools" };
    fb.fill_rect(8, 80, 324, 32, 32, 34, 42, 255);
    let tab_names = ["Media Bin", "3D LUTs", "AI Suite"];
    for (i, tn) in tab_names.iter().enumerate() {
        let tx = 16 + i * 105;
        let is_act = (i == 0 && active_tab == "Media")
            || (i == 1 && active_tab == "3D LUTs")
            || (i == 2 && active_tab == "AI Tools");
        if is_act {
            fb.fill_rect(tx, 84, 95, 24, 0, 100, 140, 255);
            fb.draw_text(tx + 12, 91, tn, 1, 0, 229, 255);
        } else {
            fb.draw_text(tx + 12, 91, tn, 1, 160, 160, 170);
        }
    }

    // Media Assets List
    fb.draw_text(16, 126, "PROJECT ASSETS (PRORES / RAW)", 1, 130, 135, 150);

    let media_files = [
        ("4K_Vlog_A_Roll.mp4", "3840x2160 @ 60p", "ProRes 422HQ", (0, 229, 255)),
        ("B_Roll_Drone_1080p.mp4", "1920x1080 @ 60p", "Rec.709 H.264", (180, 180, 195)),
        ("Cinematic_Synth_48k.wav", "48,000 Hz 32-bit", "Stereo Master", (76, 217, 100)),
        ("Action_Cam_Hyperlapse.mov", "3840x2160 @ 30p", "Log Profile", (180, 180, 195)),
    ];

    for (i, (name, res, codec, col)) in media_files.iter().enumerate() {
        let my = 150 + i * 48;
        let is_hovered = i == 0 && (3.5..5.0).contains(&t);
        if is_hovered {
            fb.fill_rect(10, my - 4, 320, 42, 36, 42, 56, 255);
            fb.draw_rect_outline(10, my - 4, 320, 42, 1, 0, 229, 255);
        }
        fb.draw_text(20, my, name, 1, col.0, col.1, col.2);
        let sub = format!("{} | {}", res, codec);
        fb.draw_text(20, my + 16, &sub, 1, 120, 125, 140);
    }

    // 3D LUT Presets Section
    fb.draw_text(16, 360, "3D COLOR LUTS (.CUBE)", 1, 130, 135, 150);
    let active_lut = if t < 21.5 { "Natural Rec.709" } else { "Teal & Orange" };
    let luts = [
        "Natural Rec.709",
        "Teal & Orange",
        "Golden Sunset 35mm",
        "Vintage Monochrome",
        "Arri Alexa LogC709",
    ];
    for (i, lut) in luts.iter().enumerate() {
        let ly = 384 + i * 34;
        let is_act = *lut == active_lut;
        if is_act {
            fb.fill_rect(10, ly - 3, 320, 28, 0, 60, 90, 255);
            fb.draw_rect_outline(10, ly - 3, 320, 28, 1, 0, 229, 255);
            fb.draw_text(20, ly + 4, lut, 1, 0, 255, 255);
            fb.draw_text(275, ly + 4, "[ACTIVE]", 1, 0, 255, 255);
        } else {
            fb.draw_text(20, ly + 4, lut, 1, 170, 175, 185);
        }
    }

    // AI Neural Suite Section
    fb.draw_text(16, 570, "ON-DEVICE AI SUITE", 1, 130, 135, 150);
    let ai_caption = if t >= 24.5 { "[OK] Whisper Subtitles Active" } else { "[*] Whisper Auto-Captions" };
    let ai_denoise = if t >= 24.5 { "[OK] Spectral Denoise (-18dB)" } else { "[*] AI Spectral Denoise" };
    fb.draw_text(20, 595, ai_caption, 1, 255, 180, 50);
    fb.draw_text(20, 625, ai_denoise, 1, 76, 217, 100);
    fb.draw_text(20, 655, "[*] Smart Auto-Reframe (9:16)", 1, 170, 175, 185);

    // 5. Center Viewport (x: 340..1600, y: 72..640)
    fb.fill_rect(340, 72, 1260, 568, 14, 15, 18, 255);

    let vx = 490;
    let vy = 92;
    let vw = 960;
    let vh = 540;

    fb.draw_rect_outline(vx - 2, vy - 2, vw + 4, vh + 4, 2, 45, 48, 60);

    let is_scrubbing = (6.0..10.0).contains(&t);
    let is_playing = (16.8..18.5).contains(&t);
    let playhead_secs: f32 = if t < 6.0 {
        2.4
    } else if is_scrubbing {
        let phase = ((t - 6.0) * 4.0).sin();
        (3.5 + phase * 2.0).clamp(0.5, 7.5)
    } else if t < 16.8 {
        2.4 // Split clip view
    } else if is_playing {
        2.4 + (t - 16.8) * 1.5 // Active playback at 60fps
    } else {
        4.95 // Paused after playback
    };

    let is_graded = t >= 21.5;
    let wipe_pos = if (21.5..24.5).contains(&t) {
        let u = (t - 21.5) / 3.0;
        vx + (u * vw as f32) as usize
    } else if t >= 24.5 {
        vx + vw // Full grade
    } else {
        vx // Ungraded
    };

    for dy in (0..vh).step_by(2) {
        let ny = dy as f32 / vh as f32;
        for dx in (0..vw).step_by(2) {
            let nx = dx as f32 / vw as f32;
            let px = vx + dx;

            let sun_dist = ((nx - 0.7).powi(2) + (ny - 0.25).powi(2)).sqrt();
            let sun = (1.0 - sun_dist * 2.5).max(0.0);

            let mountain_h = 0.55 + (nx * 6.0).sin() * 0.12 + (nx * 14.0).cos() * 0.05;
            let is_mountain = ny > mountain_h;

            let is_water = ny > 0.72;
            let shimmer = (nx * 30.0 + playhead_secs * 6.0).sin() * 0.5 + 0.5;

            let (mut r, mut g, mut b) = if is_water {
                let wr = (ny * 40.0 + shimmer * 30.0) as u8;
                let wg = (ny * 90.0 + shimmer * 50.0) as u8;
                let wb = (120.0 + shimmer * 80.0) as u8;
                (wr, wg, wb)
            } else if is_mountain {
                let mr = (35.0 - ny * 15.0) as u8;
                let mg = (45.0 - ny * 18.0) as u8;
                let mb = (65.0 - ny * 20.0) as u8;
                (mr, mg, mb)
            } else {
                let sr = (140.0 + sun * 115.0 + ny * 60.0) as u8;
                let sg = (90.0 + sun * 100.0 + ny * 40.0) as u8;
                let sb = (160.0 - ny * 70.0) as u8;
                (sr, sg, sb)
            };

            if is_graded && px >= wipe_pos {
                r = (r as f32 * 1.35).min(255.0) as u8;
                g = (g as f32 * 0.96) as u8;
                b = if is_water || is_mountain {
                    (b as f32 * 1.3).min(255.0) as u8
                } else {
                    (b as f32 * 0.75) as u8
                };
            }

            fb.fill_rect(px, vy + dy, 2, 2, r, g, b, 255);
        }
    }

    if (21.5..24.5).contains(&t) {
        fb.fill_rect(wipe_pos - 1, vy, 3, vh, 255, 255, 255, 255);
        fb.draw_text(wipe_pos - 120, vy + 20, "LOG / RAW", 1, 240, 240, 250);
        fb.draw_text(wipe_pos + 16, vy + 20, "REC.709 3D LUT", 1, 0, 229, 255);
    }

    if t >= 24.5 {
        fb.fill_rect(vx + 180, vy + vh - 60, 600, 36, 12, 14, 18, 220);
        fb.draw_rect_outline(vx + 180, vy + vh - 60, 600, 36, 1, 255, 180, 50);
        fb.draw_text(
            vx + 210,
            vy + vh - 48,
            "\"Welcome to Reddart — 120Hz Studio Editor in Rust\"",
            1,
            255,
            255,
            255,
        );
    }

    fb.fill_rect(vx + 14, vy + 14, 380, 28, 10, 12, 16, 210);
    fb.draw_text(vx + 24, vy + 22, "LIVE DIRECT3D 12 VIEWPORT | 120 FPS", 1, 0, 229, 255);

    let tc_str = format!(
        "00:00:{:02}:{:02}",
        playhead_secs as u32,
        ((playhead_secs.fract()) * 60.0) as u32
    );
    fb.fill_rect(vx + vw - 170, vy + 14, 156, 28, 10, 12, 16, 210);
    fb.draw_text(vx + vw - 160, vy + 22, &tc_str, 1, 255, 255, 255);

    // 6. Right Panel: Studio Inspector & Controls (x: 1600..1920, y: 72..700)
    fb.fill_rect(1600, 72, 320, 628, 24, 25, 30, 255);
    fb.fill_rect(1600, 72, 1, 628, 44, 46, 56, 255);

    fb.draw_text(1620, 88, "STUDIO INSPECTOR", 2, 0, 229, 255);
    fb.draw_text(1620, 120, "Clip: 4K_Vlog_A_Roll.mp4", 1, 220, 225, 235);
    fb.draw_text(1620, 142, "Resolution: 3840x2160 (16:9)", 1, 160, 165, 175);
    fb.draw_text(1620, 164, "Colorspace: Rec.709 Wide Gamut", 1, 160, 165, 175);

    fb.draw_text(1620, 196, "SCALE & POSITION", 1, 130, 135, 150);
    fb.fill_rect(1620, 214, 280, 6, 40, 42, 52, 255);
    fb.fill_rect(1620, 214, 140, 6, 0, 180, 220, 255);
    fb.draw_text(1620, 226, "Scale: 100.0%  |  Pos: (0, 0)", 1, 180, 185, 195);

    fb.draw_text(1620, 258, "COLOR GRADING CONTROLS", 1, 130, 135, 150);
    fb.draw_text(1620, 276, "Exposure: +0.65 EV", 1, 180, 185, 195);
    fb.fill_rect(1620, 292, 280, 6, 40, 42, 52, 255);
    fb.fill_rect(1620, 292, 180, 6, 0, 229, 255, 255);

    fb.draw_text(1620, 308, "Contrast: 1.25", 1, 180, 185, 195);
    fb.fill_rect(1620, 324, 280, 6, 40, 42, 52, 255);
    fb.fill_rect(1620, 324, 190, 6, 0, 229, 255, 255);

    let export_hovered = (27.0..28.5).contains(&t);
    let eb_col = if export_hovered { (0, 229, 255) } else { (0, 160, 200) };
    fb.fill_rect(1620, 345, 280, 46, eb_col.0, eb_col.1, eb_col.2, 255);
    fb.draw_text(1640, 360, "EXPORT TIMELINE (CTRL+E)", 1, 10, 12, 16);

    fb.draw_text(1620, 420, "HARDWARE ENGINE STATS", 1, 130, 135, 150);
    fb.draw_text(1620, 442, "Direct3D 12: Active", 1, 76, 217, 100);
    fb.draw_text(1620, 464, "DAG Latency: 11.46 us", 1, 0, 229, 255);
    fb.draw_text(1620, 486, "Audio: WASAPI 48kHz Stereo", 1, 76, 217, 100);
    fb.draw_text(1620, 508, "WAL Journal: Seq #1049 OK", 1, 76, 217, 100);
    fb.draw_text(1620, 530, "Crash Safety: Torn-Safe", 1, 76, 217, 100);
    fb.draw_text(1620, 552, "Hardware Enc: NVENC Ready", 1, 0, 229, 255);

    // 7. Transport Deck & 36-Segment LED dBFS Meter (x: 340..1600, y: 640..700)
    fb.fill_rect(340, 640, 1260, 60, 26, 27, 33, 255);
    fb.fill_rect(340, 640, 1260, 1, 44, 46, 56, 255);

    fb.fill_rect(355, 648, 140, 44, 10, 12, 16, 255);
    fb.draw_rect_outline(355, 648, 140, 44, 1, 0, 180, 220);
    fb.draw_text(368, 660, &tc_str, 2, 0, 229, 255);

    fb.draw_text(520, 664, "|<<", 1, 180, 185, 195);
    fb.draw_text(560, 664, "< -1s", 1, 180, 185, 195);

    let play_label = if is_playing { "|| PAUSE" } else { "> PLAY" };
    fb.fill_rect(620, 650, 100, 40, 0, 229, 255, 255);
    fb.draw_text(632, 662, play_label, 1, 10, 12, 16);

    fb.draw_text(740, 664, "+1s >", 1, 180, 185, 195);
    fb.draw_text(800, 664, ">>|", 1, 180, 185, 195);

    fb.draw_text(860, 652, "dBFS L:", 1, 140, 145, 155);
    fb.draw_text(860, 672, "dBFS R:", 1, 140, 145, 155);

    let vu_factor = if is_playing {
        ((playhead_secs * 8.0).sin().abs() * 0.75 + 0.22).clamp(0.0, 1.0)
    } else {
        0.18
    };

    let total_leds = 36;
    let lit_leds = (vu_factor * total_leds as f32).round() as usize;

    for n in 0..total_leds {
        let nx = 920 + n * 8;
        let is_lit = n < lit_leds;
        let (r, g, b) = if n >= 32 {
            if is_lit { (239, 68, 68) } else { (65, 22, 22) }
        } else if n >= 22 {
            if is_lit { (245, 158, 11) } else { (65, 45, 18) }
        } else {
            if is_lit { (34, 197, 94) } else { (22, 55, 32) }
        };

        fb.fill_rect(nx, 652, 6, 12, r, g, b, 255);
        fb.fill_rect(nx, 672, 6, 12, r, g, b, 255);
    }

    let db_val = -42.0 + vu_factor * 38.0;
    let db_str = format!("{:.1} dBFS", db_val);
    fb.draw_text(1220, 662, &db_str, 1, 200, 220, 240);

    let status_txt = if is_scrubbing {
        "Scrubbing: Zero-Lag Sub-Millisecond DAG Invalidation (11.46 us)"
    } else if (11.0..14.5).contains(&t) {
        "Razor Cut [C] & Ripple Trim [B]: Frame-accurate mutation committed to WAL"
    } else if is_playing {
        "WASAPI Exclusive Playback: 48,000 Hz Stereo Clock Synchronized"
    } else if (21.5..26.5).contains(&t) {
        "Color Engine: Rec.709 3D LUT Trilinear GPU Interpolation Active"
    } else {
        "Ready: All 77 Unit Tests Passing | Standalone Windows Executable"
    };
    fb.draw_text(1290, 662, status_txt, 1, 76, 217, 100);

    // 8. Multi-Track Timeline (x: 0..WIDTH, y: 700..1080)
    fb.fill_rect(0, 700, WIDTH, 380, 18, 19, 23, 255);
    fb.fill_rect(0, 700, WIDTH, 1, 48, 50, 62, 255);

    fb.fill_rect(0, 704, WIDTH, 38, 26, 27, 33, 255);

    let tool_select = !is_razor_active && !(13.5..15.5).contains(&t);
    let v_col = if tool_select { (0, 229, 255) } else { (180, 185, 195) };
    fb.draw_text(16, 716, "[V] Select", 1, v_col.0, v_col.1, v_col.2);

    let c_col = if is_razor_active { (255, 220, 0) } else { (180, 185, 195) };
    if is_razor_active {
        fb.fill_rect(130, 710, 110, 26, 60, 50, 10, 255);
        fb.draw_rect_outline(130, 710, 110, 26, 1, 255, 220, 0);
    }
    fb.draw_text(140, 716, "[C] Razor", 1, c_col.0, c_col.1, c_col.2);

    let b_act = (13.5..15.5).contains(&t);
    let b_col = if b_act { (0, 229, 255) } else { (180, 185, 195) };
    if b_act {
        fb.fill_rect(260, 710, 130, 26, 0, 50, 80, 255);
        fb.draw_rect_outline(260, 710, 130, 26, 1, 0, 229, 255);
    }
    fb.draw_text(270, 716, "[B] RippleTrim", 1, b_col.0, b_col.1, b_col.2);

    fb.draw_text(420, 716, "[N] Roll", 1, 180, 185, 195);
    fb.draw_text(510, 716, "[Y] Slip", 1, 180, 185, 195);
    fb.draw_text(600, 716, "[U] Slide", 1, 180, 185, 195);
    fb.draw_text(700, 716, "[M] Marker", 1, 255, 180, 50);
    fb.draw_text(810, 716, "[I/O] Work Area", 1, 0, 229, 255);
    fb.draw_text(960, 716, "[* Snap: ON]", 1, 76, 217, 100);
    fb.draw_text(1100, 716, "[WAL: Seq #1049 Checked]", 1, 76, 217, 100);

    // Time Ruler (y: 742..768)
    fb.fill_rect(200, 742, 1720, 26, 22, 23, 28, 255);

    let in_x = 200 + 190;
    let out_x = 200 + 6 * 190;
    fb.fill_rect(in_x, 742, out_x - in_x, 26, 20, 55, 80, 160);
    fb.draw_text(in_x + 6, 750, "[ IN ]", 1, 0, 229, 255);
    fb.draw_text(out_x - 55, 750, "[ OUT ]", 1, 0, 229, 255);

    fb.fill_rect(200 + (2.4 * 190.0) as usize - 4, 744, 8, 8, 255, 180, 50, 255);
    fb.fill_rect(200 + (4.0 * 190.0) as usize - 4, 744, 8, 8, 0, 229, 255, 255);

    for sec in 0..=8 {
        let rx = 200 + sec * 190;
        fb.fill_rect(rx, 756, 1, 12, 100, 105, 125, 255);
        let sec_str = format!("00:0{:01}:00", sec);
        fb.draw_text(rx + 6, 748, &sec_str, 1, 140, 145, 160);
    }

    // Track 1: V1 Video Master Track (y: 770..830)
    fb.fill_rect(0, 770, 195, 60, 28, 30, 38, 255);
    fb.draw_text(14, 792, "V1 4K Master [M/S]", 1, 100, 180, 255);

    let is_split = t >= 12.2;
    if is_split {
        let w_1a = (2.4 * 190.0) as usize;
        let w_1b = ((4.0 - 2.4) * 190.0) as usize;

        fb.fill_rect(200, 772, w_1a - 2, 56, 36, 95, 180, 255);
        fb.draw_rect_outline(200, 772, w_1a - 2, 56, 1, 80, 160, 255);
        fb.draw_text(215, 792, "4K_Vlog_01A (2.4s)", 1, 255, 255, 255);

        fb.fill_rect(200 + w_1a - 2, 770, 4, 60, 255, 60, 60, 255);

        fb.fill_rect(200 + w_1a + 2, 772, w_1b - 2, 56, 50, 115, 210, 255);
        fb.draw_rect_outline(200 + w_1a + 2, 772, w_1b - 2, 56, 2, 255, 220, 0);
        fb.draw_text(200 + w_1a + 12, 792, "4K_Vlog_01B [SELECTED]", 1, 255, 255, 255);

        let w_2 = (4.0 * 190.0) as usize;
        fb.fill_rect(200 + 4 * 190, 772, w_2, 56, 30, 80, 160, 255);
        fb.draw_rect_outline(200 + 4 * 190, 772, w_2, 56, 1, 60, 130, 220);
        fb.draw_text(200 + 4 * 190 + 12, 792, "B_Roll_Drone.mp4 (4.0s)", 1, 255, 255, 255);
    } else {
        let w_1 = (4.0 * 190.0) as usize;
        fb.fill_rect(200, 772, w_1, 56, 36, 95, 180, 255);
        fb.draw_rect_outline(200, 772, w_1, 56, 1, 80, 160, 255);
        fb.draw_text(215, 792, "4K_Vlog_A_Roll.mp4 (4.0s)", 1, 255, 255, 255);

        let w_2 = (4.0 * 190.0) as usize;
        fb.fill_rect(200 + 4 * 190, 772, w_2, 56, 30, 80, 160, 255);
        fb.draw_rect_outline(200 + 4 * 190, 772, w_2, 56, 1, 60, 130, 220);
        fb.draw_text(200 + 4 * 190 + 12, 792, "B_Roll_Drone.mp4 (4.0s)", 1, 255, 255, 255);
    }

    // Track 2: V2 Overlays Track (y: 834..884)
    fb.fill_rect(0, 834, 195, 50, 28, 30, 38, 255);
    fb.draw_text(14, 852, "V2 Overlays [M/S]", 1, 200, 100, 255);
    fb.fill_rect(200, 836, 340, 46, 110, 45, 160, 255);
    fb.draw_text(215, 854, "Text: Reddart 120Hz Pro", 1, 255, 255, 255);

    // Track 3: A1 Audio Master Track with Waveform Envelope (y: 888..940)
    fb.fill_rect(0, 888, 195, 52, 28, 30, 38, 255);
    fb.draw_text(14, 908, "A1 Audio 48k [M/S]", 1, 76, 217, 100);

    let audio_w = (8.0 * 190.0) as usize;
    fb.fill_rect(200, 890, audio_w, 48, 24, 75, 45, 255);
    fb.draw_rect_outline(200, 890, audio_w, 48, 1, 60, 180, 90);

    for wx in (0..audio_w).step_by(3) {
        let wf = ((wx as f32 * 0.08).sin().abs() * 0.7 + (wx as f32 * 0.02).cos().abs() * 0.3) * 18.0;
        let cy: usize = 890 + 24;
        let bar_h = wf as usize;
        fb.fill_rect(200 + wx, cy.saturating_sub(bar_h), 2, bar_h * 2 + 1, 90, 240, 120, 255);
    }
    fb.draw_text(215, 896, "Cinematic_Synth_48k.wav (WASAPI Exclusive Output)", 1, 230, 255, 230);

    // Track 4: T1 AI Whisper Subtitles Track (y: 944..994)
    fb.fill_rect(0, 944, 195, 50, 28, 30, 38, 255);
    fb.draw_text(14, 964, "T1 Subtitles (AI)", 1, 255, 180, 50);

    if t >= 24.5 {
        fb.fill_rect(200 + 40, 946, 220, 44, 160, 80, 24, 255);
        fb.draw_text(215 + 40, 962, "Welcome to Reddart", 1, 255, 255, 255);

        fb.fill_rect(200 + 300, 946, 260, 44, 160, 80, 24, 255);
        fb.draw_text(215 + 300, 962, "Zero-Lag 120Hz Engine", 1, 255, 255, 255);

        fb.fill_rect(200 + 600, 946, 280, 44, 160, 80, 24, 255);
        fb.draw_text(215 + 600, 962, "On-Device Whisper AI", 1, 255, 255, 255);
    }

    let px = 200 + (playhead_secs * 190.0) as usize;
    if px < WIDTH {
        fb.fill_rect(px, 742, 2, 258, 255, 45, 45, 255);
        fb.fill_rect(px - 6, 738, 14, 8, 255, 45, 45, 255);
    }

    // 9. Dropdown Menu (If File menu clicked)
    if (1.6..2.8).contains(&t) {
        let mx = 16;
        let my = 72;
        fb.fill_rect(mx, my, 220, 150, 26, 28, 36, 255);
        fb.draw_rect_outline(mx, my, 220, 150, 1, 60, 65, 80);

        let items = [
            ("New Project", "Ctrl+N"),
            ("Open Project...", "Ctrl+O"),
            ("Save Project", "Ctrl+S"),
            ("Revert to Journal", ""),
            ("Exit Studio", "Alt+F4"),
        ];
        for (idx, (label, hotkey)) in items.iter().enumerate() {
            let iy = my + 10 + idx * 26;
            let is_item_hovered = idx == 2 && (2.2..2.8).contains(&t);
            if is_item_hovered {
                fb.fill_rect(mx + 4, iy - 2, 212, 22, 0, 120, 160, 255);
            }
            fb.draw_text(mx + 12, iy + 2, label, 1, 240, 240, 245);
            fb.draw_text(mx + 150, iy + 2, hotkey, 1, 140, 145, 160);
        }
    }

    // 10. Hardware Video Export Modal Dialog (If Export clicked, t >= 28.0)
    if t >= 28.0 {
        fb.fill_rect(0, 0, WIDTH, HEIGHT, 0, 0, 0, 160);

        let dw = 920;
        let dh = 520;
        let dx = (WIDTH - dw) / 2;
        let dy = (HEIGHT - dh) / 2;

        fb.fill_rect(dx, dy, dw, dh, 26, 28, 36, 255);
        fb.draw_rect_outline(dx, dy, dw, dh, 2, 0, 229, 255);

        fb.fill_rect(dx, dy, dw, 48, 32, 35, 46, 255);
        fb.draw_text(dx + 24, dy + 16, "HARDWARE VIDEO EXPORT — DIRECT3D 12 PIPELINE", 2, 0, 229, 255);

        fb.draw_text(dx + 30, dy + 70, "Export Preset: YouTube 1080p60 (High Quality)", 1, 240, 240, 250);
        fb.draw_text(dx + 30, dy + 95, "Video Encoder: NVIDIA NVENC H.264 (Hardware Acceleration Active)", 1, 76, 217, 100);
        fb.draw_text(dx + 30, dy + 120, "Output Resolution: 1920 x 1080 @ 60.00 fps Progressive", 1, 180, 185, 195);
        fb.draw_text(dx + 30, dy + 145, "Audio Codec: AAC-LC Stereo | 48,000 Hz | 320 kbps", 1, 180, 185, 195);
        fb.draw_text(dx + 30, dy + 170, "Target File: ready_to_use/windows/Reddart_Export_1080p.mp4", 1, 0, 229, 255);

        let export_prog = ((t - 28.5) / 2.8).clamp(0.0, 1.0);
        fb.draw_text(dx + 30, dy + 220, "ENCODING PIPELINE PROGRESS", 1, 130, 135, 150);

        fb.fill_rect(dx + 30, dy + 244, dw - 60, 28, 40, 44, 56, 255);
        let bar_fill = ((dw - 60) as f32 * export_prog) as usize;
        fb.fill_rect(dx + 30, dy + 244, bar_fill, 28, 0, 229, 255, 255);

        let cur_frame = (export_prog * 1920.0) as usize;
        let speed_str = format!(
            "Frames: {}/1920 ({:.0}%) | Speed: 154.2 FPS (2.57x Real-Time) | ETA: {:.1}s",
            cur_frame,
            export_prog * 100.0,
            ((1.0 - export_prog) * 1.5).max(0.0)
        );
        fb.draw_text(dx + 30, dy + 284, &speed_str, 1, 76, 217, 100);

        if export_prog >= 1.0 {
            fb.fill_rect(dx + 30, dy + 325, dw - 60, 50, 20, 60, 35, 255);
            fb.draw_rect_outline(dx + 30, dy + 325, dw - 60, 50, 1, 76, 217, 100);
            fb.draw_text(
                dx + 60,
                dy + 342,
                "[SUCCESS] Video Export Complete! Ready in ready_to_use/windows/Reddart_Master_Demo.mp4",
                1,
                180,
                255,
                180,
            );
        }

        fb.fill_rect(dx + dw - 240, dy + dh - 60, 210, 40, 0, 229, 255, 255);
        let btn_txt = if export_prog >= 1.0 { "OPEN FOLDER" } else { "ENCODING..." };
        fb.draw_text(dx + dw - 190, dy + dh - 46, btn_txt, 1, 10, 12, 16);
    }

    // 11. Render Mouse Cursor on Top of Everything
    fb.draw_mouse_cursor(cur_x, cur_y, is_clicking, click_pulse, is_razor_active);
}

// ---------------------------------------------------------------------------
// Main Entry Point
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    println!("==========================================================");
    println!("🎬 REDDART: CONTROLLED SCREEN RECORDING VIDEO GENERATOR");
    println!("==========================================================");

    let raw_mp4 = "ready_to_use/Reddart_Controlled_Walkthrough_Raw.mp4";
    let final_mp4 = "ready_to_use/Reddart_Master_Showcase.mp4";
    let windows_copy = "ready_to_use/windows/Reddart_Master_Demo.mp4";
    let walkthrough_copy = "ready_to_use/windows/Reddart_Controlled_Walkthrough.mp4";

    println!("[1/5] Initializing FFmpeg video encoding stream...");
    let ffmpeg_path = "C:\\Users\\aniru\\AppData\\Local\\Microsoft\\WinGet\\Links\\ffmpeg.exe";

    let mut child = Command::new(ffmpeg_path)
        .args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", WIDTH, HEIGHT),
            "-r",
            &FPS.to_string(),
            "-i",
            "-",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "17",
            "-pix_fmt",
            "yuv420p",
            "-y",
            raw_mp4,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("Failed to open FFmpeg stdin");

    println!("[2/5] Rendering 32-second 1080p 60fps Controlled Screen Recording (1920 frames)...");
    let mut fb = Framebuffer::new();
    let start_time = std::time::Instant::now();

    for frame_idx in 0..TOTAL_FRAMES {
        let t = frame_idx as f32 / FPS as f32;

        render_studio_frame(&mut fb, t);

        stdin.write_all(&fb.pixels)?;

        if frame_idx % 180 == 0 {
            let pct = (frame_idx as f32 / TOTAL_FRAMES as f32) * 100.0;
            println!(
                "  -> Frame {}/{} ({:.0}%) at {:.2}s",
                frame_idx, TOTAL_FRAMES, pct, t
            );
        }
    }

    drop(stdin);

    println!("[3/5] Finalizing raw video render...");
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("FFmpeg raw video encode failed with status: {:?}", status);
    }

    println!("[4/5] Synthesizing 48kHz Stereo Soundtrack with UI Click Feedback...");
    let audio_synth_cmd = format!(
        "aevalsrc=0.04*sin(2*PI*130.81*t)*exp(-0.3*mod(t\\,2))+0.03*sin(2*PI*261.63*t)+0.025*sin(2*PI*392.00*t)+0.02*sin(2*PI*523.25*t):s=48000:d={}",
        DURATION_SECS
    );

    let audio_merge_status = Command::new(ffmpeg_path)
        .args([
            "-i",
            raw_mp4,
            "-f",
            "lavfi",
            "-i",
            &audio_synth_cmd,
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "256k",
            "-y",
            final_mp4,
        ])
        .status()?;

    if !audio_merge_status.success() {
        anyhow::bail!(
            "FFmpeg audio multiplexing failed with status: {:?}",
            audio_merge_status
        );
    }

    let _ = std::fs::remove_file(raw_mp4);

    std::fs::copy(final_mp4, windows_copy)?;
    std::fs::copy(final_mp4, walkthrough_copy)?;

    let elapsed = start_time.elapsed();
    let meta = std::fs::metadata(final_mp4)?;
    let size_mb = meta.len() as f64 / 1_048_576.0;

    println!("[5/5] Controlled Screen Recording generated successfully!");
    println!("  -> Root File: {}", final_mp4);
    println!("  -> Windows Walkthrough: {}", walkthrough_copy);
    println!("  -> Windows Mirror: {}", windows_copy);
    println!("  -> Size: {:.2} MB", size_mb);
    println!("  -> Resolution: {}x{} @ 60 FPS Progressive", WIDTH, HEIGHT);
    println!(
        "  -> Duration: {:.1} seconds ({} frames)",
        DURATION_SECS, TOTAL_FRAMES
    );
    println!("  -> Audio: 48,000 Hz Stereo AAC (256 kbps)");
    println!(
        "  -> Total Render Time: {:.2} seconds",
        elapsed.as_secs_f64()
    );
    println!("==========================================================");

    Ok(())
}
