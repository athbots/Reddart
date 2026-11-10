================================================================================
REDDART VIDEO EDITOR FOR WINDOWS — PRODUCTION RELEASE v0.2.1
================================================================================

1. OVERVIEW
Reddart is a high-performance creator video editor built on a shared Rust engine
with GPU compositing (DirectX 12 / Vulkan via wgpu), 48 kHz low-latency WASAPI
audio mixing, eased keyframe automation, and geometric shape masking.

2. SYSTEM REQUIREMENTS
- Operating System: Windows 10 (version 1903+) or Windows 11 (64-bit)
- Processor: Intel Core i5 / AMD Ryzen 5 or higher
- Graphics: DirectX 12 or Vulkan compatible GPU (NVIDIA GTX 1060+, AMD RX 580+,
  or Intel Iris Xe / Arc)
- RAM: 8 GB minimum (16 GB recommended for 4K video)
- Storage: 200 MB free space for application + media workspace cache

3. QUICK START
1. Double-click `Reddart.exe` to launch the application.
2. Click "File > Open Project..." and select `samples/Demo_Project.reddart`.
3. Press `Space` to start/pause playback.
4. Scrub across the timeline by clicking or dragging on the time ruler.
5. Use `C` to split clips at the playhead.
6. Use `Del` or `Backspace` to delete selected clips.
7. Click "🚀 Export..." or press `Ctrl + E` to render your video to MP4.
8. Press `Ctrl + T` to browse and instantiate starter project templates.

4. KEYBOARD SHORTCUTS REFERENCE
- Space               Play / Pause Transport
- J / K / L           Shuttle Backward / Stop / Shuttle Forward (1x, 2x, 4x, 8x)
- C                   Razor Tool: Split Selected Clip at Playhead
- B                   Ripple Trim: Trim In/Out and Close Gaps
- N                   Roll Edit: Adjust Cut Preserving Sequence Duration
- Y / U               Slip / Slide Edit Tools
- V                   Selection Tool
- I / O               Set Mark-In / Mark-Out Work Area Bounds
- M                   Add Timeline Cue Marker
- Delete / Backspace  Delete Selected Clip
- Ctrl + T            Open Starter Project Templates Gallery
- Ctrl + S            Save Project (Atomic Checkpoint + WAL Flush)
- Ctrl + E            Open Hardware Video Export Dialog
- Ctrl + Z            Undo Last Operation
- Ctrl + Y            Redo Last Operation
- Left Arrow          Step Backward 1 Second (Shift+Left: 1 Frame)
- Right Arrow         Step Forward 1 Second (Shift+Right: 1 Frame)
- Home / End          Jump to Timeline Start / End

5. DATA SAFETY & RECOVERY
- Projects use an append-only Write-Ahead Log (.reddart.wal) with CRC32 checksums.
- Unexpected process termination is automatically recovered via transaction replay.
- Atomic temporary swaps prevent corrupted saves or 0-byte project files.

For documentation, issues, and updates:
https://github.com/athbots/Reddart
================================================================================

