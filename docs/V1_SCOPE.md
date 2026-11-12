# Reddart v1.0 — Product Scope & Feature Contract

**Target Version:** Reddart v1.0  
**Target Platform:** Windows 10 (Build 1903+) & Windows 11 (64-bit x86_64)  
**Architecture:** Shared Native Rust Core + Direct3D 12/Vulkan GPU Compositing + 48 kHz WASAPI Audio  
**Status:** Feature Freeze Enacted  

---

## 1. Supported Operating Systems & Hardware

| Component | Minimum Specification | Recommended Studio Specification |
| :--- | :--- | :--- |
| **Operating System** | Windows 10 (1903+ / 64-bit) or Windows 11 | Windows 11 (23H2 / 24H2) |
| **Processor** | Dual-core 64-bit x86_64 with SSE4.2 | 8-core Intel Core i7/i9 (12th+ Gen) or AMD Ryzen 7/9 |
| **System Memory** | 8 GB DDR4 RAM | 32 GB DDR4/DDR5 RAM |
| **Graphics Card** | DirectX 12 Feature Level 11_0 / Vulkan 1.2 | NVIDIA RTX 3060+ (NVENC) or AMD RX 6700XT+ (AMF) |
| **Audio Output** | WASAPI stereo device (internal, 3.5mm, USB DAC) | 48 kHz WASAPI audio interface / studio monitor DAC |
| **Storage** | 500 MB free space for application + media workspace | Dedicated NVMe PCIe 4.0 SSD for media cache |

---

## 2. Supported Input Media & Codecs

Reddart leverages FFprobe and FFmpeg stream demuxing and decoding via `rd-decode`:

### Video Container Formats
- **Containers:** MP4 (`.mp4`), QuickTime (`.mov`), Matroska (`.mkv`), WebM (`.webm`).
- **Video Codecs:** H.264 / AVC (8-bit, 4:2:0), H.265 / HEVC (8-bit & 10-bit), VP8, VP9, AV1, Apple ProRes (422 / 422 HQ).
- **Resolutions:** From standard definition (480p) through 4K UHD ($3840 \times 2160$) at 24, 25, 29.97, 30, 50, 59.94, and 60 fps.
- **Frame Rate Policies:** Constant Frame Rate (CFR) and Variable Frame Rate (VFR) with microsecond PTS normalization.

### Audio Container Formats
- **Audio Formats:** Linear PCM WAV (16-bit, 24-bit, 32-bit float), MP3, AAC, FLAC, Ogg Vorbis.
- **Sample Rates:** Automatic resampling from 44.1 kHz, 48 kHz, 96 kHz, 192 kHz to the native 48,000 Hz mixing clock.

---

## 3. Core Project & Persistence Capabilities

- **Central Mutation Dispatcher:** All editing operations route through `CommandDispatcher`, ensuring complete undo/redo fidelity (`Ctrl+Z`, `Ctrl+Y`).
- **Atomic WAL Journaling:** Append-only Write-Ahead Log (`.reddart-wal`) recording operations with CRC32 checksums before memory mutation.
- **Crash Recovery:** Automatic detection of interrupted sessions with zero-loss replay of committed edits and safe truncation of torn tail bytes.
- **Relocatable Projects:** Relative media path resolution supporting project folder relocation across drives or directories.
- **Explicit Missing-Media Handling:** Missing media displays high-visibility offline placeholders; non-destructive relinking dialog verifies target streams before updating references.

---

## 4. Timeline Editing Capabilities

- **Interactive Drag Gestures:** Single-transaction pointer dragging (`TimelineDragTransaction`) with ghost preview rendering and atomic commit on release.
- **NLE Editing Tools:**
  - Selection Tool (`V`)
  - Razor Split Tool (`C`)
  - Ripple Trim Out Tool (`B`)
  - Roll Edit Tool (`N`)
  - Slip Tool (`Y`)
  - Slide Tool (`U`)
  - Marker Placement (`M`)
- **Snapping Engine:** Magnetized snapping to playhead position, clip head/tail boundaries, and timeline cue markers.
- **Multi-Track Deck:** Unlimited video tracks (V1, V2, ...) and audio tracks (A1, A2, ...) with per-track lock, mute, and solo controls.

---

## 5. Composition, Effects & Color

- **Multi-Layer GPU Compositor:** True z-order compositing across all simultaneously active video tracks.
- **Spatial Transforms:** Position ($X, Y$), Scale ($X, Y$), Rotation (degrees), Anchor Point, and Clip Opacity ($0.0\text{--}1.0$).
- **Keyframe Automation Engine:** Five interpolation curves (**Linear**, **EaseIn**, **EaseOut**, **EaseInOut**, **Hold**) driving spatial transforms and audio properties.
- **Geometric Shape Masking:** Real-time GPU rasterization of Rectangle, Ellipse, Linear Gradient, and Vignette shapes with adjustable feather radius and alpha inversion.
- **Color Grading:** 3D LUT application (`.cube` format) with exposure, contrast, saturation, temperature, and tint controls.
- **Preview/Export Parity:** Preview monitor and export engine execute identical shader DAG logic to ensure zero visual discrepancy.

---

## 6. Timeline Audio Engine

- **Real-Time 48 kHz Mixing:** Dedicated 48,000 Hz, 32-bit floating-point WASAPI stereo bus.
- **Lock-Free Pre-Roll Buffer:** Background thread computes a 2-second lookahead pre-roll buffer feeding the real-time audio callback without heap allocations, file I/O, or mutex contention.
- **Track Controls:** Individual clip volume, track gain ($-\infty\text{ dB}$ to $+12\text{ dB}$), stereo pan ($-1.0$ to $+1.0$), mute, and solo.
- **Master Bus Processing:** 3-band parametric equalization and lookahead $\tanh$ soft-knee limiter preventing digital overs.

---

## 7. Hardware-Accelerated Export Pipeline

- **Automatic ASIC Discovery:** Probes and selects dedicated silicon encoders:
  - NVIDIA NVENC (`h264_nvenc`, `hevc_nvenc`)
  - AMD AMF (`h264_amf`, `hevc_amf`)
  - Intel QuickSync (`h264_qsv`, `hevc_qsv`)
  - Multi-threaded CPU Fallback (`libx264`, `libx265`)
- **Streaming Export Architecture:** Video frames and mixed audio chunks stream directly to the encoder pipe; memory consumption is bounded and does not grow with timeline duration.
- **Safe Atomic Export:** Output renders to temporary staging file (`.exporting.mp4`) and renames atomically upon completion, preventing corrupt partial exports.
- **Semantic Validation:** Post-render verification testing for non-black frame entropy, non-silent audio samples, and correct duration.

---

## 8. Known Limitations in v1.0

1. **Flat Multi-Track Layout:** Timeline nesting (placing compound timelines inside other timelines) is not supported in v1.0. Workaround: Render sub-sequences to intermediate MP4.
2. **Text / Title Overlays:** Rich text animators and vector fonts are authored via transparent PNG graphic overlays placed on upper video tracks.
3. **Audio Channel Layout:** Timeline output is calibrated exclusively for stereo ($2.0$) monitoring. Multi-channel surround ($5.1$, $7.1$) streams are automatically downmixed to stereo.

---

## 9. Explicitly Deferred Capabilities (Post-v1.0 Roadmap)

The following items are deferred until post-v1.0 and are not part of the v1.0 release contract:

1. **Neural AI / ML Inference Models:** Local ONNX / DirectML inference for Whisper automatic speech-to-text subtitling, neural video rotoscoping/matting, neural noise suppression, and automated visual reframing (currently using lightweight heuristic DSP routines).
2. **Compound Nested Sequences:** Sub-timeline nesting inside clips.
3. **Multichannel 5.1 / 7.1 Audio Mixing.**
4. **macOS Metal & Linux Vulkan Standalone GUI Ports.**
5. **GPU Vector Font / SDF Rich Text Animator.**
6. **Cloud Synchronization & Multi-User Collaboration.**
