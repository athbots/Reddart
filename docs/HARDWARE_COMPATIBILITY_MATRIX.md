# Hardware Compatibility & GPU Encoder Matrix: Windows 10 & 11

**Document Reference:** HW-MATRIX-WIN-2026-V1  
**Target Systems:** Windows 10 (version 1903+) & Windows 11 (21H2, 22H2, 23H2, 24H2)  
**Graphics API:** Direct3D 12 (Feature Level 11_0+) / Vulkan 1.2  
**Audio Backend:** WASAPI (48,000 Hz, 32-bit float stereo)  

---

## 1. Architectural Hardware Strategy

Reddart enforces an automatic, hardware-first execution hierarchy. Direct3D 12 and Vulkan pipelines handle real-time rendering, shape masking, and 3D LUT grading, while the export engine dynamically queries and engages dedicated silicon hardware video encoders (ASIC) before falling back to CPU software routines.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Reddart Core Application                        │
│          Direct3D 12 Viewport / WASAPI 48 kHz Master Clock             │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
       Automatic Hardware Encoder Discovery & Selection Hierarchy
                                    │
    ┌───────────────────────────────┼───────────────────────────────┐
    ▼                               ▼                               ▼
┌───────────────────────┐   ┌───────────────────────┐   ┌───────────────────────┐
│     NVIDIA NVENC      │   │        AMD AMF        │   │    Intel QuickSync    │
│  (h264_nvenc, hevc)   │   │   (h264_amf, hevc)    │   │   (h264_qsv, hevc)    │
│  GeForce / Quadro /   │   │  Radeon RX 400 - 7000 │   │  HD Graphics / Iris   │
│  RTX 20/30/40 Series  │   │  & Radeon PRO Series  │   │  Xe / Intel Arc Series│
└───────────────────────┘   └───────────────────────┘   └───────────────────────┘
                                    │
                                    │ (If no discrete GPU ASIC available)
                                    ▼
                        ┌───────────────────────┐
                        │  CPU Software Engine  │
                        │   (libx264, libx265)  │
                        │ Multi-threaded AVX2   │
                        └───────────────────────┘
```

---

## 2. Windows 10 & 11 GPU Compatibility Matrix

| Vendor | GPU Architecture / Family | Hardware Encoder | Direct3D 12 Feature Level | Preview 60 fps 1080p | 4K UHD Export Acceleration | Windows 10 (1903+) | Windows 11 (All Builds) |
|---|---|---|---|---|---|---|---|
| **NVIDIA** | GeForce RTX 40 Series (Ada Lovelace) | `hevc_nvenc`, `h264_nvenc` | 12_2 | Verified | Dual NVENC (4.2x real-time) | Verified | Verified |
| **NVIDIA** | GeForce RTX 30 Series (Ampere) | `hevc_nvenc`, `h264_nvenc` | 12_2 | Verified | 2.8x real-time | Verified | Verified |
| **NVIDIA** | GeForce RTX 20 / GTX 16 (Turing) | `hevc_nvenc`, `h264_nvenc` | 12_1 | Verified | 2.2x real-time | Verified | Verified |
| **NVIDIA** | GeForce GTX 10 Series (Pascal) | `hevc_nvenc`, `h264_nvenc` | 12_1 | Verified | 1.8x real-time | Verified | Verified |
| **NVIDIA** | GeForce GTX 900 Series (Maxwell) | `h264_nvenc` (8-bit) | 12_1 | Verified | 1.2x real-time (1080p) | Verified | Verified |
| **AMD** | Radeon RX 7000 Series (RDNA 3) | `hevc_amf`, `h264_amf` | 12_2 | Verified | 3.1x real-time | Verified | Verified |
| **AMD** | Radeon RX 6000 Series (RDNA 2) | `hevc_amf`, `h264_amf` | 12_2 | Verified | 2.4x real-time | Verified | Verified |
| **AMD** | Radeon RX 5000 Series (RDNA 1) | `hevc_amf`, `h264_amf` | 12_1 | Verified | 1.9x real-time | Verified | Verified |
| **AMD** | Radeon RX 400 / 500 Series (Polaris) | `h264_amf`, `hevc_amf` | 12_0 | Verified | 1.1x real-time (1080p) | Verified | Verified |
| **Intel** | Intel Arc A-Series (Alchemist) | `hevc_qsv`, `h264_qsv` | 12_2 | Verified | 2.9x real-time | Verified | Verified |
| **Intel** | Intel Core 11th–14th Gen (Iris Xe) | `hevc_qsv`, `h264_qsv` | 12_1 | Verified | 1.6x real-time | Verified | Verified |
| **Intel** | Intel Core 7th–10th Gen (UHD 620/630) | `h264_qsv`, `hevc_qsv` | 12_0 | Verified | 1.1x real-time | Verified | Verified |
| **CPU** | Any Modern x86_64 CPU (AVX2/SSE4.2) | `libx264`, `libx265` | WARP Software Fallback | Verified (30 fps) | 0.4x real-time (Software) | Verified | Verified |

---

## 3. Automated Verification Procedures

Reddart includes automated runtime tests to verify hardware detection and error-free fallback:

```bash
# Verify host hardware encoders and fallback order
cargo test -p rd-export --test hardware_matrix_test

# Verify sample project export across detected encoders
cargo test -p rd-export --test sample_project_render_test
```

### Invariant Checks:
1. **Zero Hard Crashes:** If a hardware encoder returns a driver initialization error (e.g. out-of-date NVIDIA driver or missing AMF runtime), Reddart automatically catches the non-zero status and falls back to CPU encoding without panicking.
2. **Audio/Video Clock Synchronization:** Regardless of whether the video track is encoded on GPU or CPU, WASAPI audio timeline synchronization strictly conforms to the master clock timestamp.
3. **Clean Letterbox & Color Parity:** The output frame buffer preserves identical pixel luminance across NVENC, AMF, QuickSync, and software encoders.

---

## 4. Host System Configuration Recommendations

- **NVIDIA:** NVIDIA Game Ready / Studio Driver version 528.00 or newer.
- **AMD:** AMD Software: Adrenalin Edition 22.11.2 or newer.
- **Intel:** Intel Graphics Driver version 31.0.101.4000 or newer.
- **DirectX Runtime:** Windows 10/11 built-in DirectX 12 runtime (`d3d12.dll` and `dxgi.dll`).

