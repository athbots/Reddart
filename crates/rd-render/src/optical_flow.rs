//! Dense Optical Flow Estimation for Motion-Compensated Retiming & Slow Motion.
//!
//! Technical Proposal Section W4.1 & Post-v1.0 Initiative 7:
//! - Multi-scale hierarchical pyramidal optical flow solver (Coarse-to-fine Horn-Schunck with Sobel gradients).
//! - Forward flow field (Frame A -> Frame B) and Backward flow field (Frame B -> Frame A).
//! - Bidirectional forward-backward consistency checking for occlusion and boundary discontinuity detection.
//!
//! GPU Acceleration (Phase II):
//! - `GpuOpticalFlowEstimator` dispatches Horn-Schunck relaxation on a wgpu compute pipeline.
//! - CPU fallback: `DenseOpticalFlowEstimator` is preserved as a software rasterizer path.
//! - Both produce a packed RGBA flow texture (RG=forward u,v; BA=backward u,v) in [0,1] encoding.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Dense 2D motion vector field representing per-pixel translation vectors $(\Delta x, \Delta y)$.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpticalFlowField {
    pub width: usize,
    pub height: usize,
    /// Horizontal pixel displacements $u = \Delta x$.
    pub u: Vec<f32>,
    /// Vertical pixel displacements $v = \Delta y$.
    pub v: Vec<f32>,
}

impl OpticalFlowField {
    /// Create a zero-initialized motion field of dimensions `width x height`.
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            u: vec![0.0; size],
            v: vec![0.0; size],
        }
    }

    /// Retrieve displacement vector at integer coordinates $(x, y)$.
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> (f32, f32) {
        let cx = x.min(self.width.saturating_sub(1));
        let cy = y.min(self.height.saturating_sub(1));
        let idx = cy * self.width + cx;
        (self.u[idx], self.v[idx])
    }

    /// Set displacement vector at integer coordinates $(x, y)$.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, u: f32, v: f32) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.u[idx] = u;
            self.v[idx] = v;
        }
    }

    /// Sub-pixel bilinear sampling of displacement vectors.
    pub fn sample_bilinear(&self, x: f32, y: f32) -> (f32, f32) {
        let max_x = (self.width.saturating_sub(1)) as f32;
        let max_y = (self.height.saturating_sub(1)) as f32;

        let cx = x.clamp(0.0, max_x);
        let cy = y.clamp(0.0, max_y);

        let x0 = cx.floor() as usize;
        let y0 = cy.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let fx = cx - x0 as f32;
        let fy = cy - y0 as f32;

        let (u00, v00) = self.get(x0, y0);
        let (u10, v10) = self.get(x1, y0);
        let (u01, v01) = self.get(x0, y1);
        let (u11, v11) = self.get(x1, y1);

        let u_top = u00 * (1.0 - fx) + u10 * fx;
        let u_bot = u01 * (1.0 - fx) + u11 * fx;
        let u_interp = u_top * (1.0 - fy) + u_bot * fy;

        let v_top = v00 * (1.0 - fx) + v10 * fx;
        let v_bot = v01 * (1.0 - fx) + v11 * fx;
        let v_interp = v_top * (1.0 - fy) + v_bot * fy;

        (u_interp, v_interp)
    }

    /// Compute motion vector magnitude $\|\vec{v}\| = \sqrt{u^2 + v^2}$ at $(x, y)$.
    pub fn magnitude(&self, x: usize, y: usize) -> f32 {
        let (u, v) = self.get(x, y);
        (u * u + v * v).sqrt()
    }

    /// Calculate average displacement magnitude across the entire frame.
    pub fn mean_magnitude(&self) -> f32 {
        if self.u.is_empty() {
            return 0.0;
        }
        let sum: f32 = self
            .u
            .iter()
            .zip(self.v.iter())
            .map(|(&u, &v)| (u * u + v * v).sqrt())
            .sum();
        sum / self.u.len() as f32
    }

    /// Upsample motion field by a factor of 2x using bilinear interpolation with displacement doubling.
    pub fn upsample_2x(&self, dst_w: usize, dst_h: usize) -> Self {
        let mut up = Self::new(dst_w, dst_h);
        let scale_x = self.width as f32 / dst_w as f32;
        let scale_y = self.height as f32 / dst_h as f32;

        let up_scale_x = dst_w as f32 / self.width as f32;
        let up_scale_y = dst_h as f32 / self.height as f32;

        for dy in 0..dst_h {
            let sy = (dy as f32 + 0.5) * scale_y - 0.5;
            for dx in 0..dst_w {
                let sx = (dx as f32 + 0.5) * scale_x - 0.5;
                let (u, v) = self.sample_bilinear(sx, sy);
                up.set(dx, dy, u * up_scale_x, v * up_scale_y);
            }
        }
        up
    }
}

/// Configuration parameters for the multi-scale dense optical flow estimator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalFlowConfig {
    /// Number of image pyramid levels (default: 3).
    pub pyramid_levels: usize,
    /// Number of relaxation iterations per pyramid level (default: 10).
    pub iterations: usize,
    /// Smoothness regularization weight $\alpha^2$ (default: 0.02).
    pub alpha_sq: f32,
    /// Forward-backward consistency threshold in pixels for occlusion detection (default: 1.5).
    pub occlusion_threshold: f32,
}

impl Default for OpticalFlowConfig {
    fn default() -> Self {
        Self {
            pyramid_levels: 3,
            iterations: 10,
            alpha_sq: 0.02,
            occlusion_threshold: 1.5,
        }
    }
}

/// Cinema-grade Dense Optical Flow Estimator.
pub struct DenseOpticalFlowEstimator {
    pub config: OpticalFlowConfig,
}

impl DenseOpticalFlowEstimator {
    pub fn new(config: OpticalFlowConfig) -> Self {
        Self { config }
    }

    /// Estimate forward and backward flow fields between Frame A and Frame B.
    ///
    /// Returns `(flow_forward, flow_backward, occlusion_mask)` where:
    /// - `flow_forward` maps Frame A -> Frame B.
    /// - `flow_backward` maps Frame B -> Frame A.
    /// - `occlusion_mask` is a `Vec<f32>` in `[0.0, 1.0]` (1.0 = occluded/disoccluded, 0.0 = reliable match).
    pub fn compute_bidirectional_flow(
        &self,
        frame_a: &[u8],
        frame_b: &[u8],
        width: usize,
        height: usize,
    ) -> (OpticalFlowField, OpticalFlowField, Vec<f32>) {
        let lum_a = rgba_to_luminance(frame_a, width, height);
        let lum_b = rgba_to_luminance(frame_b, width, height);

        let flow_forward = self.compute_flow_pyramid(&lum_a, &lum_b, width, height);
        let flow_backward = self.compute_flow_pyramid(&lum_b, &lum_a, width, height);

        let occlusion_mask =
            self.compute_occlusion_mask(&flow_forward, &flow_backward, width, height);

        (flow_forward, flow_backward, occlusion_mask)
    }

    /// Hierarchical coarse-to-fine optical flow calculation using image pyramids.
    fn compute_flow_pyramid(
        &self,
        lum_a: &[f32],
        lum_b: &[f32],
        width: usize,
        height: usize,
    ) -> OpticalFlowField {
        let levels = self.config.pyramid_levels.max(1);

        // Build pyramids from finest (0) to coarsest (levels - 1)
        let mut pyr_a = Vec::with_capacity(levels);
        let mut pyr_b = Vec::with_capacity(levels);
        let mut dims = Vec::with_capacity(levels);

        pyr_a.push(lum_a.to_vec());
        pyr_b.push(lum_b.to_vec());
        dims.push((width, height));

        for l in 1..levels {
            let (prev_w, prev_h) = dims[l - 1];
            let next_w = (prev_w / 2).max(1);
            let next_h = (prev_h / 2).max(1);
            let down_a = downsample_2x(&pyr_a[l - 1], prev_w, prev_h, next_w, next_h);
            let down_b = downsample_2x(&pyr_b[l - 1], prev_w, prev_h, next_w, next_h);
            pyr_a.push(down_a);
            pyr_b.push(down_b);
            dims.push((next_w, next_h));
        }

        // Initialize coarsest flow field
        let (coarsest_w, coarsest_h) = dims[levels - 1];
        let mut current_flow = OpticalFlowField::new(coarsest_w, coarsest_h);

        // Coarse-to-fine relaxation
        for level in (0..levels).rev() {
            let (lvl_w, lvl_h) = dims[level];
            if level < levels - 1 {
                // Upsample previous coarser level
                current_flow = current_flow.upsample_2x(lvl_w, lvl_h);
            }

            self.solve_level(
                &pyr_a[level],
                &pyr_b[level],
                lvl_w,
                lvl_h,
                &mut current_flow,
            );
        }

        current_flow
    }

    /// Solve optical flow relaxation at a single scale level with warped residuals.
    fn solve_level(
        &self,
        img_a: &[f32],
        img_b: &[f32],
        width: usize,
        height: usize,
        flow: &mut OpticalFlowField,
    ) {
        let size = width * height;
        if size == 0 {
            return;
        }

        let u_init = flow.u.clone();
        let v_init = flow.v.clone();

        let mut du = vec![0.0_f32; size];
        let mut dv = vec![0.0_f32; size];

        // 1. Compute spatial gradients on Frame A
        let mut grad_x = vec![0.0_f32; size];
        let mut grad_y = vec![0.0_f32; size];

        for y in 1..height.saturating_sub(1) {
            for x in 1..width.saturating_sub(1) {
                let idx = y * width + x;
                let p00 = img_a[(y - 1) * width + (x - 1)];
                let p20 = img_a[(y - 1) * width + (x + 1)];
                let p01 = img_a[y * width + (x - 1)];
                let p21 = img_a[y * width + (x + 1)];
                let p02 = img_a[(y + 1) * width + (x - 1)];
                let p22 = img_a[(y + 1) * width + (x + 1)];

                let p10 = img_a[(y - 1) * width + x];
                let p12 = img_a[(y + 1) * width + x];

                grad_x[idx] = ((p20 + 2.0 * p21 + p22) - (p00 + 2.0 * p01 + p02)) / 8.0;
                grad_y[idx] = ((p02 + 2.0 * p12 + p22) - (p00 + 2.0 * p10 + p20)) / 8.0;
            }
        }

        // 2. Compute warped temporal difference It = Warped_B(x + u_init, y + v_init) - A(x, y)
        let mut it = vec![0.0_f32; size];
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let wx = x as f32 + u_init[idx];
                let wy = y as f32 + v_init[idx];
                let b_sample = sample_luminance_bilinear(img_b, width, height, wx, wy);
                it[idx] = b_sample - img_a[idx];
            }
        }

        let alpha_sq = self.config.alpha_sq;

        // 3. Iterative Horn-Schunck relaxation on residual flow
        for _ in 0..self.config.iterations {
            let du_old = du.clone();
            let dv_old = dv.clone();

            for y in 1..height.saturating_sub(1) {
                for x in 1..width.saturating_sub(1) {
                    let idx = y * width + x;

                    let du_avg = (du_old[idx - 1]
                        + du_old[idx + 1]
                        + du_old[idx - width]
                        + du_old[idx + width])
                        * 0.25;
                    let dv_avg = (dv_old[idx - 1]
                        + dv_old[idx + 1]
                        + dv_old[idx - width]
                        + dv_old[idx + width])
                        * 0.25;

                    let ix = grad_x[idx];
                    let iy = grad_y[idx];

                    let denom = alpha_sq + ix * ix + iy * iy;
                    let delta = (ix * du_avg + iy * dv_avg + it[idx]) / denom;

                    du[idx] = du_avg - ix * delta;
                    dv[idx] = dv_avg - iy * delta;
                }
            }
        }

        // Add residual to total flow
        for i in 0..size {
            flow.u[i] = u_init[i] + du[i];
            flow.v[i] = v_init[i] + dv[i];
        }
    }

    /// Forward-backward consistency check for motion occlusion detection.
    fn compute_occlusion_mask(
        &self,
        forward: &OpticalFlowField,
        backward: &OpticalFlowField,
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        let mut mask = vec![0.0_f32; width * height];
        let thresh = self.config.occlusion_threshold.max(0.1);

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let (u_fwd, v_fwd) = forward.get(x, y);

                // Target coordinate in Frame B
                let bx = x as f32 + u_fwd;
                let by = y as f32 + v_fwd;

                // Sample backward flow at destination
                let (u_bwd, v_bwd) = backward.sample_bilinear(bx, by);

                // Consistency roundtrip error
                let err_x = u_fwd + u_bwd;
                let err_y = v_fwd + v_bwd;
                let err = (err_x * err_x + err_y * err_y).sqrt();

                if err > thresh {
                    let confidence = ((err - thresh) / thresh).clamp(0.0, 1.0);
                    mask[idx] = confidence;
                }
            }
        }

        mask
    }
}

/// Configuration parameters for hardware-accelerated optical flow estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuOpticalFlowConfig {
    /// Number of Horn-Schunck relaxation iterations (default: 15).
    pub iterations: u32,
    /// Horn-Schunck smoothness parameter alpha (default: 0.15).
    pub alpha: f32,
    /// Maximum displacement clamp in pixels (default: 32.0).
    pub max_displacement: f32,
    /// Forward-backward occlusion threshold in pixels (default: 1.5).
    pub occlusion_threshold: f32,
}

impl Default for GpuOpticalFlowConfig {
    fn default() -> Self {
        Self {
            iterations: 15,
            alpha: 0.15,
            max_displacement: 32.0,
            occlusion_threshold: 1.5,
        }
    }
}

/// Uniform buffer structure matching `HsParams` in `effects/optical_flow/flow_estimate.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuHsParams {
    alpha: f32,
    max_displacement: f32,
    iteration: u32,
    _pad: u32,
}

/// Zero-dependency IEEE 754 half-precision float (f16) decoder.
fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;

    if exp == 0 {
        if mant == 0 {
            f32::from_bits(sign << 31)
        } else {
            let mut m = mant;
            let mut e = 113;
            while (m & 0x400) == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            f32::from_bits((sign << 31) | (e << 23) | (m << 13))
        }
    } else if exp == 31 {
        f32::from_bits((sign << 31) | (0xFF << 23) | (mant << 13))
    } else {
        let new_exp = exp + 112;
        f32::from_bits((sign << 31) | (new_exp << 23) | (mant << 13))
    }
}

/// Zero-dependency IEEE 754 half-precision float (f16) encoder.
fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = (bits >> 31) & 1;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x7FFFFF;

    if exp == 0xFF {
        ((sign << 15) | (0x1F << 10) | (mant >> 13).max(1)) as u16
    } else if exp == 0 {
        (sign << 15) as u16
    } else {
        let new_exp = exp - 127 + 15;
        if new_exp >= 31 {
            ((sign << 15) | (0x1F << 10)) as u16
        } else if new_exp <= 0 {
            0
        } else {
            ((sign << 15) | ((new_exp as u32) << 10) | (mant >> 13)) as u16
        }
    }
}

/// Hardware-Accelerated Dense Optical Flow Estimator using wgpu compute shaders.
pub struct GpuOpticalFlowEstimator {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    config: GpuOpticalFlowConfig,
    fallback: DenseOpticalFlowEstimator,
}

impl GpuOpticalFlowEstimator {
    /// Initialize a GPU optical flow estimator with an existing wgpu Device and Queue.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        config: GpuOpticalFlowConfig,
    ) -> Result<Self> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Horn-Schunck Optical Flow Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(rd_effects::OPTICAL_FLOW_ESTIMATE_WGSL.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Optical Flow Compute BGL"),
            entries: &[
                // binding 0: luma_a (texture_2d<f32>)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 1: luma_b (texture_2d<f32>)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 2: flow_in (texture_2d<f32>)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 3: flow_out (texture_storage_2d<rgba16float, write>)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // binding 4: params (uniform buffer)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Optical Flow Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Optical Flow Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let fallback = DenseOpticalFlowEstimator::new(OpticalFlowConfig {
            pyramid_levels: 3,
            iterations: config.iterations as usize,
            alpha_sq: config.alpha * config.alpha,
            occlusion_threshold: config.occlusion_threshold,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            config,
            fallback,
        })
    }

    /// Attempt to initialize a headless GPU optical flow estimator.
    pub async fn create_headless(config: GpuOpticalFlowConfig) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
        {
            Some(a) => a,
            None => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::None,
                    force_fallback_adapter: true,
                    compatible_surface: None,
                })
                .await
                .context("No available GPU or software fallback adapter found for optical flow")?,
        };

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Headless Optical Flow Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .context("Failed to create wgpu device for optical flow")?;

        Self::new(Arc::new(device), Arc::new(queue), config)
    }

    /// Dispatches the GPU Horn-Schunck compute shader iterations to produce a packed RGBA16Float flow texture.
    ///
    /// Layout:
    /// - R: forward u (normalized [0, 1])
    /// - G: forward v (normalized [0, 1])
    /// - B: backward u (normalized [0, 1])
    /// - A: backward v (normalized [0, 1])
    pub fn estimate_flow_to_texture(
        &self,
        frame_a: &[u8],
        frame_b: &[u8],
        width: usize,
        height: usize,
    ) -> Result<wgpu::Texture> {
        anyhow::ensure!(
            frame_a.len() >= width * height * 4 && frame_b.len() >= width * height * 4,
            "Input frame buffers are smaller than width * height * 4"
        );

        let w = width as u32;
        let h = height as u32;

        // 1. Upload luminance of Frame A and Frame B as R8Unorm textures
        let lum_a_data: Vec<u8> = frame_a
            .chunks_exact(4)
            .take(width * height)
            .map(|px| {
                let r = px[0] as f32;
                let g = px[1] as f32;
                let b = px[2] as f32;
                (0.299 * r + 0.587 * g + 0.114 * b)
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect();

        let lum_b_data: Vec<u8> = frame_b
            .chunks_exact(4)
            .take(width * height)
            .map(|px| {
                let r = px[0] as f32;
                let g = px[1] as f32;
                let b = px[2] as f32;
                (0.299 * r + 0.587 * g + 0.114 * b)
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect();

        let tex_desc_luma = wgpu::TextureDescriptor {
            label: Some("Luma Texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let tex_luma_a = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Luma A"),
            ..tex_desc_luma
        });
        let tex_luma_b = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Luma B"),
            ..tex_desc_luma
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex_luma_a,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &lum_a_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex_luma_b,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &lum_b_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let view_luma_a = tex_luma_a.create_view(&wgpu::TextureViewDescriptor::default());
        let view_luma_b = tex_luma_b.create_view(&wgpu::TextureViewDescriptor::default());

        // 2. Create ping-pong Rgba16Float textures for flow relaxation
        let flow_tex_desc = wgpu::TextureDescriptor {
            label: Some("Optical Flow Relaxation Target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let tex_flow_0 = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Flow Ping"),
            ..flow_tex_desc
        });
        let tex_flow_1 = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Flow Pong"),
            ..flow_tex_desc
        });

        // Initialize tex_flow_0 with 0.5 (representing zero displacement)
        let neutral_f16 = f32_to_f16(0.5);
        let neutral_pixel = [neutral_f16, neutral_f16, neutral_f16, neutral_f16];
        let init_data: Vec<u8> = (0..(width * height))
            .flat_map(|_| bytemuck::bytes_of(&neutral_pixel))
            .copied()
            .collect();

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex_flow_0,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &init_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 8),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let view_flow_0 = tex_flow_0.create_view(&wgpu::TextureViewDescriptor::default());
        let view_flow_1 = tex_flow_1.create_view(&wgpu::TextureViewDescriptor::default());

        // 3. Iterative relaxation via compute pass dispatch
        let iterations = self.config.iterations.max(1);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Optical Flow Relaxation Encoder"),
            });

        let mut ping_is_source = true;
        let workgroups_x = w.div_ceil(8);
        let workgroups_y = h.div_ceil(8);

        for it in 0..iterations {
            let (in_view, out_view) = if ping_is_source {
                (&view_flow_0, &view_flow_1)
            } else {
                (&view_flow_1, &view_flow_0)
            };

            use wgpu::util::DeviceExt;
            let params = GpuHsParams {
                alpha: self.config.alpha,
                max_displacement: self.config.max_displacement,
                iteration: it,
                _pad: 0,
            };
            let param_buf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("HS Uniform Buffer"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HS Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view_luma_a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view_luma_b),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(in_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(out_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: param_buf.as_entire_binding(),
                    },
                ],
            });

            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("HS Iteration Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
            drop(cpass);

            ping_is_source = !ping_is_source;
        }

        self.queue.submit([encoder.finish()]);

        let final_texture = if ping_is_source {
            tex_flow_0
        } else {
            tex_flow_1
        };

        Ok(final_texture)
    }

    /// Compute bidirectional flow fields and occlusion mask using the GPU compute pipeline,
    /// with synchronous readback and automatic fallback to CPU if GPU processing fails.
    pub fn compute_bidirectional_flow(
        &self,
        frame_a: &[u8],
        frame_b: &[u8],
        width: usize,
        height: usize,
    ) -> (OpticalFlowField, OpticalFlowField, Vec<f32>) {
        match self.compute_bidirectional_flow_gpu(frame_a, frame_b, width, height) {
            Ok(res) => res,
            Err(err) => {
                log::warn!(
                    "GPU optical flow estimation failed ({}); falling back to CPU",
                    err
                );
                self.fallback
                    .compute_bidirectional_flow(frame_a, frame_b, width, height)
            }
        }
    }

    /// Internal GPU execution and staging readback.
    fn compute_bidirectional_flow_gpu(
        &self,
        frame_a: &[u8],
        frame_b: &[u8],
        width: usize,
        height: usize,
    ) -> Result<(OpticalFlowField, OpticalFlowField, Vec<f32>)> {
        let final_texture = self.estimate_flow_to_texture(frame_a, frame_b, width, height)?;

        let unpadded_bytes_per_row = width * 8; // 4 half floats = 8 bytes per pixel
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) & !(align - 1);
        let buffer_size = (padded_bytes_per_row * height) as u64;

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Flow Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Flow Readback Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &final_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: Some(height as u32),
                },
            },
            wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit([encoder.finish()]);

        let buffer_slice = staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });

        self.device.poll(wgpu::Maintain::wait()).panic_on_timeout();
        rx.recv()
            .context("Readback channel hung")?
            .context("Readback map failed")?;

        let mut fwd = OpticalFlowField::new(width, height);
        let mut bwd = OpticalFlowField::new(width, height);
        let mut occlusion = vec![0.0_f32; width * height];

        let max_d = self.config.max_displacement;
        let thresh = self.config.occlusion_threshold.max(0.1);

        {
            let mapped = buffer_slice.get_mapped_range();
            for y in 0..height {
                let row_start = y * padded_bytes_per_row;
                let row_bytes = &mapped[row_start..row_start + unpadded_bytes_per_row];

                for x in 0..width {
                    let px_start = x * 8;
                    let h_u_fwd =
                        u16::from_ne_bytes([row_bytes[px_start], row_bytes[px_start + 1]]);
                    let h_v_fwd =
                        u16::from_ne_bytes([row_bytes[px_start + 2], row_bytes[px_start + 3]]);
                    let h_u_bwd =
                        u16::from_ne_bytes([row_bytes[px_start + 4], row_bytes[px_start + 5]]);
                    let h_v_bwd =
                        u16::from_ne_bytes([row_bytes[px_start + 6], row_bytes[px_start + 7]]);

                    let norm_u_fwd = f16_to_f32(h_u_fwd);
                    let norm_v_fwd = f16_to_f32(h_v_fwd);
                    let norm_u_bwd = f16_to_f32(h_u_bwd);
                    let norm_v_bwd = f16_to_f32(h_v_bwd);

                    let u_fwd = (norm_u_fwd * 2.0 - 1.0) * max_d;
                    let v_fwd = (norm_v_fwd * 2.0 - 1.0) * max_d;
                    let u_bwd = (norm_u_bwd * 2.0 - 1.0) * max_d;
                    let v_bwd = (norm_v_bwd * 2.0 - 1.0) * max_d;

                    fwd.set(x, y, u_fwd, v_fwd);
                    bwd.set(x, y, u_bwd, v_bwd);

                    let roundtrip_err = ((u_fwd + u_bwd).powi(2) + (v_fwd + v_bwd).powi(2)).sqrt();
                    let occ = (roundtrip_err / thresh).clamp(0.0, 1.0);
                    occlusion[y * width + x] = occ;
                }
            }
        }

        Ok((fwd, bwd, occlusion))
    }
}

/// Convert RGBA byte slice to normalized single-channel luminance [0.0, 1.0].
fn rgba_to_luminance(rgba: &[u8], width: usize, height: usize) -> Vec<f32> {
    let mut lum = vec![0.0_f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            if idx + 2 < rgba.len() {
                let r = rgba[idx] as f32;
                let g = rgba[idx + 1] as f32;
                let b = rgba[idx + 2] as f32;
                lum[y * width + x] = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
            }
        }
    }
    lum
}

/// Bilinear sampling of luminance buffer with edge clamping.
fn sample_luminance_bilinear(img: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    let max_x = (width.saturating_sub(1)) as f32;
    let max_y = (height.saturating_sub(1)) as f32;

    let cx = x.clamp(0.0, max_x);
    let cy = y.clamp(0.0, max_y);

    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);

    let fx = cx - x0 as f32;
    let fy = cy - y0 as f32;

    let p00 = img[y0 * width + x0];
    let p10 = img[y0 * width + x1];
    let p01 = img[y1 * width + x0];
    let p11 = img[y1 * width + x1];

    let top = p00 * (1.0 - fx) + p10 * fx;
    let bot = p01 * (1.0 - fx) + p11 * fx;
    top * (1.0 - fy) + bot * fy
}

/// Downscale single-channel luminance image by 2x using box filtering.
fn downsample_2x(src: &[f32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<f32> {
    let mut dst = vec![0.0_f32; dst_w * dst_h];
    for dy in 0..dst_h {
        let sy0 = (dy * 2).min(src_h - 1);
        let sy1 = (dy * 2 + 1).min(src_h - 1);
        for dx in 0..dst_w {
            let sx0 = (dx * 2).min(src_w - 1);
            let sx1 = (dx * 2 + 1).min(src_w - 1);

            let val = (src[sy0 * src_w + sx0]
                + src[sy0 * src_w + sx1]
                + src[sy1 * src_w + sx0]
                + src[sy1 * src_w + sx1])
                * 0.25;
            dst[dy * dst_w + dx] = val;
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optical_flow_field_sampling_and_upsample() {
        let mut flow = OpticalFlowField::new(4, 4);
        flow.set(1, 1, 4.0, 2.0);
        assert_eq!(flow.get(1, 1), (4.0, 2.0));

        let sampled = flow.sample_bilinear(1.0, 1.0);
        assert_eq!(sampled, (4.0, 2.0));

        let up = flow.upsample_2x(8, 8);
        assert_eq!(up.width, 8);
        assert_eq!(up.height, 8);
        assert!(up.mean_magnitude() > 0.0);
    }

    #[test]
    fn test_dense_optical_flow_on_horizontal_motion() {
        let width = 64;
        let height = 32;

        // Create Frame A with a bright block at x=10..20
        let mut frame_a = vec![30u8; width * height * 4];
        for y in 8..24 {
            for x in 10..20 {
                let idx = (y * width + x) * 4;
                frame_a[idx] = 220;
                frame_a[idx + 1] = 220;
                frame_a[idx + 2] = 220;
                frame_a[idx + 3] = 255;
            }
        }

        // Create Frame B with the bright block shifted right by 4 pixels to x=14..24
        let mut frame_b = vec![30u8; width * height * 4];
        for y in 8..24 {
            for x in 14..24 {
                let idx = (y * width + x) * 4;
                frame_b[idx] = 220;
                frame_b[idx + 1] = 220;
                frame_b[idx + 2] = 220;
                frame_b[idx + 3] = 255;
            }
        }

        let estimator = DenseOpticalFlowEstimator::new(OpticalFlowConfig {
            pyramid_levels: 3,
            iterations: 15,
            alpha_sq: 0.015,
            occlusion_threshold: 1.5,
        });

        let (fwd, bwd, occlusion) =
            estimator.compute_bidirectional_flow(&frame_a, &frame_b, width, height);

        assert_eq!(fwd.width, width);
        assert_eq!(bwd.width, width);
        assert_eq!(occlusion.len(), width * height);

        // Forward flow inside the moving block should have positive horizontal velocity (moving right)
        let block_center_u = fwd.get(15, 16).0;
        assert!(
            block_center_u > 0.5,
            "Expected rightward flow > 0.5 px, got {}",
            block_center_u
        );

        // Backward flow inside the moved block should have negative horizontal velocity (moving left)
        let bwd_center_u = bwd.get(19, 16).0;
        assert!(
            bwd_center_u < -0.5,
            "Expected leftward backward flow < -0.5 px, got {}",
            bwd_center_u
        );
    }
}
