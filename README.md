# Reddart

Reddart is a standalone, offline non-linear video editor (NLE) written in Rust. It provides a multi-track editing timeline, real-time render graph compilation via Direct3D 12 and Vulkan, 48 kHz low-latency stereo audio mixing, keyframe automation, geometric shape masking, and hardware-accelerated export.

No cloud accounts. No telemetry. No subscriptions. No background network traffic.

```
                    ┌────────────────────────────────────────┐
                    │               Reddart UI               │
                    │         (egui / wgpu viewport)         │
                    └───────────────────┬────────────────────┘
                                        │
           ┌────────────────────────────┼───────────────────────────┐
           ▼                            ▼                           ▼
┌────────────────────┐       ┌────────────────────┐      ┌────────────────────┐
│      rd-core       │       │     rd-render      │      │      rd-audio      │
│  Timeline & DAG    │       │  Directed Acyclic  │      │  48 kHz WASAPI Bus │
│  WAL Journal Log   │──────▶│   Graph Evaluator  │      │  Clip Gain & Pan   │
│  Keyframe Splines  │       │  WGSL Pixel Shaders│      │  3-Band EQ & Limit │
└────────────────────┘       └────────────────────┘      └────────────────────┘
           │                            │                           │
           └────────────────────────────┼───────────────────────────┘
                                        │
                                        ▼
                             ┌────────────────────┐
                             │     rd-export      │
                             │ Hardware Encoders: │
                             │ NVENC / AMF / QSV  │
                             │ libx264 / libx265  │
                             └────────────────────┘
```

---

## Capabilities

### 1. Multi-Track Timeline & Editing Tools
- **Track Layout:** Arbitrary counts of video (`TrackKind::Video`) and audio (`TrackKind::Audio`) tracks with independent mute, solo, lock, and volume controls.
- **Editing Tools:**
  - `Selection (V)`: Select, drag, reposition, and trim clips.
  - `Razor Split (C)`: Slice clips precisely at the current playhead timestamp down to microsecond precision.
  - `Ripple Trim (B)`: Shorten or extend clip in/out boundaries while shifting downstream clips automatically to eliminate gaps.
  - `Roll Edit (N)`: Move the edit boundary between two adjacent clips without altering sequence timing.
  - `Slip (Y)`: Shift a clip's source in/out window while leaving its timeline position and duration unchanged.
  - `Slide (U)`: Move a clip along the timeline while adjusting adjacent in/out points to maintain total sequence duration.
- **Markers & Work Areas:**
  - Timeline markers (`M`) with frame-accurate timestamp annotations.
  - Mark-In (`I`) and Mark-Out (`O`) work area bounds for scoped preview and partial export.
  - Magnetic snapping to clip boundaries, playhead, and marker locations.

### 2. Keyframe Automation & Splines
Reddart provides an automation engine supporting 5 interpolation modes per parameter:
- **Interpolation Curves:**
  - `Linear`: Constant rate of change between keyframes.
  - `EaseIn`: Quadratic acceleration ($t^2$) departing from rest.
  - `EaseOut`: Quadratic deceleration ($1 - (1 - t)^2$) into the target value.
  - `EaseInOut`: Symmetric cubic S-curve ($3t^2 - 2t^3$) for natural motion.
  - `Hold`: Discrete step; retains the source value until the next keyframe timestamp.
- **Animatable Properties:**
  - Affine transforms: `translate_x`, `translate_y`, `scale_x`, `scale_y`, `rotation_deg`.
  - Layer blending: `opacity`.
  - Audio: clip-level `volume` (gain multiplier) and stereo `pan` (-1.0 left to +1.0 right).
  - Playback rate: `speed` remapping.
  - Mask geometry: `center_x`, `center_y`, `width`, `height`, and `feather`.

### 3. Masking Engine
Pixel-accurate shape masks evaluated in real-time within the render graph:
- **Geometry Types:**
  - `Rectangle`: Centered rectangular window with independently adjustable width and height.
  - `Ellipse`: Radial mask with separate horizontal and vertical radius controls.
  - `Linear Gradient`: Directional alpha ramp for soft feathered blends and reveals.
  - `Vignette`: Radial falloff with edge dampening.
- **Controls:**
  - `Feather`: Soft-edge boundary radius normalized across image space.
  - `Invert`: Toggle between isolating interior content and cutting out a stencil window.
  - Real-time parameter automation via keyframes.

### 4. Audio Engine (48 kHz WASAPI)
- **Signal Path:** Native 48,000 Hz, 32-bit floating-point stereo processing pipeline.
- **Clip Processing:** Dynamic linear resampling, automated volume envelopes, and constant-power stereo panning.
- **Track Equalization:** 3-band parametric EQ per track (100 Hz low shelf, 1,000 Hz mid bell, 8,000 Hz high shelf).
- **Master Bus:** Lookahead soft-knee limiter (`tanh` saturation curve) preventing inter-sample digital clipping.
- **Metering:** Ballistic peak level monitoring with true dBFS readout (-60 dBFS to 0 dBFS) and visual overload warnings.

### 5. Variable Speed Remapping
- Continuous playback rate adjustment from **0.25x (quarter speed slow-motion)** up to **4.0x (fast motion)**.
- Bidirectional time mapping: maps timeline playhead microsecond timestamps directly to source media frame offsets.
- Automatic timeline clip duration compensation when altering playback rates.

### 6. Crash Resilience & Storage
- **Atomic Journaling:** Projects use a binary Write-Ahead Log (`.reddart.wal`) alongside the base `.reddart` file.
- **Integrity Verification:** Every transaction entry contains a 32-bit CRC32 checksum, sequence index, and operation payload.
- **Torn-Write Recovery:** If the application terminates abruptly (power outage, crash, forced kill), uncommitted operations are replayed on next launch. Damaged trailing writes are safely truncated.
- **Safe Checkpoints:** Checkpoints serialize to a temporary file (`.tmp`) before atomic rename replacement, preventing 0-byte corruptions.

### 7. Hardware-Accelerated Export
Reddart detects host hardware capabilities and automatically selects the optimal encoder backend:
- **Supported Encoders:**
  - NVIDIA NVENC (`h264_nvenc`, `hevc_nvenc`)
  - AMD AMF (`h264_amf`, `hevc_amf`)
  - Intel QuickSync (`h264_qsv`, `hevc_qsv`)
  - Apple VideoToolbox (`h264_videotoolbox`, `hevc_videotoolbox`)
  - Software Fallback (`libx264`, `libx265`)
- **Default Presets:**
  - `youtube_1080p`: 1920x1080 @ 30 fps, High Profile H.264, 48 kHz stereo AAC (320 kbps).
  - `uhd_4k`: 3840x2160 @ 30 fps UHD Master, Main10 HEVC / H.264, 48 kHz stereo AAC.
  - `vertical_9_16_1080p`: 1080x1920 @ 30 fps vertical format for Shorts, TikTok, and Reels.
  - `instagram_square`: 1080x1080 @ 30 fps square format for social media feeds.

---

## Command-Line Interface (CLI)

The compiled `reddart` binary can be run as a standalone desktop application or driven headlessly from the terminal for batch processing, verification, and diagnostics.

### Reference

```
reddart [OPTIONS] [PROJECT_FILE]
reddart <COMMAND> [ARGS...]
```

| Command / Option | Description |
|---|---|
| `reddart [file.reddart]` | Launch GUI Studio, loading the specified project file immediately. |
| `reddart export <proj> -o <out> [--preset <p>]` | Headlessly render a project to MP4 with terminal ASCII progress bar and speed multiplier. |
| `reddart probe <media_file>` | Inspect container streams, codecs, resolution, aspect ratio, and SMPTE duration via `ffprobe`. |
| `reddart templates` | Print formatted summary of all built-in starter templates. |
| `reddart benchmark` | Execute timeline render graph DAG compiler latency benchmarks across multiple track loads. |
| `reddart -V, --version` | Output version number and engine platform details. |
| `reddart -h, --help` | Display terminal CLI help reference and keyboard shortcuts table. |

### CLI Examples

**Render a project headlessly:**
```bash
reddart export my_edit.reddart -o output.mp4 --preset youtube_1080p
```
Output:
```
Executing Headless Export Pipeline:
  Project Name:     Short Promo
  Output Location:  output.mp4
  Format:           YouTube 1080p (1920x1080 @ 30 fps)
  Video Encoder:    h264_nvenc (hardware acceleration active)
  Audio Format:     48 kHz stereo AAC (320 kbps)
  Sequence Length:  12.00s
[========================] 100% (360/360) [1.85x real-time, ETA: 0.0s]
Export finished successfully in 6.48s (1.9x real-time speed).
File size: 14.82 MB
```

**Probe a media asset:**
```bash
reddart probe raw_camera_footage.mp4
```
Output:
```
Inspecting container: raw_camera_footage.mp4
  Video Stream: Detected
  Resolution:   1920x1080
  Aspect Ratio: 1.778 (16:9 widescreen)
  Audio Stream: Detected
  Duration:     32.000s (SMPTE: 00:00:32:00)
```

**List project templates:**
```bash
reddart templates
```
Output:
```
Available Starter Project Templates (5 curated):
ID                           Name                     Aspect   Resolution   Duration   Slots 
------------------------------------------------------------------------------------------
tiktok_reels_beat_sync       Reels / TikTok Beat Cut  9:16     1080x1920    10.0s      5     
cinematic_vlog_widescreen    Cinematic Vlog 16:9      16:9     1920x1080    12.0s      3     
social_square_highlight      Social Square 1:1        1:1      1080x1080    8.0s       2     
youtube_4k_documentary       YouTube 4K Documentary   16:9     3840x2160    16.0s      4     
fast_action_music_video      High-Tempo Action Video  16:9     1920x1080    8.0s       8     
------------------------------------------------------------------------------------------
Use 'reddart' to launch Studio and press Ctrl+T to instantiate any template.
```

---

## Performance Benchmarks

Reddart evaluates timeline state into an execution DAG on every frame change. Benchmarks measured on an Intel Core i9-13900K / 64 GB DDR5 / Windows 11 system:

| Workload Profile | Tracks | Total Clips | Iterations | Latency p50 | Latency p95 | Latency p99 | Mean Latency |
|---|---|---|---|---|---|---|---|
| Single Track Sequence | 1 | 5 | 1,000 | **2.60 µs** | 3.00 µs | 4.80 µs | 2.68 µs |
| Mainstream Desktop Multi-Track | 4 | 40 | 1,000 | **14.90 µs** | 24.40 µs | 34.50 µs | 15.71 µs |
| High-End Multi-Layer Workstation | 8 | 160 | 1,000 | **26.10 µs** | 48.80 µs | 86.50 µs | 30.19 µs |
| Stress Test Sequence | 16 | 800 | 500 | **47.80 µs** | 69.10 µs | 81.30 µs | 51.25 µs |

*At 47.8 µs p50 latency for 800 clips, DAG compilation consumes less than 0.3% of a 16.6 ms (60 fps) frame budget, leaving over 99.7% of the frame interval for GPU rendering and decoding.*

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Space` | Toggle playback (Play / Pause) |
| `J` | Shuttle backwards (1x, 2x, 4x, 8x speed) |
| `K` | Stop playback / cancel shuttle |
| `L` | Shuttle forward (1x, 2x, 4x, 8x speed) |
| `C` | Razor tool (split clip at playhead) |
| `B` | Ripple trim tool (trim boundary and close gap) |
| `N` | Roll edit tool (adjust boundary preserving duration) |
| `Y` | Slip tool (shift content within boundary) |
| `U` | Slide tool (reposition clip adjusting neighboring cuts) |
| `V` | Selection tool (move clips & drag endpoints) |
| `I` | Set Mark-In work area boundary |
| `O` | Set Mark-Out work area boundary |
| `M` | Add cue marker at current playhead timestamp |
| `Ctrl + T` | Open Starter Project Templates gallery |
| `Ctrl + S` | Force project flush & checkpoint |
| `Ctrl + E` | Open Hardware Export dialog |
| `Ctrl + Z` | Undo last edit command |
| `Ctrl + Y` / `Ctrl + Shift + Z` | Redo last undone command |

---

## Workspace Layout

```
Reddart/
├── crates/
│   ├── rd-core/         # Timeline data structures, track management, WAL journal, keyframes
│   ├── rd-render/       # Render DAG compilation, topological sort, frame cache, governor
│   ├── rd-effects/      # WGSL shader library, 3D LUT parser, geometric masks, templates
│   ├── rd-audio/        # 48 kHz WASAPI audio mixer, clip resampling, 3-band EQ, soft limiter
│   ├── rd-decode/       # Video decoding abstractions, software frame generator, seek cache
│   ├── rd-export/       # NVENC/AMF/QSV/libx264 hardware export pipeline, progress events
│   ├── rd-ml/           # Experimental on-device ML interfaces (deferred post-v1)
│   ├── rd-android/      # Android JNI bindings and surface rendering bridge
│   └── rd-apple/        # Apple macOS/iOS Metal rendering and platform bindings
├── platforms/
│   └── windows/         # Windows desktop application (winit, egui, wgpu, CLI entry points)
├── ready_to_use/
│   └── windows/         # Pre-compiled Windows x86_64 binary and version manifest
├── tests/               # Workspace integration tests, timeline fuzzing, gate verification
├── Cargo.toml           # Workspace manifest (v0.2.1)
└── README.md            # Technical documentation
```

---

## Building from Source

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (version 1.85.0 or later for Rust 2024 Edition support).
- Windows 10/11 with MSVC C++ Build Tools installed.
- Optional: `ffmpeg` and `ffprobe` installed on system `PATH` for video container probing and software encoding fallback.

### Compilation

Clone the repository and compile using Cargo:

```bash
# Clone the repository
git clone https://github.com/athbots/Reddart.git
cd Reddart

# Run workspace unit and integration test suite (113 tests)
cargo test --workspace

# Build optimized release binary
cargo build --release -p reddart-windows
```

The compiled binary will be placed at:
```
target/release/reddart.exe
```

A pre-built release binary is maintained at:
```
ready_to_use/windows/Reddart.exe
```

---

## Verification & Testing

The workspace test suite enforces timeline invariants, arithmetic limits, and file integrity across all crates:

```bash
cargo test --workspace
```

Test coverage includes:
- **Timeline Invariants:** Ripple trim, split, duplicate, roll, slip, slide, and track reordering (`rd-core`).
- **Fuzz Testing:** Extremes fuzzing, deep undo/redo cycles, division-by-zero resilience, and 1,000-clip scrubbing (`rd-core`).
- **Journal Recovery:** CRC32 checksum verification, transaction replay, and torn-write truncation (`rd-core`).
- **Audio Mixing:** Multichannel summation, volume scaling, stereo pan balance, and soft limiting saturation (`rd-audio`).
- **Effects & Masking:** Shape mask rasterization, invert logic, feather interpolation, and 3D LUT parsing (`rd-effects`).
- **Render Graph:** DAG compilation, node hashing, topological sorting, and frame quality adaptation (`rd-render`).
- **Export Pipeline:** Container generation, audio muxing, and probe verification (`rd-export`).

---

## Documentation & Beta Resources

- **Feature Implementation & Verification Matrix:** [docs/FEATURE_IMPLEMENTATION_MATRIX.md](docs/FEATURE_IMPLEMENTATION_MATRIX.md)
- **WAL Transaction Semantics & Durability:** [docs/WAL_TRANSACTION_SEMANTICS.md](docs/WAL_TRANSACTION_SEMANTICS.md)
- **FFmpeg & Codec Distribution Compliance:** [docs/FFMPEG_AND_CODEC_AUDIT.md](docs/FFMPEG_AND_CODEC_AUDIT.md)
- **Hardware Compatibility Matrix (Windows 10/11 GPU):** [docs/HARDWARE_COMPATIBILITY_MATRIX.md](docs/HARDWARE_COMPATIBILITY_MATRIX.md)
- **Reproducible Release Builds Protocol:** [docs/REPRODUCIBLE_BUILDS.md](docs/REPRODUCIBLE_BUILDS.md)
- **Creator Workflows & Closed-Alpha Plan:** [docs/CREATOR_WORKFLOWS_AND_ALPHA_PLAN.md](docs/CREATOR_WORKFLOWS_AND_ALPHA_PLAN.md)
- **Creator Onboarding & 60-Second First Export:** [docs/ONBOARDING_GUIDE.md](docs/ONBOARDING_GUIDE.md)
- **Windows Distribution Validation:** [docs/WINDOWS_DISTRIBUTION_VALIDATION.md](docs/WINDOWS_DISTRIBUTION_VALIDATION.md)
- **Public Beta Release Notes & Limitations:** [docs/RELEASE_NOTES_AND_LIMITATIONS.md](docs/RELEASE_NOTES_AND_LIMITATIONS.md)
- **Commercial Support & Studio Services:** [docs/COMMERCIAL_SUPPORT_OFFERING.md](docs/COMMERCIAL_SUPPORT_OFFERING.md)
- **Official Website:** [https://athbots.github.io/Reddart/](https://athbots.github.io/Reddart/) (source in `website/`)

---

## License

Reddart is available under standard dual licensing (MIT / Apache-2.0). See [LICENSE](LICENSE) for details.
