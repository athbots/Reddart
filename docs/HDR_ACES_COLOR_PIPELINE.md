# HDR & ACES Color Pipeline Architecture

## 1. Executive Summary

Reddart incorporates a professional-grade digital cinema color science pipeline compliant with the Academy Color Encoding System (ACES 1.2 / 1.3) and OpenColorIO (OCIO v2) specification. This document outlines the mathematical models, configuration schema, high-bitdepth quantization algorithms, and Windows HDR display swapchain management.

---

## 2. Mathematical Foundation & Color Spaces

The cinema pipeline operates in 32-bit floating-point linear light internally, guaranteeing zero quantization or clipping errors during multi-layer compositing, color grading, and tone mapping.

```
+--------------------+      +--------------------+      +--------------------+
|  Camera / Source   |      |   Linear Light     |      |  Grading Space     |
|  Non-Linear Log    | ---> |   ACEScg (AP1)     | ---> |  ACEScc / ACEScct  |
|  (e.g., LogC, RAW) |      |   32-bit float     |      |  (ASC CDL Ops)     |
+--------------------+      +--------------------+      +--------------------+
                                                                  |
                                                                  v
+--------------------+      +--------------------+      +--------------------+
|  Display Output    |      |  Target Gamut /    |      |   Linear Light     |
|  (PQ / HLG / sRGB) | <--- |  Tone Mapping      | <--- |   Graded Result    |
|  10-bit / 12-bit   |      |  (ACES Filmic)     |      |   32-bit float     |
+--------------------+      +--------------------+      +--------------------+
```

### 2.1 Color Gamuts & Primaries

Reddart natively supports five standard cinema, broadcast, and archival color spaces:

1. **ACES2065-1 (AP0):** Complete visible spectrum gamut for archival interchange ($D60$ white point).
2. **ACEScg (AP1):** Compact wide gamut enclosing all real-world surface colors and laser projectors, optimized for computer graphics rendering and compositing ($D60$ white point).
3. **Display P3:** Wide color gamut standardized for Apple Retina displays and digital cinema ($D65$ white point).
4. **ITU-R BT.2020 (Rec.2020):** Ultra-wide color gamut for UHD and HDR broadcasting ($D65$ white point).
5. **ITU-R BT.709 / sRGB (Rec.709):** Standard dynamic range broadcast and web standard ($D65$ white point).

Chromatic adaptation between the Academy $D60$ white point ($(0.32168, 0.33767)$) and CIE $D65$ ($(0.3127, 0.3290)$) is computed analytically using the Bradford CAT02 chromatic adaptation transform matrix.

### 2.2 Transfer Functions (EOTF & OETF)

Reddart implements analytical, bidirectional transfer functions:

- **sRGB (IEC 61966-2-1):** Linear toe segment ($L \le 0.0031308$) followed by $2.4$ power curve.
- **Rec.709 (ITU-R BT.709):** Linear segment ($L < 0.018$) followed by $0.45$ gamma.
- **SMPTE ST 2084 PQ (Perceptual Quantizer):** Absolute luminance encoding up to $10,000\text{ nits}$, where diffuse white ($100\text{ nits}$) maps to normalized $1.0$, and peak white maps to $100.0$:
  $$\mu = \left(\frac{\max(V^{1/m_2} - c_1, 0)}{c_2 - c_3 V^{1/m_2}}\right)^{1/m_1}, \quad L = 100 \cdot \mu$$
- **ARIB STD-B67 HLG (Hybrid Log-Gamma):** Scene-referred broadcast HDR with square-root toe and logarithmic shoulder.
- **ACEScc (Academy S-2014-003):** Logarithmic encoding for digital color timing with uniform stops across darks and highlights.
- **ACEScct (Academy S-2016-001):** Quasi-logarithmic encoding with an added linear toe below $L \le 0.0078125$ ($2^{-7}$), matching the tactile response of traditional Cineon and LogC grading tools:
  $$V = \begin{cases} 10.5402377 \cdot L + 0.0729055 & \text{if } L \le 0.0078125 \\ \frac{\log_2(L) + 9.72}{17.52} & \text{if } L > 0.0078125 \end{cases}$$

---

## 3. OpenColorIO (OCIO) Configuration Engine

The `rd_effects::ocio` module provides an embedded parser for OpenColorIO v2 YAML configuration profiles (`config.ocio`):

### 3.1 Supported OCIO Directives

- **Roles:** Semantic aliases mapping standard pipeline phases to concrete color spaces (`reference`, `scene_linear`, `color_timing`, `compositing_log`, `rendering`, `data`, `default`).
- **Displays & Views:** Hierarchical mapping of physical output displays (`sRGB`, `Display P3`, `Rec.2020`) and corresponding view transforms (`Standard`, `ACES 1.0 - SDR Video`, `HDR10 - 1000 nits`, `HLG - Broadcast`, `Raw`).
- **ColorSpaces:** Rich definitions including bit-depth (`8i`, `10i`, `12i`, `16f`, `32f`), data tags, transforms (`to_reference`, `from_reference`), and descriptions.
- **YAML Tag Stripping:** Custom YAML tags (`!<ColorSpace>`, `!<View>`, `!<MatrixTransform>`) are parsed and mapped with zero external C++ dependencies.

### 3.2 Reference ACES Configuration (`OcioConfig::aces_default`)

Reddart ships with a built-in reference ACES 1.2 / 1.3 profile, allowing offline and headless rendering without requiring external `.ocio` asset files on disk.

```rust
use rd_effects::ocio::OcioConfig;

// Initialize built-in reference ACES configuration
let config = OcioConfig::aces_default();

// Resolve pipeline from scene-referred linear to HDR10 1000-nit monitoring
let pipeline = config.resolve_pipeline("scene_linear", "Rec.2020", "HDR10 - 1000 nits")?;
```

---

## 4. High-Bitdepth Quantization & Dithering

When presenting 32-bit floating point HDR buffers on physical displays or exporting master files, Reddart supports hardware-optimized packed integer representations:

### 4.1 R10G10B10A2 (10-bit Packed Integer)

Matches DXGI `DXGI_FORMAT_R10G10B10A2_UNORM` and Vulkan/Metal `R10G10B10A2_UNORM`:
- Bits `0..9`: Red ($0..1023$)
- Bits `10..19`: Green ($0..1023$)
- Bits `20..29`: Blue ($0..1023$)
- Bits `30..31`: Alpha ($0..3$)

Function interfaces: `pack_rgb10a2(rgba: [f32; 4]) -> u32` and `unpack_rgb10a2(val: u32) -> [f32; 4]`.

### 4.2 R12G12B12A12 (12-bit Cinema Master)

12-bit unsigned representation normalized over $0..4095$ for Dolby Vision and DCI cinema masters:
Function interfaces: `pack_rgb12(rgba: [f32; 4]) -> [u16; 4]` and `unpack_rgb12(val: [u16; 4]) -> [f32; 4]`.

### 4.3 TPDF (Triangular Probability Density Function) Dithering

Quantizing high-dynamic-range gradients to 10-bit or 8-bit can create subtle contour banding in dark scenes. Reddart implements high-frequency spatial-temporal TPDF blue noise dithering (`triangular_dither_tpdf(x, y, frame)`):
- Generated from the difference of two independent uniform pseudo-random variables $U_1, U_2 \in [0, 1)$, yielding a symmetric triangular probability distribution in $(-1.0, 1.0)$.
- Scaled by exactly $1.0 / \text{quantization\_steps}$ ($1 / 1023$ for 10-bit, $1 / 4095$ for 12-bit), dispersing discrete quantization steps without introducing perceived pattern noise.

---

## 5. Windows HDR Swapchain Architecture

The Windows GPU surface configuration (`platforms/windows/src/gpu.rs`) dynamically detects HDR display capabilities:

```rust
pub fn configure_surface_hdr(
    surface: &wgpu::Surface,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    width: u32,
    height: u32,
    prefer_hdr: bool,
) -> HdrSurfaceInfo
```

1. Probes DXGI / display capabilities via `surface.get_capabilities(adapter)`.
2. When `prefer_hdr` is true:
   - Evaluates availability of `wgpu::TextureFormat::Rgba16Float` (16-bit floating point for scRGB linear HDR swapchains).
   - If unavailable, evaluates `wgpu::TextureFormat::Rgb10a2Unorm` (10-bit HDR10 PQ swapchain).
3. Graceful fallback: If the monitor is SDR or HDR is disabled in Windows Settings, selects the optimal sRGB format (`Bgra8UnormSrgb` or `Rgba8UnormSrgb`).
4. Backward compatibility: Standard `configure_surface` delegates directly to `configure_surface_hdr(..., false).format`.

---

## 6. Verification Suite

The pipeline is verified through automated integration tests in `crates/rd-effects/tests/ocio_hdr_pipeline_test.rs`:

| Test Case | Description | Verification Criterion |
|---|---|---|
| `test_ocio_config_default_aces_hierarchy` | Validates built-in ACES 1.2 configuration hierarchy | Roles, displays, and views resolve correctly |
| `test_ocio_config_from_yaml_roundtrip_and_serialization` | Verifies OCIO YAML parsing and canonical serialization | Bidirectional roundtrip without information loss |
| `test_ocio_pipeline_resolution_end_to_end` | Tests automated pipeline resolution for SDR and HDR | Automatic tone mapping heuristics and color gamuts |
| `test_acescct_toe_curve_vs_acescc` | Evaluates ACEScct toe curve precision | Matches ACEScc in highlights; non-negative shadow lift |
| `test_high_bitdepth_quantization_and_packed_formats` | Tests `R10G10B10A2` and 12-bit packing | Quantization error $\le 1 / (2^b - 1)$ |
| `test_tpdf_dithering_banding_elimination` | Validates TPDF noise distribution on fine HDR ramps | Quantization banding broken into dispersed transitions |
| `test_hdr10_pq_peak_luminance_and_eotf_precision` | SMPTE ST 2084 PQ roundtrip across $10,000\text{ nits}$ | Relative error $< 1.5\%$ over 6 decades of luminance |
| `test_color_pipeline_cdl_in_acescct` | ASC CDL slope, offset, power, saturation in ACEScct | Monotonic contrast and color balance preservation |
