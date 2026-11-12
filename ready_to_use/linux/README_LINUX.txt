========================================================================
REDDART - ZERO-LAG PROFESSIONAL VIDEO EDITOR (Linux v1.0.0)
========================================================================

Package: Reddart-v1.0.0-Linux-x86_64
Architecture: x86_64 Linux (glibc 2.31+ / Vulkan 1.3 / Wayland & X11)
Engine: Rust 2024 + wgpu 24 + egui 0.31 + ALSA/PulseAudio

INSTALLATION:
1. Extract archive:
   unzip Reddart-v1.0.0-Linux-x86_64.zip
   cd Reddart-v1.0.0-Linux-x86_64

2. Run directly without installation:
   ./bin/reddart

3. Optional system-wide or user desktop install:
   mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
   cp bin/reddart ~/.local/bin/
   cp share/applications/reddart.desktop ~/.local/share/applications/
   cp share/icons/hicolor/scalable/apps/reddart.svg ~/.local/share/icons/hicolor/scalable/apps/
   update-desktop-database ~/.local/share/applications 2>/dev/null || true

SYSTEM REQUIREMENTS:
- GPU: Vulkan 1.2+ capable GPU (Intel Skylake+, AMD GCN 2+, NVIDIA Maxwell+)
- Audio: ALSA or PulseAudio / PipeWire ALSA compatibility layer
- Media Engine: FFmpeg 6.0+ in system PATH for multi-format decoding/encoding
========================================================================