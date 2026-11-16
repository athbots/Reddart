//! Multi-GPU distributed rendering and peer-to-peer memory management.
//!
//! Provides hardware adapter discovery, workload partitioning (spatial tiling,
//! temporal chunking, and compute offload), and seamless multi-GPU frame stitching.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error types occurring during multi-GPU discovery, partitioning, or frame stitching.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MultiGpuError {
    #[error("No GPU adapters available for rendering")]
    NoAdaptersAvailable,

    #[error("Invalid frame dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error(
        "Tile index {tile_index} buffer size mismatch: expected at least {expected} bytes, got {actual}"
    )]
    BufferSizeMismatch {
        tile_index: usize,
        expected: usize,
        actual: usize,
    },

    #[error("Incomplete tile coverage: expected {expected_tiles} tiles, got {actual_tiles}")]
    IncompleteTileCoverage {
        expected_tiles: usize,
        actual_tiles: usize,
    },

    #[error(
        "Tile boundary out of bounds: tile bounds [{x}, {y}, {width}, {height}] exceed frame {frame_width}x{frame_height}"
    )]
    TileOutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        frame_width: u32,
        frame_height: u32,
    },

    #[error("Adapter ID {0} not found in active GPU fleet")]
    AdapterNotFound(usize),
}

/// Hardware interconnect type between adapters in the multi-GPU fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterconnectType {
    /// High-bandwidth direct peer-to-peer bridge (> 200 GB/s NVLink / xGMI).
    NvLink,
    /// Direct PCIe peer-to-peer DMA over root complex switch (> 32 GB/s).
    PcieP2p,
    /// Apple Silicon or APU unified physical system and GPU memory pool.
    UnifiedMemory,
    /// Host pinned system RAM intermediate staging transfer (< 16 GB/s).
    HostPinnedMemory,
}

/// Operational role assigned to a GPU adapter node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuNodeRole {
    /// Primary adapter: drives viewport display swapchain and master DAG scheduling.
    Primary,
    /// Secondary compute offload: executes heavy compute shaders (optical flow, SDF, matting).
    SecondaryCompute,
    /// Spatial or temporal tile worker node.
    TileWorker(usize),
    /// Standby or reserve adapter.
    Standby,
}

/// Profile information and capabilities for an individual GPU adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuAdapterProfile {
    /// Unique identifier within the multi-GPU fleet.
    pub id: usize,
    /// Human-readable adapter name (e.g. "NVIDIA GeForce RTX 4090").
    pub name: String,
    /// Underlying graphics backend (Vulkan, DX12, Metal).
    pub backend: String,
    /// Device classification (DiscreteGpu, IntegratedGpu, Cpu, etc.).
    pub device_type: String,
    /// Installed driver description.
    pub driver: String,
    /// Estimated dedicated video memory in bytes.
    pub vram_bytes: u64,
    /// Interconnect mechanism to other adapters in the system.
    pub interconnect: InterconnectType,
    /// List of peer adapter IDs that this adapter can directly access via P2P DMA.
    pub peer_accessible_ids: Vec<usize>,
    /// Whether this adapter is designated as the primary display device.
    pub is_primary: bool,
    /// Assigned operational role.
    pub role: GpuNodeRole,
}

/// Strategy for distributing rendering workloads across multiple GPUs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadPartitionStrategy {
    /// Single GPU fallback: all tasks executed on primary adapter.
    SingleGpu,
    /// Split-frame spatial tiling: high-resolution canvas divided into horizontal tiles.
    SpatialTiling {
        /// Number of horizontal stripes (typically equal to available discrete GPUs).
        num_tiles: u32,
        /// Halo border width in pixels for neighborhood filtering (blur, convolution).
        halo_pixels: u32,
    },
    /// Alternate frame temporal chunking: sequential frames round-robin scheduled.
    TemporalChunking {
        /// Number of consecutive frames dispatched to each GPU before switching.
        chunk_size: usize,
    },
    /// Dedicated compute offload: raster passes on primary, heavy compute on secondary.
    ComputeOffload,
}

/// Detailed coordinates and dimensions for a spatial rendering tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileDescriptor {
    /// Tile index within the partition set (0 .. num_tiles - 1).
    pub tile_index: usize,
    /// Assigned GPU adapter ID responsible for rendering this tile.
    pub adapter_id: usize,
    /// Target coordinates on final canvas: X.
    pub target_x: u32,
    /// Target coordinates on final canvas: Y.
    pub target_y: u32,
    /// Target dimensions on final canvas: Width.
    pub target_width: u32,
    /// Target dimensions on final canvas: Height.
    pub target_height: u32,
    /// Expanded render rectangle X (including halo).
    pub render_x: u32,
    /// Expanded render rectangle Y (including halo).
    pub render_y: u32,
    /// Expanded render rectangle Width (including halo).
    pub render_width: u32,
    /// Expanded render rectangle Height (including halo).
    pub render_height: u32,
    /// Top halo margin in pixels.
    pub halo_top: u32,
    /// Bottom halo margin in pixels.
    pub halo_bottom: u32,
    /// Left halo margin in pixels.
    pub halo_left: u32,
    /// Right halo margin in pixels.
    pub halo_right: u32,
}

/// Configuration settings for multi-GPU execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiGpuConfig {
    /// Enable multi-adapter distributed rendering. If false, falls back to SingleGpu.
    pub enabled: bool,
    /// Prefer discrete GPUs over integrated GPUs when allocating compute workers.
    pub prefer_discrete: bool,
    /// Active workload partitioning strategy.
    pub strategy: WorkloadPartitionStrategy,
    /// Maximum number of GPUs to incorporate into the active fleet.
    pub max_adapters: usize,
    /// Default halo size in pixels for spatial filter boundary margins.
    pub default_halo_pixels: u32,
}

impl Default for MultiGpuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefer_discrete: true,
            strategy: WorkloadPartitionStrategy::SpatialTiling {
                num_tiles: 2,
                halo_pixels: 16,
            },
            max_adapters: 4,
            default_halo_pixels: 16,
        }
    }
}

/// Orchestrates multi-adapter discovery, role assignment, work dispatch, and frame stitching.
#[derive(Debug, Clone)]
pub struct MultiGpuManager {
    config: MultiGpuConfig,
    adapters: Vec<GpuAdapterProfile>,
    primary_id: usize,
    compute_id: Option<usize>,
}

impl MultiGpuManager {
    /// Discovers physical adapters from a live `wgpu::Instance` and creates a manager.
    pub fn discover(instance: &wgpu::Instance, config: MultiGpuConfig) -> Self {
        let mut profiles = Vec::new();

        // Enumerate physical adapters
        let enumerated = instance.enumerate_adapters(wgpu::Backends::all());
        for (idx, adapter) in enumerated.into_iter().enumerate() {
            if idx >= config.max_adapters {
                break;
            }
            let info = adapter.get_info();
            let is_discrete = info.device_type == wgpu::DeviceType::DiscreteGpu;
            let vram_estimate = if is_discrete {
                // Approximate 8 GB default discrete VRAM if querying not directly supported
                8 * 1024 * 1024 * 1024
            } else {
                // Integrated/shared memory
                2 * 1024 * 1024 * 1024
            };

            let interconnect = if info.device_type == wgpu::DeviceType::IntegratedGpu {
                InterconnectType::UnifiedMemory
            } else {
                InterconnectType::HostPinnedMemory
            };

            profiles.push(GpuAdapterProfile {
                id: idx,
                name: info.name,
                backend: format!("{:?}", info.backend),
                device_type: format!("{:?}", info.device_type),
                driver: info.driver,
                vram_bytes: vram_estimate,
                interconnect,
                peer_accessible_ids: Vec::new(),
                is_primary: idx == 0,
                role: if idx == 0 {
                    GpuNodeRole::Primary
                } else {
                    GpuNodeRole::TileWorker(idx)
                },
            });
        }

        Self::from_profiles(profiles, config)
    }

    /// Creates a manager directly from explicit adapter profiles (ideal for deterministic tests).
    pub fn from_profiles(mut adapters: Vec<GpuAdapterProfile>, config: MultiGpuConfig) -> Self {
        if adapters.is_empty() {
            // Provide dummy single-GPU profile for headless environments
            adapters.push(GpuAdapterProfile {
                id: 0,
                name: "Default Graphics Device".to_string(),
                backend: "Vulkan".to_string(),
                device_type: "DiscreteGpu".to_string(),
                driver: "Generic Driver".to_string(),
                vram_bytes: 8 * 1024 * 1024 * 1024,
                interconnect: InterconnectType::HostPinnedMemory,
                peer_accessible_ids: Vec::new(),
                is_primary: true,
                role: GpuNodeRole::Primary,
            });
        }

        // Determine primary adapter (prefer discrete GPU, or first adapter)
        let primary_idx = if config.prefer_discrete {
            adapters
                .iter()
                .position(|a| a.device_type.contains("Discrete"))
                .unwrap_or(0)
        } else {
            0
        };

        for (idx, adapter) in adapters.iter_mut().enumerate() {
            if idx == primary_idx {
                adapter.is_primary = true;
                adapter.role = GpuNodeRole::Primary;
            } else {
                adapter.is_primary = false;
                adapter.role = GpuNodeRole::TileWorker(idx);
            }
        }

        // Select compute offload node (first secondary discrete GPU, or secondary adapter)
        let compute_id = if adapters.len() > 1 {
            let sec_idx = adapters
                .iter()
                .position(|a| !a.is_primary && a.device_type.contains("Discrete"))
                .or_else(|| adapters.iter().position(|a| !a.is_primary));
            if let Some(idx) = sec_idx {
                adapters[idx].role = GpuNodeRole::SecondaryCompute;
                Some(adapters[idx].id)
            } else {
                None
            }
        } else {
            None
        };

        Self {
            config,
            adapters,
            primary_id: primary_idx,
            compute_id,
        }
    }

    /// Returns the number of recognized GPU adapters in the fleet.
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Returns a slice of all adapter profiles.
    pub fn adapters(&self) -> &[GpuAdapterProfile] {
        &self.adapters
    }

    /// Returns the primary adapter profile.
    pub fn primary_adapter(&self) -> &GpuAdapterProfile {
        &self.adapters[self.primary_id]
    }

    /// Returns the secondary compute adapter profile, if available.
    pub fn compute_adapter(&self) -> Option<&GpuAdapterProfile> {
        self.compute_id.map(|id| &self.adapters[id])
    }

    /// Returns the active workload partitioning strategy.
    pub fn strategy(&self) -> WorkloadPartitionStrategy {
        if !self.config.enabled || self.adapters.len() <= 1 {
            WorkloadPartitionStrategy::SingleGpu
        } else {
            self.config.strategy
        }
    }

    /// Partitions a frame of dimension `width x height` into spatial rendering tiles.
    pub fn partition_frame(
        &self,
        width: u32,
        height: u32,
    ) -> Result<Vec<TileDescriptor>, MultiGpuError> {
        if width == 0 || height == 0 {
            return Err(MultiGpuError::InvalidDimensions { width, height });
        }

        let strategy = self.strategy();
        match strategy {
            WorkloadPartitionStrategy::SingleGpu | WorkloadPartitionStrategy::ComputeOffload => {
                // Entire frame rendered as a single tile on primary adapter
                Ok(vec![TileDescriptor {
                    tile_index: 0,
                    adapter_id: self.primary_id,
                    target_x: 0,
                    target_y: 0,
                    target_width: width,
                    target_height: height,
                    render_x: 0,
                    render_y: 0,
                    render_width: width,
                    render_height: height,
                    halo_top: 0,
                    halo_bottom: 0,
                    halo_left: 0,
                    halo_right: 0,
                }])
            }
            WorkloadPartitionStrategy::SpatialTiling {
                num_tiles,
                halo_pixels,
            } => {
                let actual_tiles = num_tiles.min(self.adapters.len() as u32).max(1);
                let base_tile_height = height / actual_tiles;
                let remainder_height = height % actual_tiles;

                let mut tiles = Vec::with_capacity(actual_tiles as usize);
                let mut current_y = 0;

                for i in 0..actual_tiles {
                    let tile_idx = i as usize;
                    let adapter_id = self.adapters[tile_idx % self.adapters.len()].id;
                    let this_height = if i == actual_tiles - 1 {
                        base_tile_height + remainder_height
                    } else {
                        base_tile_height
                    };

                    // Compute halo boundaries clamped to frame borders
                    let halo_top = if current_y >= halo_pixels {
                        halo_pixels
                    } else {
                        current_y
                    };
                    let halo_bottom = if (current_y + this_height + halo_pixels) <= height {
                        halo_pixels
                    } else {
                        height.saturating_sub(current_y + this_height)
                    };

                    let render_y = current_y - halo_top;
                    let render_height = this_height + halo_top + halo_bottom;

                    tiles.push(TileDescriptor {
                        tile_index: tile_idx,
                        adapter_id,
                        target_x: 0,
                        target_y: current_y,
                        target_width: width,
                        target_height: this_height,
                        render_x: 0,
                        render_y,
                        render_width: width,
                        render_height,
                        halo_top,
                        halo_bottom,
                        halo_left: 0,
                        halo_right: 0,
                    });

                    current_y += this_height;
                }

                Ok(tiles)
            }
            WorkloadPartitionStrategy::TemporalChunking { .. } => {
                // Under temporal chunking, each frame is rendered as a single tile by scheduled adapter
                Ok(vec![TileDescriptor {
                    tile_index: 0,
                    adapter_id: self.primary_id,
                    target_x: 0,
                    target_y: 0,
                    target_width: width,
                    target_height: height,
                    render_x: 0,
                    render_y: 0,
                    render_width: width,
                    render_height: height,
                    halo_top: 0,
                    halo_bottom: 0,
                    halo_left: 0,
                    halo_right: 0,
                }])
            }
        }
    }

    /// Stitches a set of rendered tile RGBA buffers into a complete frame buffer.
    ///
    /// Strips halo margins from each tile and copies non-halo rows into the composite buffer.
    pub fn stitch_frame_tiles(
        width: u32,
        height: u32,
        tiles: &[(TileDescriptor, &[u8])],
    ) -> Result<Vec<u8>, MultiGpuError> {
        if width == 0 || height == 0 {
            return Err(MultiGpuError::InvalidDimensions { width, height });
        }

        let stride = (width as usize) * 4;
        let total_size = stride * (height as usize);
        let mut composite_frame = vec![0u8; total_size];

        for (desc, buffer) in tiles {
            // Verify source buffer size matches rendered dimensions
            let expected_tile_bytes =
                (desc.render_width as usize) * (desc.render_height as usize) * 4;
            if buffer.len() < expected_tile_bytes {
                return Err(MultiGpuError::BufferSizeMismatch {
                    tile_index: desc.tile_index,
                    expected: expected_tile_bytes,
                    actual: buffer.len(),
                });
            }

            // Verify bounds
            if desc.target_x + desc.target_width > width
                || desc.target_y + desc.target_height > height
            {
                return Err(MultiGpuError::TileOutOfBounds {
                    x: desc.target_x,
                    y: desc.target_y,
                    width: desc.target_width,
                    height: desc.target_height,
                    frame_width: width,
                    frame_height: height,
                });
            }

            let source_stride = (desc.render_width as usize) * 4;
            let target_row_bytes = (desc.target_width as usize) * 4;

            // Copy each non-halo row into the composite frame
            for row in 0..desc.target_height {
                let src_row = row + desc.halo_top;
                let src_offset = (src_row as usize) * source_stride + (desc.halo_left as usize) * 4;
                let dst_y = desc.target_y + row;
                let dst_offset = (dst_y as usize) * stride + (desc.target_x as usize) * 4;

                composite_frame[dst_offset..dst_offset + target_row_bytes]
                    .copy_from_slice(&buffer[src_offset..src_offset + target_row_bytes]);
            }
        }

        Ok(composite_frame)
    }

    /// Schedules a temporal frame index to an adapter ID under temporal chunking.
    pub fn schedule_frame_temporal(&self, frame_index: u64) -> usize {
        if self.adapters.len() <= 1 {
            return self.primary_id;
        }

        match self.config.strategy {
            WorkloadPartitionStrategy::TemporalChunking { chunk_size } => {
                let chunk_size = chunk_size.max(1) as u64;
                let chunk_index = frame_index / chunk_size;
                let adapter_slot = (chunk_index as usize) % self.adapters.len();
                self.adapters[adapter_slot].id
            }
            _ => self.primary_id,
        }
    }

    /// Returns whether compute tasks should be offloaded to a secondary GPU.
    pub fn should_offload_compute(&self) -> bool {
        self.config.enabled
            && matches!(
                self.config.strategy,
                WorkloadPartitionStrategy::ComputeOffload
            )
            && self.compute_id.is_some()
    }

    /// Returns the adapter ID designated for compute offload.
    pub fn compute_adapter_id(&self) -> usize {
        self.compute_id.unwrap_or(self.primary_id)
    }
}
