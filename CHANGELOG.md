# Changelog

All notable changes to the Reddart video editor are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.2.0] - 2026-11-15

### Added
- **Hardware-Accelerated Optical Flow Retiming:** WGSL compute shader executing Horn-Schunck iterative relaxation across 8x8 thread workgroups (`effects/optical_flow/flow_estimate.wgsl`), dense motion field generation (`Rgba16Float` forward/backward vectors), and bidirectional motion-compensated frame interpolation with bilateral occlusion hole-filling. ([#35](https://github.com/athbots/Reddart/pull/35))
- **Nested Timelines & Compound Sequences:** Multi-level recursive timeline rendering supporting sub-sequences embedded as clips within master sequences (`ClipPayload::NestedSequence`), cycle detection, and high-performance pre-render frame cache (`NestedSequenceCache`) with deterministic mutation-hash invalidation. ([#37](https://github.com/athbots/Reddart/pull/37))
- **GPU Signed Distance Field (SDF) Text Rendering:** Direct WGSL vector-sharp typography shader with sub-pixel antialiasing via `smoothstep`, exact linear-time 2D Euclidean Distance Transform ($O(W \cdot H)$), R8Unorm hardware texture atlas, and rich typographic controls (tracking, leading, stroke outlines, drop shadows). ([#38](https://github.com/athbots/Reddart/pull/38))
- **HDR & ACES Digital Cinema Color Pipeline:** Zero-dependency OpenColorIO v2 YAML configuration parser (`rd_effects::ocio`), reference ACES 1.2/1.3 hierarchy, ACEScct transfer function with linear shadow toe ($L \le 0.0078125$), 10-bit and 12-bit quantizers with Triangular Probability Density Function (TPDF) blue noise dithering, and dynamic DXGI/wgpu surface capability negotiation selecting `Rgba16Float` or `Rgb10a2Unorm`. ([#39](https://github.com/athbots/Reddart/pull/39))
- **Cross-Platform Distribution Packaging:** FreeDesktop Flatpak 24.08 sandbox manifest (`platforms/linux/app.reddart.Reddart.yaml`) with Wayland, X11, DRI Vulkan, and PipeWire audio, Linux AppImage runner with Wayland fractional scaling auto-detection, and macOS DMG generation with hardened runtime codesigning and Apple Notarization CLI automation. ([#40](https://github.com/athbots/Reddart/pull/40))

---

## [1.1.0] - 2026-11-13

### Added
- **Clean-Machine QA Suite & Automated CI Runner:** Added `platforms/windows/tests/clean_machine_qa_test.rs` and `scripts/setup_ffmpeg.ps1` with an isolated zero-cache CI runner (`Clean Machine QA (Zero Cache)`) validating cold installation, toolchain setup, project editing, WAL recovery, and headless export. ([#32](https://github.com/athbots/Reddart/pull/32))
- **Streaming Export Verification Suite:** Added `crates/rd-export/tests/streaming_export_verification_test.rs` validating bounded $O(1)$ memory consumption (< 400 MB for 1080p, < 800 MB for 4K) and frame-accurate work-area range exports. ([#31](https://github.com/athbots/Reddart/pull/31))
- **Audio AV Sync & Stress Testing:** Added 9 audio stress tests in `crates/rd-audio/tests/audio_av_sync_stress_test.rs` validating 30-minute clock drift (< 0.02 µs), 1000 rapid seek operations, concurrent timeline edits during playback, and lock-free underrun recovery. ([#29](https://github.com/athbots/Reddart/pull/29))
- **Bit-for-Bit Preview/Export Parity Suite:** Added `crates/rd-export/tests/preview_export_parity_test.rs` proving exact visual parity (MSE = 0.0) between real-time viewport compositing and production rendering across multi-track footage, transforms, geometric shape masks, title cards, and 3D LUT grades. ([#28](https://github.com/athbots/Reddart/pull/28))
- **WAL Crash-Recovery & Fault Injection Suite:** Added `crates/rd-core/tests/wal_fault_injection_test.rs` validating torn-write truncation, sequence continuity checks, and automatic recovery from sudden process termination. ([#26](https://github.com/athbots/Reddart/pull/26))
- **Command Error Handling & Undo/Redo Audit:** Audited and resolved all unhandled command results across 23 editing mutations with a 100-cycle undo/redo test suite. ([#25](https://github.com/athbots/Reddart/pull/25))

### Changed
- **GPU Preview Integration:** Hooked `rd_render::Compositor` directly into native `wgpu::Device` and `wgpu::Queue`, validating sub-millisecond DAG compilation latencies. ([#27](https://github.com/athbots/Reddart/pull/27))
- **Export Work-Area Range Support:** Updated `render_project_to_file` and `mix_project_audio_range` to support temporal sequence slicing based on `mark_in_us` and `mark_out_us`. ([#31](https://github.com/athbots/Reddart/pull/31))

### Fixed
- **Website Encoding Corruption:** Eliminated multi-byte UTF-8 mojibake across `website/index.html`. ([#30](https://github.com/athbots/Reddart/pull/30))
- **Deterministic Clean-Build Reproducibility:** Updated Windows packaging script to enforce dual clean-build compilation passes.

---

## [1.0.0] - 2026-11-12

### Added
- **Official Production Release for Windows:** First feature-complete release featuring Direct3D 12 / Vulkan render graph compilation, 48 kHz WASAPI audio mixing, eased keyframe automation, geometric shape masking, and hardware video export (NVENC, QuickSync, AMF).

