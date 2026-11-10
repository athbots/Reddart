========================================================================
🎬 REDDART — ZERO-LAG NATIVE VIDEO EDITOR (WINDOWS v0.2.1)
========================================================================

Reddart is a high-performance, subscription-free, on-device video editor
built with a shared native Rust core, Direct3D 12/Vulkan hardware-accelerated
WGSL shaders, and 48 kHz low-latency WASAPI audio mixing.

🚀 HOW TO RUN:
1. Double-click `Reddart.exe` to launch the Studio interface immediately.
2. Or run via terminal: `.\Reddart.exe --help` to access headless export,
   media probing, template listing, and render graph benchmarks.
No installation or runtime dependencies required.

✨ FEATURES INCLUDED:
- Full NLE editing deck (Selection, Razor split, Ripple trim, Roll, Slip, Slide)
- Keyframe automation engine (Linear, Ease In, Ease Out, Ease In-Out, Hold)
- Geometric Shape Masks (Rectangle, Ellipse, Gradient, Vignette, Feather, Invert)
- Dedicated 48 kHz WASAPI stereo audio with clip volume/pan, 3-band EQ, limiter
- Continuous variable speed remapping (0.25x to 4.0x)
- Curated 3D LUT Color Grading presets and external .cube 3D LUT import
- In-engine Title & Subtitle Generator (bitmap glyphs, drop shadow, pill bounding box)
- Curated Starter Templates (Reels/TikTok Beat Cut, Cinematic Vlog, Social Square, 4K Doc)
- Append-only Write-Ahead Log (.reddart.wal) with CRC32 crash recovery
- Multi-format hardware video export (NVENC, AMF, QuickSync, VideoToolbox, CPU)

🌐 OFFICIAL PRODUCT WEBSITE:
https://athbots.github.io/Reddart/

⌨️ KEYBOARD SHORTCUTS:
- Space               : Play / Pause
- J / K / L           : Shuttle backwards / Stop / Shuttle forward (1x, 2x, 4x, 8x)
- C                   : Razor split clip at playhead
- B                   : Ripple trim tool (trim in/out with ripple)
- N                   : Roll edit tool (adjust cut preserving time)
- Y / U               : Slip / Slide edit tools
- V                   : Selection tool
- I / O               : Set Mark-In / Mark-Out work area bounds
- M                   : Add cue marker at playhead
- Ctrl + T            : Open Starter Project Templates gallery
- Ctrl + S            : Save project and flush WAL checkpoint
- Ctrl + E            : Open Hardware Video Export dialog
- Ctrl + Z            : Undo
- Ctrl + Y            : Redo
- Left / Right Arrow  : Step 1 sec back / forward
- Shift + Left/Right  : Step 1 frame back / forward
========================================================================

