# GPU Signed Distance Field (SDF) Text Rendering

## 1. Architectural Overview

Reddart implements a vector-sharp, resolution-independent text and title overlay rendering architecture driven by GPU Signed Distance Fields (SDF). Traditional bitmap rasterization exhibits blurring or blocky pixel artifacts when scaled, zoomed, or displayed across varying canvas resolutions (e.g., 1080p proxy preview upscaled to 4K UHD export). By encoding glyph shapes into continuous 2D distance fields, the GPU fragment shader evaluates analytic edge thresholds, producing crisp edges, custom stroke outlines, and soft drop shadows in a single compositing pass.

```
+-----------------------------------------------------------------------+
| TextClip / SdfTypographyParams                                        |
| (text, font_size, tracking, leading, outline_width, drop_shadow, etc) |
+-----------------------------------------------------------------------+
                                  |
                                  v
+-----------------------------------------------------------------------+
| SdfTextLayout & Multi-Line Character Placement                        |
| Computes sub-pixel char advance, line spacing, and alignment offsets  |
+-----------------------------------------------------------------------+
                                  |
                                  v
+-----------------------------------------------------------------------+
| 2D Euclidean Distance Transform (Felzenszwalb & Huttenlocher O(N))    |
| Computes exact Euclidean distance from binary glyph mask              |
+-----------------------------------------------------------------------+
                                  |
                                  v
+-----------------------------------------------------------------------+
| Single-Channel R8Unorm SDF Texture Upload                             |
| 0.5 = contour boundary; >0.5 = interior core; <0.5 = exterior         |
+-----------------------------------------------------------------------+
                                  |
                                  v
+-----------------------------------------------------------------------+
| WGSL Fragment Shader (effects/text_sdf/shader.wgsl)                   |
| Evaluates smoothstep antialiasing, stroke outline, and drop shadow    |
+-----------------------------------------------------------------------+
                                  |
                                  v
+-----------------------------------------------------------------------+
| Compositor Multi-Track Alpha Blending (wgpu::BlendState)              |
| Porter-Duff Over composite onto video frames with zero loss of detail |
+-----------------------------------------------------------------------+
```

---

## 2. Mathematical Foundations

### 2.1 Euclidean Distance Transform (EDT)
To convert discrete glyph rasters into smooth distance maps without costly brute-force searches, Reddart implements the separable linear-time algorithm developed by Felzenszwalb and Huttenlocher.

Given a 1D grid $f[p]$ where $f[p] = 0$ for boundary pixels and $\infty$ otherwise, the squared distance transform $D[q]$ is defined as:
$$D[q] = \min_{p} \left((q - p)^2 + f[p]\right)$$

The algorithm computes the lower envelope of $n$ intersecting parabolas in $O(n)$ time using at most $2n$ comparisons. The 2D distance transform is then achieved by evaluating 1D transforms along the columns, followed by 1D transforms along the rows of the intermediate matrix:
$$D_{2D}(x, y) = \sqrt{D_{row}(x, D_{col}(y))}$$

### 2.2 Signed Distance Normalization
For any pixel $(x, y)$, the signed distance $d(x, y)$ is:
- $d(x, y) = +d_{in}(x, y)$ if $(x, y)$ is inside the glyph (distance to nearest exterior pixel).
- $d(x, y) = -d_{out}(x, y)$ if $(x, y)$ is outside the glyph (distance to nearest interior pixel).

The distance value is mapped to a normalized floating-point range $[0.0, 1.0]$ over a spread radius $R$:
$$\text{SDF}(x, y) = \text{clamp}\left(0.5 + \frac{d(x, y)}{2R}, 0.0, 1.0\right)$$
- $\text{SDF} = 0.5$: Exact mathematical glyph contour.
- $\text{SDF} > 0.5$: Core interior.
- $\text{SDF} < 0.5$: Exterior halo.

---

## 3. WGSL Multi-Layer Shader Architecture

The shader (`effects/text_sdf/shader.wgsl`) executes sub-pixel antialiasing and composites three visual layers back-to-front in a single render pass:

```wgsl
struct TextUniforms {
    text_color: vec4<f32>,
    outline_color: vec4<f32>,
    shadow_color: vec4<f32>,
    outline_width: f32,       // 0.0 = no outline, up to 0.5 in distance space
    smoothing: f32,           // Edge antialiasing smoothing width (0.02..0.05)
    shadow_smoothing: f32,    // Shadow edge softness (0.05..0.15)
    enable_shadow: f32,       // 1.0 = enabled, 0.0 = disabled
    shadow_offset: vec2<f32>, // Offset in UV texture space
    _padding: vec2<f32>,      // Standard 16-byte buffer alignment
}
```

### Layer Synthesis:
1. **Core Text Alpha:**
   $$\alpha_{text} = \text{smoothstep}(0.5 - s, 0.5 + s, \text{dist}) \cdot C_{text.a}$$
2. **Stroke Outline Alpha:**
   $$\theta_{outline} = 0.5 - w_{outline}$$
   $$\alpha_{outline} = \max\left(0, \text{smoothstep}(\theta_{outline} - s, \theta_{outline} + s, \text{dist}) - \frac{\alpha_{text}}{C_{text.a}}\right) \cdot C_{outline.a}$$
3. **Drop Shadow Alpha:**
   Sampled at shifted UV coordinates $\mathbf{u}_{shadow} = \mathbf{u} - \mathbf{offset}_{shadow}$:
   $$\alpha_{shadow} = \text{smoothstep}(\theta_{outline} - s_{shadow}, \theta_{outline} + s_{shadow}, \text{dist}_{shadow}) \cdot C_{shadow.a}$$
4. **Porter-Duff Composition:**
   $$\mathbf{C}_{final} = \mathbf{C}_{shadow} \to \mathbf{C}_{outline} \to \mathbf{C}_{text}$$

---

## 4. Typography Controls

Reddart's `TextClip` and `SdfTypographyParams` expose professional typography adjustments:
- **Tracking (Letter Spacing):** Expressed in normalized em units ($0.0 = \text{standard}$, $0.1 = +10\%$ expansion). Advance distance between characters scales dynamically:
  $$\Delta x = \text{char\_w} \cdot (1.0 + \text{tracking})$$
- **Leading (Line Spacing):** Multiplier for line heights ($1.0 = \text{tight}$, $1.35 = \text{standard broadcast}$, $1.8 = \text{cinematic title}$).
- **Stroke Outlines:** Direct control over outline width and 32-bit RGBA color.
- **Drop Shadows:** Independent UV offsets ($dx, dy$), softness blur radius, and opacity.
- **Alignment:** Left, Center, and Right alignment across multi-line text blocks.

---

## 5. Verification Suite

The GPU SDF text pipeline is verified by automated test suites in `rd-effects` and `rd-render`:

| Test Case | Scope | Result |
| :--- | :--- | :--- |
| `test_edt_1d_and_2d_euclidean` | Exact mathematical distance transform verification | PASS |
| `test_compute_signed_distance_field_gradient` | Verifies contour threshold at 0.5 and monotonic gradients | PASS |
| `test_sdf_typography_tracking_and_leading` | Verifies bounding span expands under positive tracking | PASS |
| `test_sdf_stroke_outline_and_drop_shadow_raster` | Verifies pixel footprint expansion with stroke and shadow | PASS |
| `test_text_sdf_manifest_and_wgsl_shader` | Confirms effect manifest and WGSL compilation | PASS |
| `test_compositor_render_text_sdf_headless_basic` | Dispatches headless GPU SDF render and checks readback | PASS |
| `test_compositor_render_text_sdf_stroke_outline` | Validates GPU outline generation and perimeter pixel colors | PASS |
| `test_compositor_render_text_sdf_drop_shadow` | Validates GPU shadow displacement and edge softening | PASS |
| `test_compositor_render_text_sdf_alpha_blend_over_background` | Validates non-destructive alpha blending over video frames | PASS |
| `test_software_rasterizer_matches_sdf_typography` | Validates CPU software fallback for zero-dependency operation | PASS |
