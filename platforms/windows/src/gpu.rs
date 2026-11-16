//! GPU initialization and management for the Windows platform.
//!
//! Creates the wgpu device, surface, and manages GPU resources.

use anyhow::Result;
use std::sync::Arc;

/// GPU context holding wgpu device and queue, with multi-adapter fleet management.
pub struct GpuContext {
    #[allow(dead_code)]
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    #[allow(dead_code)]
    pub multi_gpu: rd_render::MultiGpuManager,
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

        // Discover multi-GPU fleet capabilities
        let multi_gpu =
            rd_render::MultiGpuManager::discover(instance, rd_render::MultiGpuConfig::default());
        log::info!(
            "Multi-GPU fleet initialized with {} adapter(s) (strategy: {:?})",
            multi_gpu.adapter_count(),
            multi_gpu.strategy()
        );

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
            multi_gpu,
        })
    }
}

/// HDR surface capability and configuration metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HdrSurfaceInfo {
    /// Configured swapchain texture format.
    pub format: wgpu::TextureFormat,
    /// Whether high dynamic range (HDR10 or scRGB) format is active.
    pub is_hdr: bool,
}

/// Configure the surface for rendering, optionally enabling HDR display swapchains.
///
/// Probes surface capabilities for wide color gamut and high dynamic range presentation:
/// - `wgpu::TextureFormat::Rgba16Float`: 16-bit floating point scRGB linear HDR swapchain.
/// - `wgpu::TextureFormat::Rgb10a2Unorm`: 10-bit HDR10 PQ / BT.2100 swapchain.
///
/// When `prefer_hdr` is requested and supported by the display adapter, an HDR format is selected;
/// otherwise, the swapchain gracefully falls back to an sRGB standard dynamic range format.
pub fn configure_surface_hdr(
    surface: &wgpu::Surface,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    width: u32,
    height: u32,
    prefer_hdr: bool,
) -> HdrSurfaceInfo {
    let caps = surface.get_capabilities(adapter);

    let (format, is_hdr) = if prefer_hdr {
        if caps.formats.contains(&wgpu::TextureFormat::Rgba16Float) {
            log::info!("HDR display detected: selecting Rgba16Float swapchain");
            (wgpu::TextureFormat::Rgba16Float, true)
        } else if caps.formats.contains(&wgpu::TextureFormat::Rgb10a2Unorm) {
            log::info!("HDR display detected: selecting Rgb10a2Unorm swapchain");
            (wgpu::TextureFormat::Rgb10a2Unorm, true)
        } else {
            let sdr_format = caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(caps.formats[0]);
            log::info!(
                "HDR requested but not supported by display surface; falling back to {:?}",
                sdr_format
            );
            (sdr_format, false)
        }
    } else {
        let sdr_format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        (sdr_format, false)
    };

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
    log::info!(
        "Surface configured: {}x{} {:?} (HDR active: {})",
        width,
        height,
        format,
        is_hdr
    );

    HdrSurfaceInfo { format, is_hdr }
}

/// Configure the surface for rendering.
pub fn configure_surface(
    surface: &wgpu::Surface,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::TextureFormat {
    configure_surface_hdr(surface, adapter, device, width, height, false).format
}
