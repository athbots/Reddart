# Contributing to Reddart

Thank you for your interest in contributing to Reddart! We welcome contributions from systems engineers, audio DSP specialists, GPU shader programmers, and creator workflow advocates.

---

## 1. Development Prerequisites

Reddart is built in modern Rust utilizing the **2024 Edition**.

- **Rust Toolchain:** Version 1.85.0 or later (`rustup update stable`).
- **Platform Toolchains:**
  - **Windows:** Visual Studio 2022 C++ Build Tools (MSVC) + Windows SDK.
  - **Linux:** GCC/Clang, `libasound2-dev`, `libvulkan-dev`, `pkg-config`.
  - **macOS:** Xcode Command Line Tools.
- **Media Engine Dependencies:** `ffmpeg` and `ffprobe` (v5.0+ required, v6.0+ recommended) in system `PATH` or placed in `bin/`. Required for importing real video/audio media assets, decoding, and hardware/software video export. Full integration tests (`cargo test --workspace` with `REDDART_REQUIRE_FFMPEG=1` in CI) enforce FFmpeg presence.
  - Windows: `winget install Gyan.FFmpeg` or `choco install ffmpeg`
  - macOS: `brew install ffmpeg`
  - Linux: `sudo apt install ffmpeg libasound2-dev pkg-config`

---

## 2. Workspace Architecture

Reddart enforces a modular crate structure with strict separation between engine domain logic, rendering DAG evaluation, audio mixing, and platform UI:

```
Reddart/
├── crates/
│   ├── rd-core/         # Pure domain timeline data structures, WAL journal, keyframes
│   ├── rd-render/       # Render DAG compilation, topological sort, frame quality governor
│   ├── rd-effects/      # WGSL shaders, 3D LUT parser, geometric masks, starter templates
│   ├── rd-audio/        # 48 kHz low-latency WASAPI audio mixer, clip resampling, limiter
│   ├── rd-decode/       # Video decoding abstractions, synthetic fallback generator
│   ├── rd-export/       # Multi-track export pipeline, hardware encoder discovery (NVENC/AMF/QSV)
│   ├── rd-ml/           # On-device ML pipelines (Whisper captions, speech denoise)
│   ├── rd-android/      # Android JNI bindings
│   └── rd-apple/        # Apple macOS/iOS Metal bindings
├── platforms/
│   └── windows/         # Windows desktop interface (egui 0.31 + wgpu 24 + CLI)
├── ready_to_use/        # Pre-compiled standalone binaries and distribution packages
└── docs/                # Architectural whitepapers, codec audits, and alpha guides
```

---

## 3. Local Verification & Invariants

Before submitting any Pull Request, your changes MUST satisfy the following quality invariants:

### 1. Test Suite (All tests must pass)
```bash
cargo test --workspace
```
*Current test suite enforces over 130 unit and integration tests across timeline correctness, fuzzing, journal recovery, audio mixing, optical flow, and export pipelines.*

### 2. Strict Clippy Compliance (Zero warnings allowed)
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### 3. Formatting
```bash
cargo fmt --all -- --check
```
*To automatically format: `cargo fmt --all`.*

---

## 4. Pull Request Guidelines

1. **Focus & Modularity:** Keep PRs targeted to a single feature, bug fix, or subsystem improvement.
2. **Deterministic Timeline Invariants:** Any modification to `rd-core` timeline operations (split, ripple trim, roll, slip, slide) must include corresponding unit tests in `crates/rd-core/tests/`.
3. **No Unaudited C FFI:** Reddart avoids direct C ABI dependencies in core crates. External media tools must interface via subprocess IPC or memory-safe Rust abstractions.
4. **Zero Telemetry Guarantee:** Reddart never transmits user data, project metadata, or usage analytics to remote servers. Code introducing background network traffic will be rejected.
5. **Licensing:** All contributions are dual-licensed under **MIT OR Apache-2.0**.

