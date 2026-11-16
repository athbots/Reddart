# Reddart v1.3.0 Production Release: Multi-GPU, VST3 Hosting, Hardware AV1/HEVC & AI Inpainting/Reframe

**Release Tag:** `v1.3.0`  
**Distribution Archive:** `Reddart-v1.3.0-Windows-x86_64.zip`  
**Binary Checksum (SHA-256):** `db2a22696fdec28df2c13cf5a50b9fad289235e9dbbc2cdb9ab97de5dc49cdfd`  
**Archive Checksum (SHA-256):** `67bb65a3a35a221eb9e72540b269e3d4daa97bcee3cfd7a4740e15f4aebf0077`  
**MSRV:** Rust 1.85.0 (2024 Edition)  

---

## 1. Verified Production Capabilities

Reddart v1.3.0 delivers major Phase III architectural expansions, bringing enterprise-grade multi-GPU distributed rendering, studio VST3/AU audio plugin hosting, hardware-accelerated AV1 and HEVC codecs, and AI generative video inpainting and smart multi-subject reframing:

### Multi-GPU Distributed Rendering & Peer-to-Peer NVLink
- **Adapter Fleet Discovery:** Queries all physical graphics adapters, classifying VRAM capacity, driver architecture, PCIe/NVLink interconnect bandwidth, and multi-GPU capabilities.
- **Workload Partitioning:** Spatial split-frame rendering with configurable halo margins to eliminate edge seams, alternate-frame temporal chunking, and compute-offload pipelines.
- **Bit-for-Bit Tile Stitching:** Memory-safe parallel tile recombination with single-GPU fallback when multi-adapter is unavailable.

### Real-Time VST3 / AU Audio Plugin Hosting
- **Zero-Heap In-Place DSP:** Audio plugin architecture operating directly on contiguous channel buffers without intermediate heap allocations.
- **Serial Insert Chains:** Multi-slot plugin chains with latency tracking, compensation, bypass states, and JSON state persistence.
- **Built-in Studio DSP Suite:** Studio Compressor (RMS/peak detection, soft knee, auto make-up gain), 4-Band Parametric EQ (Low/High Shelf, Bell filters with bilinear transform), and True Peak Brickwall Limiter.

### Hardware-Accelerated AV1 & HEVC Codec Expansion
- **Hardware Encoder Pipeline:** Zero-copy hardware-accelerated encoding via NVENC, Intel QuickSync, AMD AMF, and Apple VideoToolbox for AV1 and HEVC (H.265).
- **10-Bit HDR & Color Signaling:** Full BT.2020 PQ (HDR10) and HLG metadata signaling with high-efficiency 10-bit pixel formats (`p010le`, `yuv420p10le`).
- **Flexible Rate Control:** Support for CBR, VBR, CRF, and Constrained VBR modes with seamless software encoder fallback (`libsvtav1`, `libx265`, `libx264`).

### AI Generative Video Inpainting & Smart Saliency Reframe
- **Temporal Video Completion:** Optical flow-guided pixel propagation from past/future clean frames with bilateral temporal smoothing to eliminate flicker.
- **Spatial Exemplar Inpainting:** Multi-scale exemplar patch synthesis with boundary priority queue and Laplacian fringe Poisson smoothing.
- **Content-Aware Dynamic Programming Seam Carving:** $O(W \cdot H)$ cumulative gradient energy evaluation for seamless object removal (energy attraction) and aspect-ratio retargeting without squishing protected subjects.
- **Multi-Subject Saliency & Smart Reframe:** Multi-peak spatial clustering with NMS, unified aspect ratio bounding box generation, and automated speaker-switch detection with kinematic deadband smoothing.

---

## 2. System Requirements & Hardware Compatibility

### Windows Specifications:
- **Operating System:** Windows 10 (version 1903 or higher, 64-bit) or Windows 11.
- **Processor:** 64-bit x86_64 CPU with SSE4.2 support (Intel 8th+ Gen, AMD Ryzen).
- **Memory:** 16 GB DDR4/DDR5 RAM recommended for 4K and multi-layer nested workflows.
- **Graphics Card:** DirectX 12 Feature Level 11_0 or Vulkan 1.2 compatible GPU (NVIDIA GTX 900+, AMD Radeon RX 400+, Intel UHD 600+).
- **Audio Output:** WASAPI-compatible stereo output device.
- **External Media Engine:** FFmpeg 6.0+ recommended in system `PATH` or `<app_dir>/bin/`.

### Linux Specifications:
- **Display Server:** Wayland (native fractional scaling) or X11.
- **Graphics:** Vulkan 1.2+ DRI GPU driver (Mesa 23+, NVIDIA 525+).
- **Audio:** PipeWire or PulseAudio low-latency sound server.

### macOS Specifications:
- **Operating System:** macOS 12 (Monterey) or higher (macOS 14+ Sonoma recommended).
- **Architecture:** Universal 2 (Apple Silicon M1/M2/M3/M4 & Intel x86_64).
- **Graphics:** Metal 2.4+ hardware acceleration.

---

## 3. Reporting Issues

To report bugs, regressions, or feature requests:
- **GitHub Issues:** [github.com/athbots/Reddart/issues](https://github.com/athbots/Reddart/issues)
- Include your platform, GPU model, and stdout from `reddart --version`.
