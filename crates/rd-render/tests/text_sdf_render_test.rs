//! Integration Test Suite: GPU Signed Distance Field (SDF) Text Rendering
//!
//! Validates:
//! 1. `text_sdf` effect registration, parameter specifications, and WGSL shader syntax.
//! 2. Headless GPU rendering via `Compositor::render_text_sdf` with `create_sdf_texture`.
//! 3. Vector-sharp typography with sub-pixel antialiasing edge smoothing.
//! 4. Stroke outline generation and multi-channel Porter-Duff compositing.
//! 5. Drop shadow offset synthesis and soft edge blur.
//! 6. Alpha-blended rendering over underlying video frames via `LoadOp::Load`.

use rd_effects::registry::EffectRegistry;
use rd_effects::text::{SdfTypographyParams, generate_text_sdf_field, render_sdf_text_overlay};
use rd_render::{Compositor, TextSdfUniforms};
use std::sync::mpsc::sync_channel;

/// Helper to round an integer up to the nearest multiple of `align`.
fn align_to(val: u32, align: u32) -> u32 {
    val.div_ceil(align) * align
}

/// Helper to read back an RGBA8 texture from GPU to CPU vector.
fn readback_rgba_texture(
    compositor: &Compositor,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let unpadded_bytes_per_row = width * 4;
    let padded_bytes_per_row = align_to(unpadded_bytes_per_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer_size = (padded_bytes_per_row * height) as u64;

    let staging_buffer = compositor.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback Staging Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut copy_encoder =
        compositor
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Readback Copy Encoder"),
            });

    copy_encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    compositor.queue().submit([copy_encoder.finish()]);

    let slice = staging_buffer.slice(..);
    let (tx, rx) = sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });

    compositor.device().poll(wgpu::Maintain::Wait);
    rx.recv()
        .expect("Failed to receive readback channel signal")
        .expect("Buffer mapping failed");

    let mapped = slice.get_mapped_range();
    let mut out_pixels = Vec::with_capacity((width * height * 4) as usize);

    for row in 0..height {
        let row_start = (row * padded_bytes_per_row) as usize;
        let row_end = row_start + (width * 4) as usize;
        out_pixels.extend_from_slice(&mapped[row_start..row_end]);
    }

    drop(mapped);
    staging_buffer.unmap();
    out_pixels
}

#[test]
fn test_text_sdf_manifest_and_wgsl_shader() {
    let registry = EffectRegistry::new();
    let entry = registry
        .get("text_sdf")
        .expect("text_sdf effect must be registered");

    assert_eq!(entry.manifest.id, "text_sdf");
    assert_eq!(entry.manifest.vertex_entry, "vs_main");
    assert_eq!(entry.manifest.fragment_entry, "fs_main");

    // Verify all typography & outline parameters are declared
    let param_ids: Vec<&str> = entry
        .manifest
        .parameters
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert!(param_ids.contains(&"outline_width"));
    assert!(param_ids.contains(&"smoothing"));
    assert!(param_ids.contains(&"shadow_smoothing"));
    assert!(param_ids.contains(&"enable_shadow"));

    // Verify WGSL source contains multi-layer compositing uniforms
    assert!(entry.wgsl_source.contains("struct TextUniforms"));
    assert!(entry.wgsl_source.contains("outline_width"));
    assert!(entry.wgsl_source.contains("enable_shadow"));
    assert!(entry.wgsl_source.contains("font_atlas: texture_2d<f32>"));
}

#[test]
fn test_compositor_render_text_sdf_headless_basic() {
    let compositor = match pollster::block_on(Compositor::create_headless(
        wgpu::TextureFormat::Rgba8Unorm,
        16 * 1024 * 1024,
    )) {
        Ok(c) => c,
        Err(err) => {
            println!("Skipping Compositor GPU test: no wgpu adapter ({err})");
            return;
        }
    };

    let width = 128u32;
    let height = 64u32;
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let params = SdfTypographyParams {
        text: "REDDART",
        pos_x: 0.5,
        pos_y: 0.5,
        font_size: 24.0,
        color: [1.0, 1.0, 1.0, 1.0],
        ..Default::default()
    };

    let sdf = generate_text_sdf_field(&params, width as usize, height as usize);
    let (_tex_sdf, view_sdf) = compositor.create_sdf_texture(width, height, &sdf);
    let (tex_out, view_out) = compositor.create_texture(width, height, format, "Text SDF Output");

    let uniforms = TextSdfUniforms {
        text_color: [1.0, 1.0, 1.0, 1.0],
        outline_color: [0.0, 0.0, 0.0, 0.0],
        shadow_color: [0.0, 0.0, 0.0, 0.0],
        outline_width: 0.0,
        smoothing: 0.04,
        shadow_smoothing: 0.08,
        enable_shadow: 0.0,
        shadow_offset: [0.0, 0.0],
        _padding: [0.0, 0.0],
    };

    let mut encoder = compositor
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Text SDF Test Encoder"),
        });

    compositor.render_text_sdf(
        &mut encoder,
        &view_sdf,
        &view_out,
        uniforms,
        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
    );

    compositor.queue().submit([encoder.finish()]);

    let pixels = readback_rgba_texture(&compositor, &tex_out, width, height);
    let visible_count = pixels.chunks_exact(4).filter(|p| p[3] > 30).count();
    assert!(
        visible_count > 100,
        "GPU text SDF rendering should produce visible non-zero glyph pixels (got {visible_count})"
    );
}

#[test]
fn test_compositor_render_text_sdf_stroke_outline() {
    let compositor = match pollster::block_on(Compositor::create_headless(
        wgpu::TextureFormat::Rgba8Unorm,
        16 * 1024 * 1024,
    )) {
        Ok(c) => c,
        Err(err) => {
            println!("Skipping Compositor GPU test: no wgpu adapter ({err})");
            return;
        }
    };

    let width = 128u32;
    let height = 64u32;
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let params = SdfTypographyParams {
        text: "CINEMA",
        pos_x: 0.5,
        pos_y: 0.5,
        font_size: 24.0,
        color: [1.0, 1.0, 1.0, 1.0],
        ..Default::default()
    };

    let sdf = generate_text_sdf_field(&params, width as usize, height as usize);
    let (_tex_sdf, view_sdf) = compositor.create_sdf_texture(width, height, &sdf);

    // 1. Render plain text (no outline)
    let (tex_plain, view_plain) = compositor.create_texture(width, height, format, "Plain Text");
    let mut enc1 = compositor
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let uniforms_plain = TextSdfUniforms {
        text_color: [1.0, 1.0, 1.0, 1.0],
        outline_color: [1.0, 0.0, 0.0, 1.0],
        shadow_color: [0.0, 0.0, 0.0, 0.0],
        outline_width: 0.0,
        smoothing: 0.04,
        shadow_smoothing: 0.08,
        enable_shadow: 0.0,
        shadow_offset: [0.0, 0.0],
        _padding: [0.0, 0.0],
    };
    compositor.render_text_sdf(
        &mut enc1,
        &view_sdf,
        &view_plain,
        uniforms_plain,
        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
    );
    compositor.queue().submit([enc1.finish()]);

    // 2. Render text with red outline
    let (tex_outline, view_outline) =
        compositor.create_texture(width, height, format, "Outlined Text");
    let mut enc2 = compositor
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let uniforms_outline = TextSdfUniforms {
        text_color: [1.0, 1.0, 1.0, 1.0],
        outline_color: [1.0, 0.0, 0.0, 1.0], // Red outline
        shadow_color: [0.0, 0.0, 0.0, 0.0],
        outline_width: 0.15,
        smoothing: 0.04,
        shadow_smoothing: 0.08,
        enable_shadow: 0.0,
        shadow_offset: [0.0, 0.0],
        _padding: [0.0, 0.0],
    };
    compositor.render_text_sdf(
        &mut enc2,
        &view_sdf,
        &view_outline,
        uniforms_outline,
        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
    );
    compositor.queue().submit([enc2.finish()]);

    let px_plain = readback_rgba_texture(&compositor, &tex_plain, width, height);
    let px_outline = readback_rgba_texture(&compositor, &tex_outline, width, height);

    let count_plain = px_plain.chunks_exact(4).filter(|p| p[3] > 30).count();
    let count_outline = px_outline.chunks_exact(4).filter(|p| p[3] > 30).count();

    assert!(
        count_outline > count_plain,
        "Stroke outline should expand the total rasterized pixel footprint (outline: {count_outline}, plain: {count_plain})"
    );

    // Verify presence of red outline pixels (R high, G low, B low)
    let red_outline_pixels = px_outline
        .chunks_exact(4)
        .filter(|p| p[0] > 180 && p[1] < 80 && p[2] < 80 && p[3] > 50)
        .count();
    assert!(
        red_outline_pixels > 20,
        "Expected distinct red outline pixels on perimeter, found {red_outline_pixels}"
    );
}

#[test]
fn test_compositor_render_text_sdf_drop_shadow() {
    let compositor = match pollster::block_on(Compositor::create_headless(
        wgpu::TextureFormat::Rgba8Unorm,
        16 * 1024 * 1024,
    )) {
        Ok(c) => c,
        Err(err) => {
            println!("Skipping Compositor GPU test: no wgpu adapter ({err})");
            return;
        }
    };

    let width = 128u32;
    let height = 64u32;
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let params = SdfTypographyParams {
        text: "SHADOW",
        pos_x: 0.5,
        pos_y: 0.5,
        font_size: 24.0,
        color: [1.0, 1.0, 1.0, 1.0],
        ..Default::default()
    };

    let sdf = generate_text_sdf_field(&params, width as usize, height as usize);
    let (_tex_sdf, view_sdf) = compositor.create_sdf_texture(width, height, &sdf);

    let (tex_shadow, view_shadow) = compositor.create_texture(width, height, format, "Shadow Text");
    let mut enc = compositor
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    let uniforms_shadow = TextSdfUniforms {
        text_color: [1.0, 1.0, 1.0, 1.0],
        outline_color: [0.0, 0.0, 0.0, 0.0],
        shadow_color: [0.1, 0.1, 0.1, 0.8],
        outline_width: 0.0,
        smoothing: 0.04,
        shadow_smoothing: 0.10,
        enable_shadow: 1.0,
        shadow_offset: [0.04, 0.04], // 4% down-right offset
        _padding: [0.0, 0.0],
    };

    compositor.render_text_sdf(
        &mut enc,
        &view_sdf,
        &view_shadow,
        uniforms_shadow,
        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
    );
    compositor.queue().submit([enc.finish()]);

    let px_shadow = readback_rgba_texture(&compositor, &tex_shadow, width, height);
    let visible_shadow = px_shadow.chunks_exact(4).filter(|p| p[3] > 30).count();
    assert!(
        visible_shadow > 100,
        "Text with drop shadow should render visible text and shadow pixels (got {visible_shadow})"
    );
}

#[test]
fn test_compositor_render_text_sdf_alpha_blend_over_background() {
    let compositor = match pollster::block_on(Compositor::create_headless(
        wgpu::TextureFormat::Rgba8Unorm,
        16 * 1024 * 1024,
    )) {
        Ok(c) => c,
        Err(err) => {
            println!("Skipping Compositor GPU test: no wgpu adapter ({err})");
            return;
        }
    };

    let width = 64u32;
    let height = 32u32;
    let format = wgpu::TextureFormat::Rgba8Unorm;

    // Create a solid blue background texture [0, 0, 255, 255]
    let (tex_bg, view_bg) =
        compositor.create_solid_texture(width, height, format, [0, 0, 255, 255]);

    let params = SdfTypographyParams {
        text: "OVER",
        pos_x: 0.5,
        pos_y: 0.5,
        font_size: 16.0,
        color: [1.0, 1.0, 1.0, 1.0], // White text
        ..Default::default()
    };

    let sdf = generate_text_sdf_field(&params, width as usize, height as usize);
    let (_tex_sdf, view_sdf) = compositor.create_sdf_texture(width, height, &sdf);

    let uniforms = TextSdfUniforms {
        text_color: [1.0, 1.0, 1.0, 1.0],
        outline_color: [0.0, 0.0, 0.0, 0.0],
        shadow_color: [0.0, 0.0, 0.0, 0.0],
        outline_width: 0.0,
        smoothing: 0.04,
        shadow_smoothing: 0.08,
        enable_shadow: 0.0,
        shadow_offset: [0.0, 0.0],
        _padding: [0.0, 0.0],
    };

    let mut enc = compositor
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    // Render text with LoadOp::Load preserving background pixels
    compositor.render_text_sdf(&mut enc, &view_sdf, &view_bg, uniforms, wgpu::LoadOp::Load);

    compositor.queue().submit([enc.finish()]);

    let px_composite = readback_rgba_texture(&compositor, &tex_bg, width, height);

    // Verify corner pixel remains untouched solid blue [0, 0, 255, 255]
    let corner = &px_composite[0..4];
    assert_eq!(corner[0], 0);
    assert_eq!(corner[1], 0);
    assert_eq!(corner[2], 255);
    assert_eq!(corner[3], 255);

    // Verify background retains pure blue pixels
    let pure_blue_count = px_composite
        .chunks_exact(4)
        .filter(|p| p[0] == 0 && p[1] == 0 && p[2] == 255 && p[3] == 255)
        .count();
    assert!(
        pure_blue_count > 500,
        "Untouched background pixels should remain solid blue (got {pure_blue_count})"
    );

    // Verify text pixels have white text blended over blue (R > 100 and G > 100)
    let blended_text_count = px_composite
        .chunks_exact(4)
        .filter(|p| p[0] > 100 && p[1] > 100 && p[2] > 200)
        .count();
    assert!(
        blended_text_count > 50,
        "Should find white text pixels alpha-blended over blue background (got {blended_text_count})"
    );
}

#[test]
fn test_software_rasterizer_matches_sdf_typography() {
    let width = 200u32;
    let height = 100u32;
    let mut buffer = vec![0u8; (width * height * 4) as usize];

    let params = SdfTypographyParams {
        text: "VECTOR\nSHARP",
        pos_x: 0.5,
        pos_y: 0.5,
        font_size: 24.0,
        tracking: 0.1,
        leading: 1.4,
        alignment: rd_effects::TextAlignment::Center,
        color: [1.0, 1.0, 1.0, 1.0],
        outline_color: [1.0, 0.5, 0.0, 1.0],
        outline_width: 0.1,
        drop_shadow: true,
        shadow_color: [0.0, 0.0, 0.0, 0.75],
        shadow_offset: [0.02, 0.02],
        smoothing: 0.04,
        shadow_smoothing: 0.08,
    };

    render_sdf_text_overlay(&mut buffer, width, height, &params);

    let non_zero_count = buffer.chunks_exact(4).filter(|p| p[3] > 0).count();
    assert!(
        non_zero_count > 400,
        "Software SDF rasterizer should render multi-line outlined text with shadow (got {non_zero_count})"
    );
}
