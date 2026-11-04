================================================================================
REDDART VIDEO EDITOR FOR WINDOWS — PRODUCTION RELEASE v0.1.0
================================================================================

1. OVERVIEW
Reddart is a high-performance creator video editor built on a shared Rust engine
with GPU compositing (DirectX 12 / Vulkan via wgpu), adaptive quality scaling,
and lock-free audio mixing.

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
5. Use `S` to split clips at the playhead.
6. Use `Del` or `Backspace` to delete selected clips.
7. Click "🚀 Export..." to render your video to MP4 (1080p, 4K, 9:16 Vertical).

4. KEYBOARD SHORTCUTS REFERENCE
- Space               Play / Pause Transport
- S                   Split Selected Clip at Current Playhead
- Delete / Backspace  Delete Selected Clip
- Ctrl + Z            Undo Last Operation
- Ctrl + Y            Redo Last Operation
- Left Arrow          Step Backward 1 Second (Shift+Left: 1 Frame @ 30fps)
- Right Arrow         Step Forward 1 Second (Shift+Right: 1 Frame @ 30fps)
- Ctrl + S            Save Project (Atomic)
- Home / End          Jump to Timeline Start / End

5. DATA SAFETY & RECOVERY
- Projects are saved atomically using safe temporary swaps to prevent corruption.
- Background autosave snapshots are created automatically every 30 seconds.
- Offline/moved media is detected automatically with one-click relinking.

For documentation, issues, and updates:
https://github.com/reddart/reddart
================================================================================

