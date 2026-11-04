//! GPU compositor: compiles and executes the RenderGraph on wgpu.
//!
//! Section 3.4 & Gate 0:
//! - Multi-layer compositing using WGSL shaders.
//! - Result cache: reuses cached intermediate textures by content hash.
//! - Single WGSL shader definitions running portably across backends.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

use crate::cache::ResultCache;
use rd_effects::registry::EffectRegistry;

/// Uniform buffer layout for Brightness / Contrast shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BrightnessContrastUniforms {
    brightness: f32,
    contrast: f32,
    _padding: [f32; 2],
}

/// Uniform buffer layout for Composite Blend shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlendUniforms {
    opacity: f32,
    blend_mode: u32,
    _padding: [f32; 2],
}

/// GPU Compositor engine.
pub struct Compositor {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    cache: ResultCache,
    #[allow(dead_code)]
    effect_registry: EffectRegistry,
    #[allow(dead_code)]
    sampler: wgpu::Sampler,

    // Pipelines
    #[allow(dead_code)]
    bc_pipeline: wgpu::RenderPipeline,
    #[allow(dead_code)]
    bc_bind_group_layout: wgpu::BindGroupLayout,

    #[allow(dead_code)]
    blend_pipeline: wgpu::RenderPipeline,
    #[allow(dead_code)]
    blend_bind_group_layout: wgpu::BindGroupLayout,
}

impl Compositor {
    /// Initialize the GPU compositor on a given wgpu device and queue.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        target_format: wgpu::TextureFormat,
        cache_bytes: usize,
    ) -> Result<Self> {
        let effect_registry = EffectRegistry::new();
        let cache = ResultCache::new(cache_bytes);

        // General texture sampler (bilinear filtering with clamp to edge)
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Compositor Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // 1. Setup Brightness / Contrast pipeline
        let bc_entry = effect_registry
            .get("brightness_contrast")
            .context("brightness_contrast shader missing")?;
        let bc_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Brightness/Contrast Shader"),
            source: wgpu::ShaderSource::Wgsl(bc_entry.wgsl_source.as_str().into()),
        });

        let bc_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("BC Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let bc_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("BC Pipeline Layout"),
            bind_group_layouts: &[&bc_bind_group_layout],
            push_constant_ranges: &[],
        });

        let bc_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BC Pipeline"),
            layout: Some(&bc_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &bc_shader,
                entry_point: Some(&bc_entry.manifest.vertex_entry),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &bc_shader,
                entry_point: Some(&bc_entry.manifest.fragment_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 2. Setup Composite Blend pipeline
        let blend_entry = effect_registry
            .get("composite_blend")
            .context("composite_blend shader missing")?;
        let blend_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blend Shader"),
            source: wgpu::ShaderSource::Wgsl(blend_entry.wgsl_source.as_str().into()),
        });

        let blend_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Blend Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let blend_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Blend Pipeline Layout"),
                bind_group_layouts: &[&blend_bind_group_layout],
                push_constant_ranges: &[],
            });

        let blend_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Blend Pipeline"),
            layout: Some(&blend_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blend_shader,
                entry_point: Some(&blend_entry.manifest.vertex_entry),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blend_shader,
                entry_point: Some(&blend_entry.manifest.fragment_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            cache,
            effect_registry,
            sampler,
            bc_pipeline,
            bc_bind_group_layout,
            blend_pipeline,
            blend_bind_group_layout,
        })
    }

    /// Create an intermediate render target texture.
    pub fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Create a solid color texture (e.g. for background or testing).
    pub fn create_solid_texture(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        color: [u8; 4],
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let (texture, view) = self.create_texture(width, height, format, "Solid Color Texture");
        let data = vec![color; (width * height) as usize];
        let flat_data: &[u8] = bytemuck::cast_slice(&data);

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            flat_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        (texture, view)
    }

    /// Access the result cache.
    pub fn cache(&self) -> &ResultCache {
        &self.cache
    }

    /// Mutable access to the result cache.
    pub fn cache_mut(&mut self) -> &mut ResultCache {
        &mut self.cache
    }

    /// Execute a Brightness / Contrast pass from input_view into output_view.
    pub fn render_brightness_contrast(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        brightness: f32,
        contrast: f32,
    ) {
        use wgpu::util::DeviceExt;

        let uniforms = BrightnessContrastUniforms {
            brightness,
            contrast,
            _padding: [0.0; 2],
        };
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("BC Uniform Buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BC Bind Group"),
            layout: &self.bc_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("BC Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.bc_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Execute a Blend pass compositing overlay_view over base_view into output_view.
    pub fn render_blend(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        base_view: &wgpu::TextureView,
        overlay_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        opacity: f32,
        blend_mode: u32,
    ) {
        use wgpu::util::DeviceExt;

        let uniforms = BlendUniforms {
            opacity,
            blend_mode,
            _padding: [0.0; 2],
        };
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blend Uniform Buffer"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blend Bind Group"),
            layout: &self.blend_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(base_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(overlay_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Blend Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.blend_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
