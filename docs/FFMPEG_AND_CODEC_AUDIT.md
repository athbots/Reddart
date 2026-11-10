# FFmpeg & Video Codec Redistribution Audit

**Document:** AUDIT-FFMPEG-2026-V1  
**Project:** Reddart Video Editor  
**Version:** v0.2.1  
**Author:** Reddart Systems Engineering & Compliance  
**Status:** Approved Architectural Policy  

---

## 1. Architectural Boundary & Relationship to FFmpeg

Reddart is designed with a strict isolation boundary between its proprietary/open-source Rust core and external media decoding/encoding engines:

```
┌────────────────────────────────────────────────────────┐
│                   Reddart Workspace                    │
│      (Rust 2024 / wgpu / Direct3D 12 / WASAPI)         │
│          Dual Licensed: MIT OR Apache-2.0              │
└───────────────────────────┬────────────────────────────┘
                            │
               Subprocess IPC (Standard I/O)
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│                External FFmpeg / ffprobe               │
│              (Discovered on System PATH)               │
│        Governed by LGPL v2.1+ or GPL v2.0+ / v3.0+     │
└────────────────────────────────────────────────────────┘
```

1. **No Static or Dynamic C Symbol Linking:** Reddart does not compile or link against `libavcodec`, `libavformat`, `libavutil`, or `libswscale` via C FFI (`extern "C"`). The compiled `Reddart.exe` binary contains zero FFmpeg object code.
2. **Subprocess Invocation:** Interfacing with FFmpeg and ffprobe occurs strictly through standard OS process execution (`std::process::Command`), passing command-line arguments and reading structured streams via standard pipes (`stdin`, `stdout`, `stderr`).
3. **Pure-Rust Fallback Invariant:** If `ffmpeg` and `ffprobe` are not installed or discoverable on the host environment, Reddart maintains full operational integrity for editing, UI playback, timeline manipulation, and synthetic test-pattern rendering via `rd-decode`.

---

## 2. FFmpeg Build Configurations & Licensing Obligations

When building or distributing FFmpeg alongside Reddart or guiding users on compilation, the following licensing regimes apply:

| Configuration Profile | Typical Flags | Applicable License | Redistribution Rights |
|---|---|---|---|
| **LGPL Standard** | `--enable-shared --disable-gpl --disable-nonfree` | LGPL v2.1 or later | May be distributed dynamically alongside applications. |
| **GPL Standard (Recommended for software x264/x265)** | `--enable-gpl --enable-libx264 --enable-libx265` | GPL v2.0+ or v3.0+ | Binary may be redistributed provided full FFmpeg source and build scripts are offered under GPL. |
| **Non-Free (PROHIBITED)** | `--enable-nonfree` (e.g. `libfdk_aac`) | Proprietary / Non-Redistributable | **DO NOT REDISTRIBUTE.** Binaries built with `--enable-nonfree` cannot be legally redistributed in compiled form. |

### Canonical Recommended Build Flags for Windows / Linux
```bash
./configure \
  --prefix=/usr/local \
  --enable-gpl \
  --enable-version3 \
  --enable-shared \
  --disable-static \
  --enable-libx264 \
  --enable-libx265 \
  --enable-libvpx \
  --enable-libvorbis \
  --enable-libopus \
  --enable-nvenc \
  --enable-amf \
  --disable-debug \
  --disable-doc
```

---

## 3. Hardware Acceleration vs Software Codecs & Patent Pools

Reddart's export pipeline (`rd-export`) enforces an **optimal hardware-first hierarchy**:

```
1. NVIDIA NVENC (h264_nvenc, hevc_nvenc)   ──▶ Hardware Silicon (Driver API)
2. AMD AMF (h264_amf, hevc_amf)             ──▶ Hardware Silicon (Driver API)
3. Intel QuickSync (h264_qsv, hevc_qsv)     ──▶ Hardware Silicon (Driver API)
4. Apple VideoToolbox (h264_videotoolbox)   ──▶ Hardware Silicon (Driver API)
5. CPU Fallback (libx264, libx265)          ──▶ Software Encoding (FFmpeg GPL)
```

### Patent Liability Analysis:
- **Hardware Encoders (NVENC, AMF, QSV, VideoToolbox):**  
  Encoding instructions are passed directly to hardware ASIC chips on the host GPU. Patent licensing for H.264/AVC (MPEG LA) and H.265/HEVC (HEVC Advance / Access Advance) is covered by the GPU manufacturer's end-user hardware license. Reddart incurs zero patent royalty obligations when leveraging native hardware encoders.
- **Software Codecs (`libx264`, `libx265`):**  
  Software encoders contain patented algorithmic logic. Reddart does not compile or vendor software codec implementations. When running on systems where users have installed FFmpeg with x264/x265, users operate under standard personal or fair-use exemptions.

---

## 4. Official Distribution Guidelines

To maintain complete legal compliance with both copyright licensing (LGPL/GPL) and international software patent frameworks:

1. **Standalone Binary Releases:**  
   The primary release artifacts (`ready_to_use/windows/Reddart.exe`) MUST NOT bundle unvetted FFmpeg DLLs or executables in the base repository.
2. **Package Manager Integration:**  
   Documentation directs end users to obtain FFmpeg from established, compliant distribution repositories:
   - Windows: `winget install Gyan.FFmpeg` or `choco install ffmpeg`
   - macOS: `brew install ffmpeg`
   - Linux: `sudo apt install ffmpeg` or `sudo dnf install ffmpeg`
3. **Custom Studio Bundling:**  
   If an enterprise customer or distribution packager chooses to package FFmpeg with Reddart in a single installer:
   - The installer must clearly provide the source code or a written offer for the exact FFmpeg build used.
   - The installer must include copies of `COPYING.LGPLv2.1` or `COPYING.GPLv2`/`COPYING.GPLv3`.
   - Separate directory structures must be maintained (`Reddart/bin/Reddart.exe` and `Reddart/ffmpeg/bin/ffmpeg.exe`).

---

## 5. Compliance Verification Invariants

Reddart's CI and automated test suites verify compliance:
- `rd-export` verifies encoder discovery dynamically at runtime without hardcoded binary paths.
- `cargo test --workspace` validates that all tests pass even in completely isolated environments without FFmpeg present on disk.

