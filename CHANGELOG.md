# Changelog

All notable changes to the Reddart video editor are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

