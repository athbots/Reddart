# Reddart v1.0.0 Production Release: Integrated Editing Workflow & Media Engine

**Release Tag:** `v1.0.0`  
**Distribution Archive:** `Reddart-v1.0.0-Windows-x86_64.zip`  
**Binary Checksum (SHA-256):** `ab848fbaa8113a18cefd86ce07570bf9b500a58180077988a23b885b79972cab`  
**Archive Checksum (SHA-256):** `561209508533f0d8deb418a88d3348ac7a64aca23c92598be3f5c92994073e87`
**MSRV:** Rust 1.85.0 (2024 Edition)  

---

## 1. Verified Production Capabilities

Reddart v1.0.0 is officially released for production use by creators on Windows 10 and Windows 11:

### Core Editing & Timeline Deck
- **NLE Editing Tools:** Full implementation of Selection (`V`), Razor Split (`C`), Ripple Trim Out (`B`), Roll Edit (`N`), Slip (`Y`), Slide (`U`), and Cue Marker (`M`) tools.
- **Keyframe Automation Engine:** Five interpolation curves (**Linear**, **EaseIn**, **EaseOut**, **EaseInOut**, and stepped **Hold**) driving spatial translation, scaling, rotation, opacity, speed, audio volume, and mask parameters.
- **Geometric Shape Masking:** Real-time GPU rasterization of Rectangle, Ellipse, Linear Gradient, and Vignette shapes with normalized feather radius and alpha inversion.
- **Dedicated 48 kHz Audio Engine:** Native 48,000 Hz 32-bit floating-point WASAPI stereo mixing bus with dynamic linear resampling, 3-band parametric EQ, and lookahead $\tanh$ soft-knee limiter.
- **Variable Playback Speed:** Continuous $0.25\times\text{--}4.0\times$ speed remapping with microsecond timeline-to-source synchronization.
- **Starter Project Templates:** Five pre-calibrated multi-format project blueprints (Reels/TikTok 9:16 Beat Cut, Cinematic Vlog 16:9, Social Square 1:1, YouTube 4K Doc, Action Video).
- **Atomic WAL Journaling:** Append-only transaction logging with CRC32 verification and automatic torn-write truncation.
- **Hardware Export Pipeline:** Automatic discovery of NVENC, AMF, QuickSync, and VideoToolbox with libx264/libx265 fallback.
- **Headless Terminal CLI:** Direct command-line video rendering, container probing (`reddart probe`), template listing (`reddart templates`), and DAG latency benchmarking (`reddart benchmark`).

---

## 2. System Requirements & Hardware Compatibility

### Minimum Specifications:
- **Operating System:** Windows 10 (version 1903 or higher, 64-bit) or Windows 11.
- **Processor:** Dual-core 64-bit x86_64 CPU with SSE4.2 support.
- **Memory:** 8 GB DDR4 RAM.
- **Graphics Card:** DirectX 12 Feature Level 11_0 or Vulkan 1.2 compatible GPU (NVIDIA GTX 900 series+, AMD Radeon RX 400 series+, Intel HD Graphics 500+).
- **Audio Output:** WASAPI-compatible stereo output device (built-in speakers, headphone jack, or USB DAC).

### Recommended Studio Specifications:
- **Processor:** 8-core CPU (Intel Core i7/i9 12th+ Gen, AMD Ryzen 7/9 5000+ series).
- **Memory:** 32 GB DDR4/DDR5 RAM.
- **Graphics Card:** NVIDIA RTX 3060+ (for 4K NVENC export) or AMD Radeon RX 6700XT+.
- **Storage:** Dedicated NVMe SSD for high-bandwidth media caching.

---

## 3. Known Limitations & Phase II Workarounds

As a public beta release, creators should be aware of the following known architectural limitations:

1. **Nested Compound Clips / Sub-Sequences:**  
   *Current State:* The v1.0.0 timeline supports flat multi-track arrangements of video and audio tracks. Timeline nesting (placing a timeline inside another timeline) is scheduled for Phase II.  
   *Workaround:* Render sub-sequences headlessly to an intermediate MP4 (`reddart export sub.reddart -o sub.mp4`) and import into the main project.
2. **Text / Title Generator Overlay:**  
   *Current State:* Subtitles and caption tracks render via `rd-ml` whisper integration; arbitrary rich-text graphical title animators are currently being migrated to WGSL signed distance field text rendering.  
   *Workaround:* Import transparent PNG title cards directly into overlay video tracks.
3. **Multi-Channel Surround Audio (5.1 / 7.1):**  
   *Current State:* The WASAPI audio engine operates in studio stereo (2.0 channels). Multichannel surround sound and 3D ambisonics bed mixing are slated for Phase II.
4. **macOS and Linux Binary Releases:**  
   *Current State:* Core crates (`rd-core`, `rd-render`, `rd-effects`, `rd-decode`, `rd-export`) support macOS (Metal) and Linux (Vulkan); pre-compiled GUI binaries are currently packaged exclusively for Windows x86_64.

---

## 4. Reporting Issues

To report bugs or regressions without compromising your media privacy:
- **GitHub Issues:** [github.com/athbots/Reddart/issues](https://github.com/athbots/Reddart/issues)
- Include your GPU model, Windows version, and stdout from `reddart --version`.
