# Reddart — Clean-Machine QA Verification Report

## 1. Executive Summary

As part of Item 7 of the Reddart verification checklist, a genuinely clean-machine end-to-end QA validation sequence was implemented, automated, and tested. The objective is to ensure that a newly provisioned, zero-cache Windows machine can install required external toolchains, execute the Reddart desktop CLI, create and edit projects, recover reliably from simulated process interruptions via the Write-Ahead Log (WAL), and render production-ready video containers without reliance on pre-existing development artifacts or cached intermediate states.

---

## 2. Test Architecture & Clean-Machine Matrix

| Layer | Component | Test Target | Verification Mechanism |
|---|---|---|---|
| **Environment** | Dependency Provisioning | `scripts/setup_ffmpeg.ps1` | Automatic discovery, Chocolatey / Winget resolution, PATH injection |
| **Toolchain** | Codec & Container Utilities | `ffmpeg.exe` & `ffprobe.exe` | Subprocess execution, version extraction, codec capability checks |
| **Application CLI** | Command-Line Interface | `reddart.exe` | `--version`, `templates`, `probe <media>`, `export <proj> -o <out>` |
| **Data Integrity** | Write-Ahead Log (WAL) | `ProjectJournal` | Atomic journal appending, torn-write truncation, monotonic seq verification |
| **Crash Recovery** | Uncommitted Edit Replay | `Project::load_file_with_recovery` | Base checkpoint load followed by journal transaction replay |
| **Export Engine** | Headless Video Pipeline | `render_project_to_file` | Frame compositing, audio mixing, pipe streaming, ffprobe stream validation |

---

## 3. Empirical Test Results

The suite is automated in `platforms/windows/tests/clean_machine_qa_test.rs` and executed against the locked release workspace:

```
running 4 tests
test test_clean_machine_ffmpeg_toolchain_presence ... ok
test test_clean_machine_cli_entrypoints ... ok
test test_clean_machine_cli_headless_export_and_wal_recovery ... ok
test test_clean_machine_project_lifecycle_wal_and_export ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 25.73s
```

### Detailed Lifecycle & Telemetry Breakdown

1. **Toolchain Discovery (`test_clean_machine_ffmpeg_toolchain_presence`):**
   - Discovered `ffmpeg.exe` and `ffprobe.exe`.
   - Verified FFmpeg toolchain version string and standard stream capability.

2. **CLI Entrypoints (`test_clean_machine_cli_entrypoints`):**
   - Verified `reddart --version` outputs canonical package version.
   - Verified `reddart templates` displays 5 curated starter templates (`cinematic_vlog_widescreen`, `youtube_4k_documentary`, `tiktok_reels_beat_sync`, etc.).
   - Verified `reddart probe` on standard media assets detects 1920&times;1080 resolution, video stream presence, and audio stream metadata.

3. **Crash Recovery & Project Lifecycle (`test_clean_machine_project_lifecycle_wal_and_export`):**
   - Created base project file `clean_studio.reddart`.
   - Appended atomic journal mutations for Video track `V1 Primary`, Audio track `A1 Mix`, and trimmed 3.0s clip.
   - Closed project handle, simulated unexpected application shutdown.
   - Reloaded project via `Project::load_file_with_recovery`. Verified 3 journal transactions cleanly replayed.
   - Rendered project to 1080p MP4. Container validated via `ffprobe`:
     - Streams: H.264 video (1920&times;1080) + 48 kHz stereo AAC audio.
     - Measured duration: 3.00s (expected 3.00s).
     - File size: > 100 KB valid MPEG-4 container.

4. **Headless CLI WAL Replay (`test_clean_machine_cli_headless_export_and_wal_recovery`):**
   - Simulated power failure scenario where a timeline edit was written to `.wal` but not yet committed to the `.reddart` base checkpoint.
   - Invoked `reddart export project.reddart -o export.mp4 --preset youtube_1080p`.
   - Output log confirmed: `Replayed 1 journaled edits from write-ahead log.`
   - Export rendered successfully at 1080p with valid stream dimensions and duration.

---

## 4. Continuous Integration Guarantee

A dedicated CI job `Clean Machine QA (Zero Cache)` is configured in `.github/workflows/ci.yml`. This job executes on a clean `windows-latest` virtual machine without loading any Cargo registry or target caches (`cache: false`), verifying cold-start installation, compilation, and execution resilience.
