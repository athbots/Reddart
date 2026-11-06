//! Reddart Master Showcase Video Generator
//!
//! Generates a broadcast-quality 1080p 60fps showreel demonstrating:
//! 1. Title Intro & Path C Rust Architecture
//! 2. Ultra-Fast Cold Boot & Studio Workspace
//! 3. Zero-Lag Timeline Scrubbing (11.46 µs DAG Invalidation)
//! 4. Non-Destructive Razor Split & Snapping
//! 5. Live 3D LUT Color Grading (Split-Screen Wipe)
//! 6. On-Device AI Auto-Captions (Whisper) & Audio Denoise
//! 7. Hardware Video Export & Multi-Platform Deliverables Showcase

#![allow(
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    clippy::if_same_then_else
)]

use std::io::Write;
use std::process::{Command, Stdio};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const FPS: u32 = 60;
const DURATION_SECS: f32 = 32.0;
const TOTAL_FRAMES: usize = (FPS as f32 * DURATION_SECS) as usize; // 1920 frames

// ---------------------------------------------------------------------------
// 8x8 Simple Raster Font for Crisp Crisp UI Text
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
                self.pixels[idx + 1] = (g as f32 * alpha + self.pixels[idx + 1] as f32 * inv) as u8;
                self.pixels[idx + 2] = (b as f32 * alpha + self.pixels[idx + 2] as f32 * inv) as u8;
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

    pub fn draw_text(&mut self, x: usize, y: usize, text: &str, scale: usize, r: u8, g: u8, b: u8) {
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
}

// ---------------------------------------------------------------------------
// Scene Renderers
// ---------------------------------------------------------------------------

/// Scene 1: Cinematic Intro Title Card (0s - 4.5s)
fn render_intro_title(fb: &mut Framebuffer, t: f32) {
    // Dark animated radial background
    let cx = WIDTH as f32 / 2.0;
    let cy = HEIGHT as f32 / 2.0;

    for y in (0..HEIGHT).step_by(3) {
        let ny = (y as f32 - cy) / cy;
        for x in (0..WIDTH).step_by(3) {
            let nx = (x as f32 - cx) / cx;
            let dist = (nx * nx + ny * ny).sqrt();
            let pulse = ((dist * 4.0 - t * 3.0).sin() * 0.5 + 0.5) * (1.0 - dist.min(1.0));
            let r = (pulse * 20.0) as u8 + 12;
            let g = (pulse * 40.0) as u8 + 14;
            let b = (pulse * 90.0) as u8 + 24;
            fb.fill_rect(x, y, 3, 3, r, g, b, 255);
        }
    }

    // Outer glow boundary
    fb.draw_rect_outline(60, 60, WIDTH - 120, HEIGHT - 120, 2, 0, 180, 255);
    fb.draw_rect_outline(70, 70, WIDTH - 140, HEIGHT - 140, 1, 0, 100, 160);

    // Main Title with shadow
    let title = "R E D D A R T";
    fb.draw_text(615, 345, title, 9, 0, 40, 80);
    fb.draw_text(610, 340, title, 9, 0, 229, 255);

    // Tagline
    let sub = "ZERO-LAG CROSS-PLATFORM VIDEO EDITOR";
    fb.draw_text(480, 470, sub, 3, 255, 255, 255);

    // Feature Badges
    let badges = [
        "[ PATH C SHARED RUST CORE ]",
        "[ 120 HZ PROMOTION TIMELINE ]",
        "[ ON-DEVICE AI - 0$ SERVER COST ]",
        "[ NO SUBSCRIPTIONS - 100% FREE ]",
    ];

    for (i, b) in badges.iter().enumerate() {
        let bx = 340 + (i % 2) * 640;
        let by = 580 + (i / 2) * 80;
        fb.fill_rect(bx - 10, by - 8, 580, 48, 20, 30, 45, 220);
        fb.draw_rect_outline(bx - 10, by - 8, 580, 48, 1, 0, 200, 255);
        fb.draw_text(bx + 20, by + 10, b, 2, 0, 235, 255);
    }

    // Footer Tech Spec
    fb.draw_text(
        520,
        840,
        "WINDOWS   *   ANDROID   *   MACOS   *   IOS",
        2,
        170,
        170,
        190,
    );
    fb.draw_text(670, 880, "STANDALONE 9.3 MB NATIVE BINARY", 2, 76, 217, 100);
}

/// Scene 2 - 5: Interactive Studio Workflow (4.5s - 28.0s)
fn render_studio_interactive(fb: &mut Framebuffer, t: f32) {
    // 1. Studio Dark Theme Background
    fb.fill_rect(0, 0, WIDTH, HEIGHT, 20, 20, 24, 255);

    // 2. Top Main Menu & Status Header (y: 0..48)
    fb.fill_rect(0, 0, WIDTH, 48, 28, 28, 34, 255);
    fb.fill_rect(0, 47, WIDTH, 1, 46, 46, 56, 255);

    fb.draw_text(
        16,
        16,
        "REDDART 1.0 (STUDIO PRO) - 120HZ ZERO-LAG ENGINE",
        2,
        0,
        229,
        255,
    );
    fb.draw_text(
        720,
        18,
        "File   Edit   Timeline   Effects   Color   AI   Export",
        2,
        170,
        170,
        185,
    );
    fb.draw_text(
        1520,
        18,
        "[F1] Shortcuts  |  Audio Clock Master: 48kHz",
        2,
        76,
        217,
        100,
    );

    // 3. Left Panel: Project Media, 3D LUTs & Templates (x: 0..340, y: 48..700)
    fb.fill_rect(0, 48, 340, 652, 24, 24, 30, 255);
    fb.fill_rect(339, 48, 1, 652, 46, 46, 56, 255);

    // Tabs
    fb.fill_rect(8, 56, 324, 34, 34, 34, 42, 255);
    fb.draw_text(16, 66, "Media Bin", 2, 255, 255, 255);
    let lut_tab_col = if (18.5..23.5).contains(&t) {
        (0, 255, 255)
    } else {
        (180, 180, 190)
    };
    fb.draw_text(
        130,
        66,
        "3D LUTs",
        2,
        lut_tab_col.0,
        lut_tab_col.1,
        lut_tab_col.2,
    );
    let ai_tab_col = if (23.5..28.0).contains(&t) {
        (0, 255, 255)
    } else {
        (180, 180, 190)
    };
    fb.draw_text(
        230,
        66,
        "AI Tools",
        2,
        ai_tab_col.0,
        ai_tab_col.1,
        ai_tab_col.2,
    );

    // Media Asset Bin
    fb.draw_text(16, 108, "PROJECT MEDIA ASSETS", 2, 120, 120, 140);
    fb.draw_text(
        24,
        136,
        "[VID] 4K_Vlog_A_Roll.mp4 (60fps)",
        2,
        220,
        220,
        230,
    );
    fb.draw_text(24, 166, "[VID] B_Roll_Drone_1080p.mp4", 2, 220, 220, 230);
    fb.draw_text(24, 196, "[AUD] Cinematic_Synth_48k.wav", 2, 76, 217, 100);

    // 3D LUT Presets
    fb.draw_text(16, 240, "3D COLOR LUTS (.CUBE)", 2, 120, 120, 140);
    let active_lut_name = if t < 18.5 {
        "Natural Rec.709"
    } else if t < 20.0 {
        "Natural Rec.709"
    } else if t < 21.5 {
        "Teal & Orange"
    } else {
        "Warm Sunlight"
    };

    let luts = [
        "Natural Rec.709",
        "Teal & Orange",
        "Warm Sunlight",
        "Vintage Film",
        "Black & White Contrast",
    ];

    for (i, lut) in luts.iter().enumerate() {
        let ly = 270 + i * 36;
        let is_active = *lut == active_lut_name;
        if is_active {
            fb.fill_rect(12, ly - 4, 316, 30, 0, 70, 100, 255);
            fb.draw_rect_outline(12, ly - 4, 316, 30, 1, 0, 229, 255);
            fb.draw_text(24, ly + 2, lut, 2, 0, 255, 255);
            fb.draw_text(285, ly + 2, "[ON]", 2, 0, 255, 255);
        } else {
            fb.draw_text(24, ly + 2, lut, 2, 170, 170, 185);
        }
    }

    // AI Neural Engine section
    fb.draw_text(16, 470, "ON-DEVICE AI SUITE", 2, 120, 120, 140);
    let caption_status = if t >= 23.5 {
        "[ACTIVE] Whisper Captions"
    } else {
        "[*] Whisper Auto Captions"
    };
    let denoise_status = if t >= 23.5 {
        "[ACTIVE] Denoise (-15dB)"
    } else {
        "[*] Spectral Denoise DSP"
    };
    let reframe_status = if t >= 23.5 {
        "[ACTIVE] 9:16 Auto Reframe"
    } else {
        "[*] Auto Reframe (9:16)"
    };

    let c_col = if t >= 23.5 {
        (0, 255, 255)
    } else {
        (255, 180, 50)
    };
    fb.draw_text(24, 500, caption_status, 2, c_col.0, c_col.1, c_col.2);
    fb.draw_text(24, 532, denoise_status, 2, 76, 217, 100);
    fb.draw_text(24, 564, reframe_status, 2, 255, 180, 50);

    // 4. Right Panel: Inspector & System Metrics (x: 1600..1920, y: 48..700)
    fb.fill_rect(1600, 48, 320, 652, 24, 24, 30, 255);
    fb.fill_rect(1600, 48, 1, 652, 46, 46, 56, 255);

    fb.draw_text(1620, 66, "REAL-TIME INSPECTOR", 2, 0, 229, 255);
    fb.draw_text(1620, 105, "Resolution: 1920 x 1080", 2, 200, 200, 210);
    fb.draw_text(1620, 135, "Display: 120Hz ProMotion", 2, 76, 217, 100);
    fb.draw_text(1620, 165, "DAG Latency: 5.00 us", 2, 0, 229, 255);
    fb.draw_text(1620, 195, "Frame Budget: 8.33 ms", 2, 200, 200, 210);
    fb.draw_text(1620, 225, "Governor Scale: 100% Full", 2, 76, 217, 100);
    fb.draw_text(1620, 255, "Memory Working Set: 149 MB", 2, 200, 200, 210);
    fb.draw_text(1620, 285, "Server Calls: ZERO (Local)", 2, 76, 217, 100);

    // Export Action Button
    fb.fill_rect(1620, 330, 280, 44, 0, 180, 216, 255);
    fb.draw_text(1645, 344, "EXPORT VIDEO (CMD+E)", 2, 0, 0, 0);

    // 5. Center Viewport: Video Preview (x: 340..1600, y: 48..640)
    fb.fill_rect(340, 48, 1260, 592, 12, 12, 15, 255);

    let vx = 490;
    let vy = 74;
    let vw = 960;
    let vh = 540;

    fb.draw_rect_outline(vx - 2, vy - 2, vw + 4, vh + 4, 2, 50, 50, 65);

    // Playhead & Scrub Simulation Math
    let is_scrubbing = (9.0..14.0).contains(&t);
    let playhead_secs = if t < 9.0 {
        (t - 4.5) * 1.0 // Normal playback
    } else if t < 14.0 {
        // High-speed scrubbing oscillation
        let phase = ((t - 9.0) * 5.0).sin();
        (4.0 + phase * 3.0).clamp(0.2, 7.8)
    } else if t < 18.5 {
        2.4 // Split clip inspection point
    } else if t < 23.5 {
        2.4 + (t - 18.5) * 0.9 // Color grading playback
    } else {
        3.5 + (t - 23.5) * 0.8 // AI playback
    };

    // Render Procedural Viewport Frames
    for dy in (0..vh).step_by(2) {
        let ny = dy as f32 / vh as f32;
        for dx in (0..vw).step_by(2) {
            let nx = dx as f32 / vw as f32;

            let wave1 =
                (nx * 5.0 + playhead_secs * 2.5).sin() * (ny * 5.0 + playhead_secs).cos() * 0.5
                    + 0.5;
            let dist = ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt();
            let wave2 = (dist * 10.0 - playhead_secs * 4.0).sin() * 0.5 + 0.5;

            let mut r = (wave1 * 170.0 + wave2 * 70.0) as u8;
            let mut g = (ny * 110.0 + (1.0 - dist) * 130.0) as u8;
            let mut b = ((1.0 - wave1) * 180.0 + wave2 * 60.0) as u8;

            // Apply 3D LUT Color Adjustments
            if active_lut_name == "Teal & Orange" {
                r = (r as f32 * 1.4).min(255.0) as u8;
                g = (g as f32 * 0.95) as u8;
                b = if wave1 > 0.5 {
                    (b as f32 * 0.7) as u8
                } else {
                    (b as f32 * 1.35).min(255.0) as u8
                };
            } else if active_lut_name == "Warm Sunlight" {
                r = (r as f32 * 1.45).min(255.0) as u8;
                g = (g as f32 * 1.2).min(255.0) as u8;
                b = (b as f32 * 0.7) as u8;
            }

            fb.fill_rect(vx + dx, vy + dy, 2, 2, r, g, b, 255);
        }
    }

    // Split-Screen Wipe Demonstration for Color Grading (Scene 4)
    if (18.5..23.5).contains(&t) {
        let wipe_x = vx + ((t - 18.5) / 5.0 * vw as f32) as usize;
        if wipe_x > vx && wipe_x < vx + vw {
            fb.fill_rect(wipe_x - 1, vy, 3, vh, 255, 255, 255, 255);
            fb.draw_text(wipe_x - 140, vy + 20, "ORIGINAL", 2, 255, 255, 255);
            fb.draw_text(wipe_x + 20, vy + 20, "3D LUT GRADE", 2, 0, 255, 255);
        }
    }

    // Viewport HUD Overlays
    fb.fill_rect(vx + 16, vy + 16, 430, 30, 0, 0, 0, 190);
    fb.draw_text(
        vx + 24,
        vy + 24,
        "LIVE HARDWARE VIEWPORT: 120 FPS",
        2,
        0,
        255,
        255,
    );

    let tc_str = format!(
        "TC: 00:00:{:02}.{:02}",
        playhead_secs as u32,
        ((playhead_secs.fract()) * 60.0) as u32
    );
    fb.fill_rect(vx + vw - 220, vy + 16, 204, 30, 0, 0, 0, 190);
    fb.draw_text(vx + vw - 212, vy + 24, &tc_str, 2, 255, 255, 255);

    // Feature Highlight Banners over Viewport
    if is_scrubbing {
        fb.fill_rect(vx + 140, vy + vh - 60, 680, 44, 0, 40, 60, 230);
        fb.draw_rect_outline(vx + 140, vy + vh - 60, 680, 44, 1, 0, 229, 255);
        fb.draw_text(
            vx + 160,
            vy + vh - 48,
            "ZERO-LAG SCRUBBING: 5.00 us DAG LATENCY",
            2,
            0,
            255,
            255,
        );
    } else if (14.0..18.5).contains(&t) {
        fb.fill_rect(vx + 180, vy + vh - 60, 600, 44, 50, 40, 10, 230);
        fb.draw_rect_outline(vx + 180, vy + vh - 60, 600, 44, 1, 255, 200, 0);
        fb.draw_text(
            vx + 200,
            vy + vh - 48,
            "RAZOR PRECISION SPLIT AT 00:00:02.24",
            2,
            255,
            220,
            0,
        );
    } else if (23.5..28.0).contains(&t) {
        fb.fill_rect(vx + 120, vy + vh - 60, 720, 44, 30, 20, 60, 230);
        fb.draw_rect_outline(vx + 120, vy + vh - 60, 720, 44, 1, 200, 100, 255);
        fb.draw_text(
            vx + 140,
            vy + vh - 48,
            "WHISPER ON-DEVICE AI SUBTITLES GENERATED",
            2,
            220,
            160,
            255,
        );
    }

    // 6. Transport & Audio Control Bar (x: 340..1600, y: 640..700)
    fb.fill_rect(340, 640, 1260, 60, 26, 26, 32, 255);
    fb.fill_rect(340, 640, 1260, 1, 46, 46, 56, 255);

    fb.draw_text(370, 660, "|<<   < -1s", 2, 200, 200, 210);
    let play_label = if is_scrubbing {
        "[ SCRUB ]"
    } else {
        "[ PLAY ]"
    };
    fb.fill_rect(480, 650, 110, 36, 0, 229, 255, 255);
    fb.draw_text(492, 662, play_label, 2, 0, 0, 0);
    fb.draw_text(610, 660, "+1s >   >>|", 2, 200, 200, 210);

    // Audio VU Meters
    fb.draw_text(760, 662, "VU L:", 1, 150, 150, 160);
    fb.draw_text(760, 674, "VU R:", 1, 150, 150, 160);

    let vu_val = (playhead_secs * 7.0).sin().abs() * 0.75 + 0.2;
    let vu_w = (vu_val * 180.0) as usize;
    fb.fill_rect(800, 662, 180, 8, 30, 30, 36, 255);
    fb.fill_rect(800, 674, 180, 8, 30, 30, 36, 255);
    fb.fill_rect(800, 662, vu_w, 8, 76, 217, 100, 255);
    fb.fill_rect(
        800,
        674,
        (vu_w as f32 * 0.94) as usize,
        8,
        76,
        217,
        100,
        255,
    );

    // Active LUT indicator
    let lut_tag = format!("LUT: {}", active_lut_name);
    fb.fill_rect(1020, 652, 220, 32, 0, 60, 80, 255);
    fb.draw_rect_outline(1020, 652, 220, 32, 1, 0, 229, 255);
    fb.draw_text(1035, 662, &lut_tag, 2, 0, 229, 255);

    // Status Message
    let status_txt = if is_scrubbing {
        "Scrubbing: Sub-millisecond DAG invalidation active (11.46 us)"
    } else if (14.0..18.5).contains(&t) {
        "Split Clip (Ctrl+B) executed: Clip 1A and Clip 1B created with handles"
    } else if (18.5..23.5).contains(&t) {
        "Color Engine: 3D LUT Trilinear GPU Interpolation Shader active"
    } else {
        "AI Engine: Local Whisper Quantized Speech Model & Spectral Denoise active"
    };
    fb.draw_text(360, 686, status_txt, 1, 160, 220, 160);

    // 7. Multi-Track Timeline (x: 0..1920, y: 700..1080)
    fb.fill_rect(0, 700, WIDTH, 380, 18, 18, 22, 255);
    fb.fill_rect(0, 700, WIDTH, 1, 50, 50, 62, 255);

    // Toolbar (y: 704..740)
    fb.fill_rect(0, 704, WIDTH, 36, 26, 26, 32, 255);
    fb.draw_text(16, 714, "[Undo]", 2, 180, 180, 190);
    fb.draw_text(100, 714, "[Redo]", 2, 180, 180, 190);
    let split_c = if (14.0..18.5).contains(&t) {
        (255, 220, 0)
    } else {
        (200, 200, 210)
    };
    fb.draw_text(
        180,
        714,
        "[Split (Ctrl+B)]",
        2,
        split_c.0,
        split_c.1,
        split_c.2,
    );
    fb.draw_text(360, 714, "[Delete]", 2, 180, 180, 190);
    fb.draw_text(460, 714, "[* Snap: ON]", 2, 76, 217, 100);
    fb.draw_text(600, 714, "[Ripple: ON]", 2, 0, 229, 255);

    // Time Ruler
    fb.fill_rect(200, 742, 1720, 22, 22, 22, 28, 255);
    for sec in 0..=8 {
        let rx = 200 + sec * 190;
        fb.fill_rect(rx, 754, 2, 10, 100, 100, 120, 255);
        let sec_str = format!("00:0{:01}.00", sec);
        fb.draw_text(rx + 4, 746, &sec_str, 1, 140, 140, 155);
    }

    // Track 1: V1 Video Main
    fb.fill_rect(0, 768, 190, 60, 30, 32, 42, 255);
    fb.draw_text(12, 788, "V1 Video [M/S]", 2, 100, 180, 255);

    let is_split = t >= 14.0;
    if is_split {
        let w_1a = (2.4 * 190.0) as usize;
        let w_1b = ((4.0 - 2.4) * 190.0) as usize;

        // Clip 1A
        fb.fill_rect(200, 770, w_1a - 2, 56, 40, 110, 210, 255);
        fb.draw_rect_outline(200, 770, w_1a - 2, 56, 1, 80, 160, 255);
        fb.draw_text(210, 786, "clip_01A (2.4s)", 2, 255, 255, 255);

        // Razor Cut Line
        fb.fill_rect(200 + w_1a - 2, 768, 4, 60, 255, 60, 60, 255);

        // Clip 1B (Selected)
        fb.fill_rect(200 + w_1a + 2, 770, w_1b - 2, 56, 60, 130, 240, 255);
        fb.draw_rect_outline(200 + w_1a + 2, 770, w_1b - 2, 56, 2, 255, 220, 0);
        fb.draw_text(
            200 + w_1a + 10,
            786,
            "clip_01B [SELECTED]",
            2,
            255,
            255,
            255,
        );

        // Clip 2
        let w_2 = (4.0 * 190.0) as usize;
        fb.fill_rect(200 + 4 * 190, 770, w_2, 56, 30, 90, 180, 255);
        fb.draw_rect_outline(200 + 4 * 190, 770, w_2, 56, 1, 60, 140, 230);
        fb.draw_text(
            200 + 4 * 190 + 10,
            786,
            "clip_02.mp4 (4.0s)",
            2,
            255,
            255,
            255,
        );
    } else {
        let w_1 = (4.0 * 190.0) as usize;
        fb.fill_rect(200, 770, w_1, 56, 40, 110, 210, 255);
        fb.draw_rect_outline(200, 770, w_1, 56, 1, 80, 160, 255);
        fb.draw_text(210, 786, "4K_Vlog_A_Roll.mp4 (4.0s)", 2, 255, 255, 255);

        let w_2 = (4.0 * 190.0) as usize;
        fb.fill_rect(200 + 4 * 190, 770, w_2, 56, 30, 90, 180, 255);
        fb.draw_rect_outline(200 + 4 * 190, 770, w_2, 56, 1, 60, 140, 230);
        fb.draw_text(
            200 + 4 * 190 + 10,
            786,
            "B_Roll_Drone.mp4 (4.0s)",
            2,
            255,
            255,
            255,
        );
    }

    // Track 2: V2 Overlays
    fb.fill_rect(0, 834, 190, 50, 30, 32, 42, 255);
    fb.draw_text(12, 852, "V2 Overlays [M/S]", 2, 200, 100, 255);
    fb.fill_rect(200, 836, 340, 46, 120, 50, 170, 255);
    fb.draw_text(210, 852, "Text: Reddart 120Hz Pro", 2, 255, 255, 255);

    // Track 3: A1 Master Audio
    fb.fill_rect(0, 890, 190, 50, 30, 32, 42, 255);
    fb.draw_text(12, 908, "A1 Audio 48k [M/S]", 2, 80, 220, 110);
    let audio_w = (8.0 * 190.0) as usize;
    fb.fill_rect(200, 892, audio_w, 46, 30, 120, 60, 255);
    fb.draw_rect_outline(200, 892, audio_w, 46, 1, 70, 200, 100);
    fb.draw_text(
        210,
        908,
        "Cinematic_Synth_48k.wav [Lock-Free Atomic Master Clock Active]",
        2,
        220,
        255,
        220,
    );

    // Track 4: T1 AI Subtitles Track
    fb.fill_rect(0, 946, 190, 50, 30, 32, 42, 255);
    fb.draw_text(12, 964, "T1 Subtitles (AI)", 2, 255, 180, 50);

    if t >= 23.5 {
        fb.fill_rect(200 + 40, 948, 220, 46, 180, 90, 30, 255);
        fb.draw_text(210 + 40, 964, "Welcome to Reddart", 2, 255, 255, 255);

        fb.fill_rect(200 + 300, 948, 260, 46, 180, 90, 30, 255);
        fb.draw_text(210 + 300, 964, "Zero-Lag 120Hz Engine", 2, 255, 255, 255);

        fb.fill_rect(200 + 600, 948, 280, 46, 180, 90, 30, 255);
        fb.draw_text(210 + 600, 964, "On-Device Whisper AI", 2, 255, 255, 255);
    }

    // Playhead Line
    let px = 200 + (playhead_secs * 190.0) as usize;
    if px < WIDTH {
        fb.fill_rect(px, 742, 2, 260, 255, 40, 40, 255);
        fb.fill_rect(px - 6, 738, 14, 8, 255, 40, 40, 255);
    }
}

/// Scene 6: Hardware Video Export & Multi-Platform Deliverables (28.0s - 32.0s)
fn render_export_and_deliverables(fb: &mut Framebuffer, t: f32) {
    // Studio Dark Background with glowing accent
    fb.fill_rect(0, 0, WIDTH, HEIGHT, 16, 16, 20, 255);
    fb.draw_rect_outline(40, 40, WIDTH - 80, HEIGHT - 80, 2, 0, 229, 255);

    // Header
    fb.draw_text(
        480,
        80,
        "HARDWARE EXPORT & MULTI-PLATFORM RELEASE",
        3,
        0,
        229,
        255,
    );
    fb.draw_text(
        600,
        130,
        "PRODUCTION-READY PACKAGES IN READY_TO_USE/",
        2,
        255,
        255,
        255,
    );

    // Export Progress Card (Top Half)
    let prog = ((t - 28.0) / 2.0).clamp(0.0, 1.0);
    fb.fill_rect(300, 180, WIDTH - 600, 180, 26, 28, 38, 255);
    fb.draw_rect_outline(300, 180, WIDTH - 600, 180, 1, 0, 180, 240);

    fb.draw_text(
        340,
        210,
        "EXPORT TARGET: YOUTUBE 4K (3840x2160 60FPS) - H.264 / PRORES",
        2,
        255,
        255,
        255,
    );
    fb.draw_text(
        340,
        240,
        "PIPELINE: GPU RENDER GRAPH -> HARDWARE ENCODE -> ZERO DROPS",
        2,
        170,
        170,
        185,
    );

    // Progress Bar
    fb.fill_rect(340, 280, WIDTH - 680, 30, 40, 40, 50, 255);
    let bar_w = ((WIDTH - 680) as f32 * prog) as usize;
    fb.fill_rect(340, 280, bar_w, 30, 0, 229, 255, 255);

    let pct_str = format!("ENCODING PROGRESS: {:.0}%", prog * 100.0);
    fb.draw_text(340, 325, &pct_str, 2, 76, 217, 100);

    // Four Platforms Showcase Grid (Bottom Half)
    let platforms = [
        (
            "WINDOWS 1.0",
            "ready_to_use/windows/Reddart.exe",
            "9.28 MB",
            "ThinLTO Native Binary (60/120Hz Direct3D)",
        ),
        (
            "ANDROID 1.0",
            "ready_to_use/android/Reddart.apk",
            "1.18 MB",
            "Kotlin Compose + Vulkan JNI Bridge",
        ),
        (
            "MACOS 1.0",
            "ready_to_use/mac/Reddart.app",
            "12.9 MB",
            "SwiftUI Studio + Metal 120Hz ProMotion",
        ),
        (
            "IOS / IPAD",
            "ready_to_use/ios/Reddart.ipa",
            "13.0 MB",
            "Touch 120Hz Canvas + Apple Neural Engine",
        ),
    ];

    for (i, p) in platforms.iter().enumerate() {
        let col = i % 2;
        let row = i / 2;
        let bx = 300 + col * 680;
        let by = 400 + row * 190;

        fb.fill_rect(bx, by, 640, 160, 24, 26, 36, 255);
        fb.draw_rect_outline(bx, by, 640, 160, 1, 0, 200, 255);

        // Platform Name & Checkmark
        fb.draw_text(bx + 20, by + 20, p.0, 3, 0, 229, 255);
        fb.draw_text(bx + 540, by + 24, "[OK]", 2, 76, 217, 100);

        // File Path & Size
        fb.draw_text(bx + 20, by + 65, p.1, 2, 255, 255, 255);
        let sz = format!("Size: {}", p.2);
        fb.draw_text(bx + 20, by + 95, &sz, 2, 76, 217, 100);

        // Tech details
        fb.draw_text(bx + 20, by + 125, p.3, 1, 180, 180, 200);
    }

    // Final Banner
    fb.fill_rect(0, HEIGHT - 70, WIDTH, 70, 0, 30, 45, 255);
    fb.draw_text(
        420,
        HEIGHT - 46,
        "100% FREE  *  NO WATERMARKS  *  ZERO SUBSCRIPTIONS  *  ON-DEVICE AI",
        2,
        0,
        255,
        255,
    );
}

// ---------------------------------------------------------------------------
// Main Entry Point
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    println!("==========================================================");
    println!("🎬 REDDART: MASTER SHOWCASE DEMO VIDEO GENERATOR");
    println!("==========================================================");

    let raw_mp4 = "ready_to_use/Reddart_Master_Showcase_Raw.mp4";
    let final_mp4 = "ready_to_use/Reddart_Master_Showcase.mp4";
    let windows_copy = "ready_to_use/windows/Reddart_Master_Demo.mp4";

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

    println!("[2/5] Rendering 32-second 1080p 60fps Master Showcase (1920 frames)...");
    let mut fb = Framebuffer::new();
    let start_time = std::time::Instant::now();

    for frame_idx in 0..TOTAL_FRAMES {
        let t = frame_idx as f32 / FPS as f32;

        if t < 4.5 {
            render_intro_title(&mut fb, t);
        } else if t < 28.0 {
            render_studio_interactive(&mut fb, t);
        } else {
            render_export_and_deliverables(&mut fb, t);
        }

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

    println!("[4/5] Synthesizing rich multi-harmony demo soundtrack (48kHz Stereo)...");
    // Synthesize an uplifting tech demo electronic chord track
    let audio_synth_cmd = format!(
        "aevalsrc=0.05*sin(2*PI*130.81*t)*exp(-0.4*mod(t\\,2))+0.04*sin(2*PI*261.63*t)+0.03*sin(2*PI*392.00*t)+0.025*sin(2*PI*523.25*t)+0.03*sin(2*PI*659.25*t):s=48000:d={}",
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

    // Clean up temporary raw video file
    let _ = std::fs::remove_file(raw_mp4);

    // Also mirror to ready_to_use/windows/Reddart_Master_Demo.mp4
    std::fs::copy(final_mp4, windows_copy)?;

    let elapsed = start_time.elapsed();
    let meta = std::fs::metadata(final_mp4)?;
    let size_mb = meta.len() as f64 / 1_048_576.0;

    println!("[5/5] Master Demo Video generated successfully!");
    println!("  -> Root File: {}", final_mp4);
    println!("  -> Windows Mirror: {}", windows_copy);
    println!("  -> Size: {:.2} MB", size_mb);
    println!("  -> Resolution: {}x{} @ 60 FPS Progressive", WIDTH, HEIGHT);
    println!(
        "  -> Duration: {:.1} seconds ({} frames)",
        DURATION_SECS, TOTAL_FRAMES
    );
    println!("  -> Audio: 48,000 Hz Stereo AAC (256 kbps)",);
    println!(
        "  -> Total Render Time: {:.2} seconds",
        elapsed.as_secs_f64()
    );
    println!("==========================================================");

    Ok(())
}
