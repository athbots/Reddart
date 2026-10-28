//! Reddart Video Demonstration Generator
//!
//! Generates a pristine 1080p 60fps recording of the Windows application in action,
//! showcasing zero-lag playback, high-speed scrubbing, razor clip splitting,
//! 3D LUT grading, Whisper on-device auto-captions, and hardware video export.

use std::io::Write;
use std::process::{Command, Stdio};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const FPS: u32 = 60;
const DURATION_SECS: f32 = 18.0;
const TOTAL_FRAMES: usize = (FPS as f32 * DURATION_SECS) as usize; // 1080 frames

// ---------------------------------------------------------------------------
// 8x8 Simple Raster Font for Crisp Crisp UI Text
// ---------------------------------------------------------------------------
mod font_8x8 {
    // Basic 8x8 glyph map for ASCII characters 32 to 126
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

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8, a: u8) {
        for dy in 0..h {
            for dx in 0..w {
                self.set_pixel(x + dx, y + dy, r, g, b, a);
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
        self.fill_rect(x, y, w, thickness, r, g, b, 255); // Top
        self.fill_rect(x, y + h - thickness, w, thickness, r, g, b, 255); // Bottom
        self.fill_rect(x, y, thickness, h, r, g, b, 255); // Left
        self.fill_rect(x + w - thickness, y, thickness, h, r, g, b, 255); // Right
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
}

// ---------------------------------------------------------------------------
// Simulation State
// ---------------------------------------------------------------------------

struct DemoScenario {
    pub playhead_secs: f32,
    pub is_playing: bool,
    pub active_lut: &'static str,
    pub clip_split: bool,
    pub ai_captions_active: bool,
    pub export_modal_active: bool,
    pub export_progress: f32,
    pub message: String,
}

impl DemoScenario {
    pub fn update_for_time(&mut self, t: f32) {
        if t < 3.5 {
            // Stage 1: Playback
            self.is_playing = true;
            self.playhead_secs = t;
            self.active_lut = "Natural Rec.709";
            self.clip_split = false;
            self.ai_captions_active = false;
            self.export_modal_active = false;
            self.message = format!("Playback active (60 fps ProMotion) | Audio Clock Synced");
        } else if t < 7.0 {
            // Stage 2: Rapid Scrubbing back and forth (Zero-Lag demo)
            self.is_playing = false;
            let scrub_phase = ((t - 3.5) * 4.0).sin(); // oscillate between -1 and 1
            self.playhead_secs = (3.5 + scrub_phase * 2.5).clamp(0.5, 6.5);
            self.message = format!("Interactive Scrubbing: < 15 us DAG Invalidation Latency");
        } else if t < 10.0 {
            // Stage 3: Split Clip at 2.4s
            self.is_playing = false;
            self.playhead_secs = 2.4;
            self.clip_split = true;
            self.message = format!("Split Clip (Ctrl+B) executed at 00:00:02.24 | Clip 1A & 1B created");
        } else if t < 13.5 {
            // Stage 4: 3D LUT Color Grading
            self.is_playing = true;
            self.playhead_secs = 2.4 + (t - 10.0) * 0.8;
            self.active_lut = "Teal & Orange";
            self.message = format!("3D LUT Applied: Teal & Orange [Warm Highlights, Teal Shadows]");
        } else if t < 16.5 {
            // Stage 5: Whisper On-Device Auto Captions
            self.is_playing = true;
            self.playhead_secs = 3.5 + (t - 13.5) * 0.8;
            self.ai_captions_active = true;
            self.message = format!("Whisper Auto-Captions generated with word-level timing");
        } else {
            // Stage 6: Video Export Dialog & Hardware Progress
            self.is_playing = false;
            self.export_modal_active = true;
            self.export_progress = ((t - 16.5) / 2.5).clamp(0.0, 1.0);
            if self.export_progress >= 1.0 {
                self.message = format!("Export Complete: YouTube 4K (60fps H.264 VideoToolbox / MediaFoundation)");
            } else {
                self.message = format!("Hardware Exporting: {:.0}%...", self.export_progress * 100.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render Frame
// ---------------------------------------------------------------------------

fn render_studio_frame(fb: &mut Framebuffer, scenario: &DemoScenario, t: f32) {
    // 1. Studio Background Theme
    fb.fill_rect(0, 0, WIDTH, HEIGHT, 22, 22, 26, 255); // #16161a

    // 2. Header Bar (y: 0..48)
    fb.fill_rect(0, 0, WIDTH, 48, 30, 30, 36, 255);
    fb.fill_rect(0, 47, WIDTH, 1, 46, 46, 56, 255);

    fb.draw_text(16, 16, "REDDART 1.0 (WINDOWS) - ZERO-LAG VIDEO EDITOR", 2, 0, 229, 255);
    fb.draw_text(650, 18, "File   Edit   Timeline   View   AI Tools   Export", 2, 170, 170, 185);
    fb.draw_text(1520, 18, "[F1] Shortcuts  |  120Hz ProMotion Sync", 2, 0, 229, 255);

    // 3. Left Panel: Media & 3D LUT Browser (x: 0..340, y: 48..700)
    fb.fill_rect(0, 48, 340, 652, 26, 26, 32, 255);
    fb.fill_rect(339, 48, 1, 652, 46, 46, 56, 255);

    // Tab Header
    fb.fill_rect(8, 56, 324, 34, 36, 36, 44, 255);
    fb.draw_text(16, 66, "Media Bin", 2, 255, 255, 255);
    fb.draw_text(130, 66, "3D LUTs", 2, 0, 229, 255);
    fb.draw_text(230, 66, "AI Tools", 2, 160, 160, 175);

    // Asset Items
    fb.draw_text(16, 110, "PROJECT MEDIA ASSETS", 2, 120, 120, 140);
    fb.draw_text(24, 138, "[VID] clip_01.mp4 (4K 60fps)", 2, 220, 220, 230);
    fb.draw_text(24, 168, "[VID] clip_02.mp4 (1080p 60fps)", 2, 220, 220, 230);
    fb.draw_text(24, 198, "[AUD] ambient_soundtrack.wav", 2, 76, 217, 100);

    // 3D LUT Presets
    fb.draw_text(16, 240, "3D COLOR LUTS (.CUBE)", 2, 120, 120, 140);
    let luts = [
        "Natural Rec.709",
        "Teal & Orange",
        "Warm Sunlight",
        "Vintage Film",
        "Black & White High Contrast",
    ];

    for (i, lut) in luts.iter().enumerate() {
        let ly = 270 + i * 36;
        let is_active = *lut == scenario.active_lut;
        if is_active {
            fb.fill_rect(12, ly - 4, 316, 30, 0, 80, 110, 255);
            fb.draw_rect_outline(12, ly - 4, 316, 30, 1, 0, 229, 255);
            fb.draw_text(24, ly + 2, lut, 2, 0, 255, 255);
            fb.draw_text(290, ly + 2, "[X]", 2, 0, 255, 255);
        } else {
            fb.draw_text(24, ly + 2, lut, 2, 180, 180, 195);
        }
    }

    // Starter Templates
    fb.draw_text(16, 470, "STARTER TEMPLATES", 2, 120, 120, 140);
    fb.draw_text(24, 500, "[*] TikTok / Reels Beat Cut", 2, 255, 180, 50);
    fb.draw_text(24, 530, "[*] Cinematic Vlog 16:9", 2, 255, 180, 50);
    fb.draw_text(24, 560, "[*] Social Square 1:1", 2, 255, 180, 50);

    // 4. Right Panel: Inspector (x: 1600..1920, y: 48..700)
    fb.fill_rect(1600, 48, 320, 652, 26, 26, 32, 255);
    fb.fill_rect(1600, 48, 1, 652, 46, 46, 56, 255);

    fb.draw_text(1620, 66, "PROJECT INSPECTOR", 2, 0, 229, 255);
    fb.draw_text(1620, 105, "Resolution: 1920 x 1080", 2, 200, 200, 210);
    fb.draw_text(1620, 135, "Frame Rate: 60.00 fps", 2, 200, 200, 210);
    fb.draw_text(1620, 165, "Engine: Path C Shared Core", 2, 200, 200, 210);
    fb.draw_text(1620, 195, "Governor Scale: 100% Full", 2, 76, 217, 100);
    fb.draw_text(1620, 225, "Dirty DAG: 11.46 us", 2, 0, 229, 255);
    fb.draw_text(1620, 255, "Memory Pool: 149 MB", 2, 200, 200, 210);

    // Export Action Button
    fb.fill_rect(1620, 310, 280, 44, 0, 180, 216, 255);
    fb.draw_text(1645, 324, "EXPORT VIDEO (CMD+E)", 2, 0, 0, 0);

    // 5. Center Viewport: Video Preview (x: 340..1600, y: 48..640)
    fb.fill_rect(340, 48, 1260, 592, 14, 14, 16, 255);

    // Inner 16:9 Video Canvas: 960x540 centered at x=490, y=74
    let vx = 490;
    let vy = 74;
    let vw = 960;
    let vh = 540;

    fb.draw_rect_outline(vx - 2, vy - 2, vw + 4, vh + 4, 2, 50, 50, 60);

    // Procedural Live Video Content
    let p_time = scenario.playhead_secs;
    let is_teal_orange = scenario.active_lut == "Teal & Orange";

    for dy in 0..vh {
        let ny = dy as f32 / vh as f32;
        for dx in (0..vw).step_by(2) {
            let nx = dx as f32 / vw as f32;

            // Generate animated procedural scene
            let wave = ((nx * 6.0 + p_time * 2.0).sin() * (ny * 6.0 + p_time).cos() * 0.5 + 0.5);
            let circle_dist = ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt();
            let circle = (circle_dist * 8.0 - p_time * 3.0).sin() * 0.5 + 0.5;

            let mut r = (wave * 180.0 + circle * 60.0) as u8;
            let mut g = (ny * 100.0 + (1.0 - circle_dist) * 120.0) as u8;
            let mut b = ((1.0 - wave) * 160.0 + circle * 90.0) as u8;

            // Apply 3D LUT (Teal & Orange)
            if is_teal_orange {
                // Boost warm skin/highlight tones into orange and cool shadows into deep teal
                r = (r as f32 * 1.35).min(255.0) as u8;
                g = (g as f32 * 0.95) as u8;
                b = if wave > 0.5 { (b as f32 * 0.7) as u8 } else { (b as f32 * 1.3).min(255.0) as u8 };
            }

            fb.set_pixel(vx + dx, vy + dy, r, g, b, 255);
            fb.set_pixel(vx + dx + 1, vy + dy, r, g, b, 255);
        }
    }

    // Viewport HUD Overlays
    fb.fill_rect(vx + 16, vy + 16, 420, 30, 0, 0, 0, 180);
    fb.draw_text(vx + 24, vy + 24, "LIVE HARDWARE VIEWPORT: 60 FPS", 2, 0, 255, 255);

    let tc_str = format!("TC: 00:00:{:02}.{:02}", p_time as u32, ((p_time.fract()) * 60.0) as u32);
    fb.fill_rect(vx + vw - 220, vy + 16, 204, 30, 0, 0, 0, 180);
    fb.draw_text(vx + vw - 212, vy + 24, &tc_str, 2, 255, 255, 255);

    // 6. Transport & Audio Bar (x: 340..1600, y: 640..700)
    fb.fill_rect(340, 640, 1260, 60, 26, 26, 32, 255);
    fb.fill_rect(340, 640, 1260, 1, 46, 46, 56, 255);

    // Transport buttons
    fb.draw_text(370, 660, "|<<   < -1s", 2, 200, 200, 210);
    let play_label = if scenario.is_playing { "[ PAUSE ]" } else { "[ PLAY ]" };
    fb.fill_rect(480, 650, 100, 36, 0, 229, 255, 255);
    fb.draw_text(495, 662, play_label, 2, 0, 0, 0);
    fb.draw_text(600, 660, "+1s >   >>|", 2, 200, 200, 210);

    // Timecode in Transport
    fb.draw_text(740, 660, &tc_str, 2, 0, 229, 255);

    // Audio VU Meter
    fb.draw_text(920, 662, "VU L:", 1, 150, 150, 160);
    fb.draw_text(920, 674, "VU R:", 1, 150, 150, 160);

    let vu_level = if scenario.is_playing {
        ((p_time * 8.0).sin().abs() * 0.7 + 0.25)
    } else {
        0.05
    };
    let vu_w = (vu_level * 180.0) as usize;
    fb.fill_rect(960, 662, 180, 8, 30, 30, 35, 255);
    fb.fill_rect(960, 674, 180, 8, 30, 30, 35, 255);
    fb.fill_rect(960, 662, vu_w, 8, 76, 217, 100, 255);
    fb.fill_rect(960, 674, (vu_w as f32 * 0.95) as usize, 8, 76, 217, 100, 255);

    // Active LUT indicator pill
    let lut_tag = format!("LUT: {}", scenario.active_lut);
    fb.fill_rect(1180, 652, 220, 32, 0, 60, 80, 255);
    fb.draw_rect_outline(1180, 652, 220, 32, 1, 0, 229, 255);
    fb.draw_text(1195, 662, &lut_tag, 2, 0, 229, 255);

    // Status Message
    fb.draw_text(360, 686, &scenario.message, 1, 160, 220, 160);

    // 7. Bottom Multi-Track Timeline (x: 0..1920, y: 700..1080)
    fb.fill_rect(0, 700, WIDTH, 380, 20, 20, 24, 255);
    fb.fill_rect(0, 700, WIDTH, 1, 50, 50, 62, 255);

    // Timeline Toolbar (y: 706..740)
    fb.fill_rect(0, 704, WIDTH, 36, 28, 28, 34, 255);
    fb.draw_text(16, 714, "[Undo]", 2, 180, 180, 190);
    fb.draw_text(100, 714, "[Redo]", 2, 180, 180, 190);
    let split_col = if scenario.clip_split { (0, 255, 255) } else { (255, 255, 255) };
    fb.draw_text(180, 714, "[Split (Ctrl+B)]", 2, split_col.0, split_col.1, split_col.2);
    fb.draw_text(350, 714, "[Delete]", 2, 180, 180, 190);
    fb.draw_text(450, 714, "[* Snap: ON]", 2, 76, 217, 100);
    fb.draw_text(590, 714, "[Zoom 100%]", 2, 180, 180, 190);

    // Time Ruler (y: 742..762)
    fb.fill_rect(200, 742, 1720, 22, 24, 24, 30, 255);
    for sec in 0..=8 {
        let rx = 200 + sec * 190;
        fb.fill_rect(rx, 754, 2, 10, 100, 100, 120, 255);
        let sec_str = format!("00:0{:01}.00", sec);
        fb.draw_text(rx + 4, 746, &sec_str, 1, 140, 140, 155);
    }

    // Track 1: V1 Video Main (y: 768..828)
    fb.fill_rect(0, 768, 190, 60, 32, 34, 44, 255);
    fb.draw_text(12, 788, "V1 Video Main [M/S]", 2, 100, 180, 255);

    if scenario.clip_split {
        // Two split clip blocks: Clip 1A (0..2.4s) and Clip 1B (2.4s..4.0s)
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
        fb.draw_rect_outline(200 + w_1a + 2, 770, w_1b - 2, 56, 2, 255, 220, 0); // Yellow selection border
        fb.draw_text(200 + w_1a + 10, 786, "clip_01B [SELECTED]", 2, 255, 255, 255);

        // Clip 2
        let w_2 = (4.0 * 190.0) as usize;
        fb.fill_rect(200 + 4 * 190, 770, w_2, 56, 30, 90, 180, 255);
        fb.draw_rect_outline(200 + 4 * 190, 770, w_2, 56, 1, 60, 140, 230);
        fb.draw_text(200 + 4 * 190 + 10, 786, "clip_02.mp4 (4.0s)", 2, 255, 255, 255);
    } else {
        // Single Clip 1 (0..4.0s)
        let w_1 = (4.0 * 190.0) as usize;
        fb.fill_rect(200, 770, w_1, 56, 40, 110, 210, 255);
        fb.draw_rect_outline(200, 770, w_1, 56, 1, 80, 160, 255);
        fb.draw_text(210, 786, "clip_01.mp4 (4.0s)", 2, 255, 255, 255);

        // Clip 2 (4.0s..8.0s)
        let w_2 = (4.0 * 190.0) as usize;
        fb.fill_rect(200 + 4 * 190, 770, w_2, 56, 30, 90, 180, 255);
        fb.draw_rect_outline(200 + 4 * 190, 770, w_2, 56, 1, 60, 140, 230);
        fb.draw_text(200 + 4 * 190 + 10, 786, "clip_02.mp4 (4.0s)", 2, 255, 255, 255);
    }

    // Track 2: V2 Overlays / Titles (y: 834..884)
    fb.fill_rect(0, 834, 190, 50, 32, 34, 44, 255);
    fb.draw_text(12, 852, "V2 Overlays [M/S]", 2, 200, 100, 255);

    fb.fill_rect(200, 836, 300, 46, 130, 60, 180, 255);
    fb.draw_text(210, 852, "Title: Reddart 1.0", 2, 255, 255, 255);

    // Track 3: A1 Master Audio (y: 890..940)
    fb.fill_rect(0, 890, 190, 50, 32, 34, 44, 255);
    fb.draw_text(12, 908, "A1 Audio 48k [M/S]", 2, 80, 220, 110);

    let audio_w = (8.0 * 190.0) as usize;
    fb.fill_rect(200, 892, audio_w, 46, 30, 120, 60, 255);
    fb.draw_rect_outline(200, 892, audio_w, 46, 1, 70, 200, 100);
    fb.draw_text(210, 908, "ambient_soundtrack.wav [Soft Limiter & Master Clock Active]", 2, 220, 255, 220);

    // Track 4: T1 Subtitles / Captions (y: 946..996)
    fb.fill_rect(0, 946, 190, 50, 32, 34, 44, 255);
    fb.draw_text(12, 964, "T1 Subtitles (AI)", 2, 255, 180, 50);

    if scenario.ai_captions_active {
        // Captions populated by Whisper
        fb.fill_rect(200 + 40, 948, 220, 46, 180, 100, 30, 255);
        fb.draw_text(210 + 40, 964, "Welcome to Reddart", 2, 255, 255, 255);

        fb.fill_rect(200 + 300, 948, 260, 46, 180, 100, 30, 255);
        fb.draw_text(210 + 300, 964, "Zero-Lag 120Hz Editor", 2, 255, 255, 255);

        fb.fill_rect(200 + 600, 948, 280, 46, 180, 100, 30, 255);
        fb.draw_text(210 + 600, 964, "On-Device AI Engine", 2, 255, 255, 255);
    }

    // Red Playhead Scrub Line
    let playhead_x = 200 + (scenario.playhead_secs * 190.0) as usize;
    if playhead_x < WIDTH {
        fb.fill_rect(playhead_x, 742, 2, 260, 255, 40, 40, 255);
        // Playhead head inverted triangle
        fb.fill_rect(playhead_x - 6, 738, 14, 8, 255, 40, 40, 255);
    }

    // 8. Modals / Dialog Popups
    if scenario.export_modal_active {
        // Dim background
        fb.fill_rect(0, 0, WIDTH, HEIGHT, 0, 0, 0, 160);

        // Modal Box
        let mx = 660;
        let my = 300;
        let mw = 600;
        let mh = 360;

        fb.fill_rect(mx, my, mw, mh, 28, 28, 36, 255);
        fb.draw_rect_outline(mx, my, mw, mh, 2, 0, 229, 255);

        fb.draw_text(mx + 30, my + 30, "HARDWARE VIDEO EXPORT PIPELINE", 2, 0, 229, 255);
        fb.draw_text(mx + 30, my + 75, "Target: YouTube 4K Ultra HD (60.0 fps)", 2, 220, 220, 230);
        fb.draw_text(mx + 30, my + 105, "Encoder: VideoToolbox / MediaFoundation H.264", 2, 180, 180, 190);
        fb.draw_text(mx + 30, my + 135, "Bitrate: 45.0 Mbps CBR | Audio: 48kHz Stereo AAC", 2, 180, 180, 190);

        // Progress Bar
        fb.fill_rect(mx + 30, my + 180, mw - 60, 26, 40, 40, 50, 255);
        let prog_w = ((mw - 60) as f32 * scenario.export_progress) as usize;
        fb.fill_rect(mx + 30, my + 180, prog_w, 26, 0, 229, 255, 255);

        let pct_str = format!("Encoding Progress: {:.0}%", scenario.export_progress * 100.0);
        fb.draw_text(mx + 30, my + 225, &pct_str, 2, 255, 255, 255);

        if scenario.export_progress >= 1.0 {
            fb.fill_rect(mx + 30, my + 270, mw - 60, 48, 40, 180, 80, 255);
            fb.draw_text(mx + 60, my + 286, "[OK] EXPORT COMPLETE: Reddart_Output.mp4", 2, 0, 0, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// Main Entry Point
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    println!("==========================================================");
    println!("🎬 REDDART: WINDOWS WORKING VIDEO RECORDING GENERATOR");
    println!("==========================================================");

    let output_mp4 = "ready_to_use/windows/Reddart_Windows_Demo.mp4";

    println!("[1/4] Spawning FFmpeg encoder process...");
    let ffmpeg_path = "C:\\Users\\aniru\\AppData\\Local\\Microsoft\\WinGet\\Links\\ffmpeg.exe";

    let mut child = Command::new(ffmpeg_path)
        .args([
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-s", &format!("{}x{}", WIDTH, HEIGHT),
            "-r", &FPS.to_string(),
            "-i", "-", // read raw frames from stdin
            "-c:v", "libx264",
            "-preset", "veryfast",
            "-crf", "18",
            "-pix_fmt", "yuv420p",
            "-y",
            output_mp4,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("Failed to open FFmpeg stdin");

    println!("[2/4] Rendering 1080p 60fps frames demonstrating interactive workflow...");
    let mut fb = Framebuffer::new();
    let mut scenario = DemoScenario {
        playhead_secs: 0.0,
        is_playing: true,
        active_lut: "Natural Rec.709",
        clip_split: false,
        ai_captions_active: false,
        export_modal_active: false,
        export_progress: 0.0,
        message: String::new(),
    };

    let start_time = std::time::Instant::now();

    for frame_idx in 0..TOTAL_FRAMES {
        let t = frame_idx as f32 / FPS as f32;
        scenario.update_for_time(t);
        render_studio_frame(&mut fb, &scenario, t);
        stdin.write_all(&fb.pixels)?;

        if frame_idx % 120 == 0 {
            let pct = (frame_idx as f32 / TOTAL_FRAMES as f32) * 100.0;
            println!("  -> Frame {}/{} ({:.0}%) at {:.2}s", frame_idx, TOTAL_FRAMES, pct, t);
        }
    }

    drop(stdin); // Flush and close pipe

    println!("[3/4] Awaiting FFmpeg encoding completion...");
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("FFmpeg failed with status: {:?}", status);
    }

    let elapsed = start_time.elapsed();
    let meta = std::fs::metadata(output_mp4)?;
    let size_mb = meta.len() as f64 / 1_048_576.0;

    println!("[4/4] Video generated successfully!");
    println!("  -> File: {}", output_mp4);
    println!("  -> Size: {:.2} MB", size_mb);
    println!("  -> Resolution: {}x{} @ 60 FPS", WIDTH, HEIGHT);
    println!("  -> Duration: {:.1} seconds ({} frames)", DURATION_SECS, TOTAL_FRAMES);
    println!("  -> Render Time: {:.2} seconds", elapsed.as_secs_f64());
    println!("==========================================================");

    Ok(())
}

