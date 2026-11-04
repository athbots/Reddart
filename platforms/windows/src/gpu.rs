//! GPU initialization and management for the Windows platform.
//!
//! Creates the wgpu device, surface, and manages GPU resources.

use anyhow::Result;
use std::sync::Arc;

/// GPU context holding wgpu device and queue.
pub struct GpuContext {
    #[allow(dead_code)]
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

impl GpuContext {
    /// Create a new GPU context, selecting the best available backend.
    pub async fn new(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self> {
        // Request adapter — prefer high-performance GPU
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to find a suitable GPU adapter"))?;

        let adapter_info = adapter.get_info();
        log::info!(
            "GPU Adapter: {} ({:?})",
            adapter_info.name,
            adapter_info.backend
        );
        log::info!("  Driver: {}", adapter_info.driver);

        // Request device with features needed for video editing
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Reddart Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await?;

        log::info!("GPU device created successfully");

        Ok(Self {
            instance: instance.clone(),
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
        })
    }
}

/// Configure the surface for rendering.
pub fn configure_surface(
    surface: &wgpu::Surface,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::TextureFormat {
    let caps = surface.get_capabilities(adapter);
    let format = caps
        .formats
        .iter()
        .find(|f| f.is_srgb())
        .copied()
        .unwrap_or(caps.formats[0]);

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };

    surface.configure(device, &config);
    log::info!("Surface configured: {}x{} {:?}", width, height, format);

    format
}
