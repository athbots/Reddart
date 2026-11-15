# Reddart v1.2.0 Production Release: Advanced Motion, Typography & Color Engine

**Release Tag:** `v1.2.0`  
**Distribution Archive:** `Reddart-v1.2.0-Windows-x86_64.zip`  
**Binary Checksum (SHA-256):** `412e4f3f947859bc8c3814897aeda305915dbaf4382597b5be635f13f4f06d5e`  
**Archive Checksum (SHA-256):** `f6416a463f9b24f3d95fc8db6a7831d307398d514350e91c2d9f0981988ebe3c`  
**MSRV:** Rust 1.85.0 (2024 Edition)  

---

## 1. Verified Production Capabilities

Reddart v1.2.0 represents a major milestone expansion, introducing professional motion estimation, vector typography, digital cinema color science, and nested timeline compositions:

### Hardware-Accelerated Optical Flow Retiming
- **WGSL Compute Pipeline:** Dispatches `effects/optical_flow/flow_estimate.wgsl` executing Horn-Schunck iterative relaxation across $8 \times 8$ thread workgroups.
- **Dense Motion Field:** Dense forward and backward vector fields packed into `Rgba16Float` textures for real-time velocity estimation.
- **Bidirectional Synthesis:** Motion-compensated intermediate frame generation with bilateral occlusion hole-filling and smooth velocity interpolation.

### Nested Timelines & Compound Sequences
- **Multi-Level Recursion:** Embed complete sub-sequences as individual clips within master timelines with automatic cycle detection and recursion depth clamping.
- **Pre-Render Sequence Cache:** High-speed LRU frame caching layer (`NestedSequenceCache`) with deterministic mutation-hash invalidation avoiding redundant DAG recompilation.

### GPU Signed Distance Field (SDF) Text Rendering
- **Vector-Sharp Typography:** Direct WGSL rendering with sub-pixel antialiasing via `smoothstep` regardless of canvas zoom or display scale.
- **Linear-Time 2D EDT:** Felzenszwalb & Huttenlocher exact Euclidean Distance Transform ($O(W \cdot H)$) with high-efficiency R8Unorm atlas caching.
- **Rich Typographic Styling:** Native tracking (kerning), leading (line spacing), stroke outline width/color, and customizable drop shadows.

### Digital Cinema HDR & ACES Color Pipeline
- **OpenColorIO v2 Parser:** Zero-dependency YAML configuration parser compliant with OCIO v2 specifications and reference ACES 1.2 / 1.3 color hierarchy.
- **ACEScct Transfer Function:** Academy S-2016-001 logarithmic grading curve with linear toe segment below black ($L \le 0.0078125$) for tactile shadow control.
- **High-Precision Quantization & Dithering:** 10-bit (`Rgb10a2Unorm`) and 12-bit (`Rgb12`) packing with symmetric Triangular Probability Density Function (TPDF) blue noise dithering.
- **Dynamic HDR Surface Negotiation:** Automatic display probing on Windows DXGI/wgpu selecting `Rgba16Float` (scRGB Linear HDR) or `Rgb10a2Unorm` (HDR10 PQ) with graceful sRGB SDR fallback.

### Cross-Platform Packaging & Distribution Matrix
- **Linux Flatpak & AppImage:** Complete FreeDesktop Flatpak 24.08 sandbox manifest (`platforms/linux/app.reddart.Reddart.yaml`) and AppImage runner with Wayland fractional scaling auto-detection.
- **macOS DMG & Apple Notarization:** Drag-and-drop UDZO disk image generation, hardened runtime codesigning (`--options runtime --timestamp`), and Apple Notarization CLI automation (`xcrun notarytool`).

---

## 2. System Requirements & Hardware Compatibility

### Windows Specifications:
- **Operating System:** Windows 10 (version 1903 or higher, 64-bit) or Windows 11.
- **Processor:** 64-bit x86_64 CPU with SSE4.2 support (Intel 8th+ Gen, AMD Ryzen).
- **Memory:** 16 GB DDR4/DDR5 RAM recommended for 4K and multi-layer nested workflows.
- **Graphics Card:** DirectX 12 Feature Level 11_0 or Vulkan 1.2 compatible GPU (NVIDIA GTX 900+, AMD Radeon RX 400+, Intel UHD 600+).
- **Audio Output:** WASAPI-compatible stereo output device.
- **External Media Engine:** FFmpeg 6.0+ recommended in system `PATH` or `<app_dir>/bin/`.

### Linux Specifications:
- **Display Server:** Wayland (native fractional scaling) or X11.
- **Graphics:** Vulkan 1.2+ DRI GPU driver (Mesa 23+, NVIDIA 525+).
- **Audio:** PipeWire or PulseAudio low-latency sound server.

### macOS Specifications:
- **Operating System:** macOS 12 (Monterey) or higher (macOS 14+ Sonoma recommended).
- **Architecture:** Universal 2 (Apple Silicon M1/M2/M3/M4 & Intel x86_64).
- **Graphics:** Metal 2.4+ hardware acceleration.

---

## 3. Reporting Issues

To report bugs, regressions, or feature requests:
- **GitHub Issues:** [github.com/athbots/Reddart/issues](https://github.com/athbots/Reddart/issues)
- Include your platform, GPU model, and stdout from `reddart --version`.
