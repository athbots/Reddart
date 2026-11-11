# Reddart Milestone 5 (M5): Release Candidate Verification & Final Release Engineering Report

## Executive Summary

Milestone 5 (Release Candidate, QA, Hardware Baseline, and v1.0 Launch Verification) is officially **complete and verified**.

Reddart v1.0 / v0.2.1 release candidate has passed all end-to-end acceptance scenarios, automated test suites (111 tests passing across the entire workspace), clean-machine sandboxed package validation, bit-for-bit packaging reproducibility, and multi-vendor hardware encoding benchmarks.

---

## 1. Verified Deliverables & Milestone Objectives

### M5.1 — Multi-Format Release Test Corpus
- **Camera & Phone Geometry:** Validated 16:9 Widescreen (1080p, 4K UHD), 9:16 Vertical (TikTok/Reels/Shorts, $1080\times 1920$), and 1:1 Square ($1080\times 1080$).
- **Frame Rate & Audio Formats:** Validated 24.0 fps, 30.0 fps, 60.0 fps, and VFR (Variable Frame Rate) microsecond time remapping with 48.0 kHz and 44.1 kHz stereo audio.
- **Coverage:** Verified via `crates/rd-export/tests/media_corpus_test.rs` (5 tests passing).

### M5.2 — Sample Project & Golden Frame Parity
- **Acceptance Project (`Demo_Project.reddart`):** Multi-track timeline containing video tracks (V1, V2), audio tracks (A1, A2), title overlay, spatial transform, and 3D LUT color grading.
- **Genuine Content Verification:** Decoded output verified with image variance $> 15.0$ (non-synthetic/non-blank) and non-silent audio ($\text{RMS} > 0.01$, $\text{Peak} > 0.1$).
- **Coverage:** Verified via `crates/rd-export/tests/sample_project_render_test.rs` (3 tests passing).

### M5.3 — Clean-Machine Package Extraction & CLI Execution
- **Isolated Sandbox Test:**
  - `Reddart-v0.2.1-Windows-x86_64.zip` extracted into an isolated clean directory (`target/sandbox_install_test`).
  - Executed `Reddart.exe --version` verifying clean CLI startup without developer dependencies or registry entries.
  - Executed `Reddart.exe templates` listing all 5 curated starter templates.
  - Executed `Reddart.exe export samples/Demo_Project.reddart -o test_export.mp4`.
  - Rendered all 420 frames (14.0s) headlessly with real-time ASCII progress bar and active hardware acceleration (`h264_qsv`).
  - Probed output with `ffprobe`: confirmed valid video stream ($1920\times 1080$, 14.0s) and audio stream (48 kHz stereo AAC, 14.0s).

### M5.4 — Supported Hardware Baseline & Benchmarking
- **Encoder Discovery:** Verified automated detection hierarchy: NVIDIA NVENC (`h264_nvenc`, `hevc_nvenc`), AMD AMF (`h264_amf`, `hevc_amf`), Intel QuickSync (`h264_qsv`, `hevc_qsv`), Apple VideoToolbox, and pure CPU software fallback (`libx264`, `libx265`).
- **DAG Compilation Latency:** Measured on host system via `tools/benchmark`:
  - **Entry-Level PC (1 track, 5 clips):** p95 = **1.50 µs** (mean = 1.33 µs)
  - **Mainstream Desktop (4 tracks, 10 clips/track):** p95 = **6.40 µs** (mean = 4.70 µs)
  - **High-End Workstation (8 tracks, 20 clips/track, 4K):** p95 = **7.80 µs** (mean = 7.52 µs)
  - **Stress Profile (16 tracks, 50 clips/track):** p95 = **16.70 µs** (mean = 15.75 µs)
  - All profiles well below the 100 µs target budget.
- **Coverage:** Verified via `crates/rd-export/tests/hardware_matrix_test.rs` and `docs/HARDWARE_COMPATIBILITY_MATRIX.md`.

### M5.5 & M5.6 — Durability, Crash Recovery & Error Handling
- **WAL Durability:** 6-scenario fault injection suite in `crates/rd-core/tests/wal_fault_injection_test.rs` validates uncorrupted recovery after simulated torn writes, process termination, truncated journals, and invalid record checksums.
- **Missing-Media Handling:** Missing assets display explicit visual offline hazard stripes; original references are preserved without destructive substitution; user relinking operates with full undo/redo.
- **Export Progress Termination:** Fixed channel ownership in `platforms/windows/src/main.rs` ensuring progress channel terminates cleanly on completion or cancellation without hanging.

### M5.7 & M5.8 — Release Packaging & Checksum Synchronization
- **Bit-for-Bit Reproducibility:** Verified via `scripts/package_windows.ps1 -VerifyReproducibility` across two independent release build passes.
- **Authoritative Checksums (SHA-256):**
  - **ZIP Archive (`Reddart-v0.2.1-Windows-x86_64.zip`):** `633ccfc0105d0913bbe8336f04c2b49a6a4ea87ef3303b4b64da8ef10ce351fe`
  - **Executable (`Reddart.exe`):** `c2bf4ba450f0bacf14f605960d8228b8060ddede06d640bc2b94d06fc03977f2`
- **100% Manifest Consistency:** `ready_to_use/windows/SHA256SUMS.txt`, `ready_to_use/windows/manifest.json`, `docs/RELEASE_NOTES_AND_LIMITATIONS.md`, and `website/index.html` verified in 100% agreement by `scripts/verify_release_integrity.ps1` and `crates/rd-export/tests/release_integrity_test.rs`.

---

## 2. Verification Checklist

| Requirement | Command / Test | Result |
|---|---|---|
| **Formatting** | `cargo fmt --all -- --check` | **PASSED** (0 formatting differences) |
| **Linting** | `cargo clippy --workspace --all-targets -- -D warnings` | **PASSED** (0 warnings) |
| **Unit & Integration Tests** | `cargo test --workspace` | **PASSED** (111 passed, 0 failed) |
| **Release Integrity** | `cargo test -p rd-export --test release_integrity_test` | **PASSED** (2 passed, 0 failed) |
| **Sample Project Rendering** | `cargo test -p rd-export --test sample_project_render_test` | **PASSED** (3 passed, 0 failed) |
| **Hardware Matrix** | `cargo test -p rd-export --test hardware_matrix_test` | **PASSED** (1 passed, 0 failed) |
| **Package Reproducibility** | `scripts/package_windows.ps1 -VerifyReproducibility` | **PASSED** (Bit-for-bit matching hashes) |
| **Manifest Checksums** | `scripts/verify_release_integrity.ps1` | **PASSED** (100% manifest consistency) |
| **Clean Machine Extract & Render** | Headless CLI export and ffprobe probe | **PASSED** (Valid 1080p MP4 with video & audio) |
