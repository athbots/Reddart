//! Integration tests for Multi-GPU distributed rendering and peer-to-peer memory management.

use rd_render::{
    GpuAdapterProfile, GpuNodeRole, InterconnectType, MultiGpuConfig, MultiGpuError,
    MultiGpuManager, TileDescriptor, WorkloadPartitionStrategy,
};

/// Helper to construct a simulated multi-GPU fleet with heterogeneous adapters.
fn create_test_fleet() -> Vec<GpuAdapterProfile> {
    vec![
        GpuAdapterProfile {
            id: 0,
            name: "NVIDIA GeForce RTX 4090 (Primary)".to_string(),
            backend: "Vulkan".to_string(),
            device_type: "DiscreteGpu".to_string(),
            driver: "560.94".to_string(),
            vram_bytes: 24 * 1024 * 1024 * 1024,
            interconnect: InterconnectType::NvLink,
            peer_accessible_ids: vec![1],
            is_primary: true,
            role: GpuNodeRole::Primary,
        },
        GpuAdapterProfile {
            id: 1,
            name: "NVIDIA GeForce RTX 4090 (Secondary Compute)".to_string(),
            backend: "Vulkan".to_string(),
            device_type: "DiscreteGpu".to_string(),
            driver: "560.94".to_string(),
            vram_bytes: 24 * 1024 * 1024 * 1024,
            interconnect: InterconnectType::NvLink,
            peer_accessible_ids: vec![0],
            is_primary: false,
            role: GpuNodeRole::TileWorker(1),
        },
        GpuAdapterProfile {
            id: 2,
            name: "Intel UHD Graphics 770 (Integrated)".to_string(),
            backend: "Vulkan".to_string(),
            device_type: "IntegratedGpu".to_string(),
            driver: "31.0.101.4575".to_string(),
            vram_bytes: 2 * 1024 * 1024 * 1024,
            interconnect: InterconnectType::UnifiedMemory,
            peer_accessible_ids: vec![],
            is_primary: false,
            role: GpuNodeRole::TileWorker(2),
        },
    ]
}

#[test]
fn test_multi_adapter_discovery_and_role_assignment() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: true,
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::SpatialTiling {
            num_tiles: 2,
            halo_pixels: 16,
        },
        max_adapters: 4,
        default_halo_pixels: 16,
    };

    let manager = MultiGpuManager::from_profiles(fleet, config);
    assert_eq!(manager.adapter_count(), 3);

    // Primary adapter must be the first discrete GPU
    let primary = manager.primary_adapter();
    assert_eq!(primary.id, 0);
    assert!(primary.is_primary);
    assert_eq!(primary.role, GpuNodeRole::Primary);
    assert_eq!(primary.interconnect, InterconnectType::NvLink);

    // Compute adapter must be the second discrete GPU
    let compute = manager.compute_adapter().expect("Compute adapter present");
    assert_eq!(compute.id, 1);
    assert_eq!(compute.role, GpuNodeRole::SecondaryCompute);
    assert_eq!(compute.vram_bytes, 24 * 1024 * 1024 * 1024);
}

#[test]
fn test_spatial_frame_partition_and_halo_geometry() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: true,
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::SpatialTiling {
            num_tiles: 2,
            halo_pixels: 16,
        },
        max_adapters: 2,
        default_halo_pixels: 16,
    };

    let manager = MultiGpuManager::from_profiles(fleet, config);

    // Partition 4K UHD (3840 x 2160)
    let width = 3840;
    let height = 2160;
    let tiles = manager
        .partition_frame(width, height)
        .expect("Valid partition");
    assert_eq!(tiles.len(), 2);

    // Tile 0: Top half
    let t0 = &tiles[0];
    assert_eq!(t0.tile_index, 0);
    assert_eq!(t0.target_x, 0);
    assert_eq!(t0.target_y, 0);
    assert_eq!(t0.target_width, 3840);
    assert_eq!(t0.target_height, 1080);
    assert_eq!(t0.halo_top, 0); // At top canvas boundary
    assert_eq!(t0.halo_bottom, 16); // Overlaps bottom tile
    assert_eq!(t0.render_y, 0);
    assert_eq!(t0.render_height, 1096); // 1080 + 16

    // Tile 1: Bottom half
    let t1 = &tiles[1];
    assert_eq!(t1.tile_index, 1);
    assert_eq!(t1.target_x, 0);
    assert_eq!(t1.target_y, 1080);
    assert_eq!(t1.target_width, 3840);
    assert_eq!(t1.target_height, 1080);
    assert_eq!(t1.halo_top, 16); // Overlaps top tile
    assert_eq!(t1.halo_bottom, 0); // At bottom canvas boundary
    assert_eq!(t1.render_y, 1080 - 16);
    assert_eq!(t1.render_height, 1096); // 1080 + 16

    // Verify seamless vertical coverage: 1080 + 1080 = 2160
    assert_eq!(t0.target_height + t1.target_height, height);
}

#[test]
fn test_spatial_frame_stitching_bit_for_bit() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: true,
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::SpatialTiling {
            num_tiles: 2,
            halo_pixels: 4,
        },
        max_adapters: 2,
        default_halo_pixels: 4,
    };

    let manager = MultiGpuManager::from_profiles(fleet, config);

    let width = 64;
    let height = 32;
    let tiles = manager
        .partition_frame(width, height)
        .expect("Valid partition");
    assert_eq!(tiles.len(), 2);

    // Create unique pattern for full frame: color = (x % 256, y % 256, 128, 255)
    let stride = (width as usize) * 4;
    let mut ground_truth = vec![0u8; stride * (height as usize)];
    for y in 0..height {
        for x in 0..width {
            let offset = ((y as usize) * stride) + (x as usize) * 4;
            ground_truth[offset] = (x % 256) as u8;
            ground_truth[offset + 1] = (y % 256) as u8;
            ground_truth[offset + 2] = 128;
            ground_truth[offset + 3] = 255;
        }
    }

    // Populate rendered tile buffers including halo
    let mut tile_buffers: Vec<(TileDescriptor, Vec<u8>)> = Vec::new();
    for desc in &tiles {
        let tile_stride = (desc.render_width as usize) * 4;
        let mut buffer = vec![0u8; tile_stride * (desc.render_height as usize)];

        for r_row in 0..desc.render_height {
            let canvas_y = desc.render_y + r_row;
            for r_col in 0..desc.render_width {
                let canvas_x = desc.render_x + r_col;
                let buf_offset = ((r_row as usize) * tile_stride) + (r_col as usize) * 4;

                // Color depends on canvas coordinate
                buffer[buf_offset] = (canvas_x % 256) as u8;
                buffer[buf_offset + 1] = (canvas_y % 256) as u8;
                buffer[buf_offset + 2] = 128;
                buffer[buf_offset + 3] = 255;
            }
        }
        tile_buffers.push((*desc, buffer));
    }

    // Stitch tiles
    let slice_refs: Vec<(TileDescriptor, &[u8])> = tile_buffers
        .iter()
        .map(|(desc, buf)| (*desc, buf.as_slice()))
        .collect();

    let stitched =
        MultiGpuManager::stitch_frame_tiles(width, height, &slice_refs).expect("Successful stitch");

    // Bit-for-bit exact check against ground truth
    assert_eq!(
        stitched.len(),
        ground_truth.len(),
        "Stitched buffer length must match ground truth"
    );
    assert_eq!(
        stitched, ground_truth,
        "Stitched frame pixels must match ground truth exactly without halo corruption"
    );
}

#[test]
fn test_odd_dimensions_and_remainder_handling() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: true,
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::SpatialTiling {
            num_tiles: 3,
            halo_pixels: 8,
        },
        max_adapters: 3,
        default_halo_pixels: 8,
    };

    let manager = MultiGpuManager::from_profiles(fleet, config);

    // 1920 x 1081 (1081 is not divisible by 3: 360 + 360 + 361)
    let width = 1920;
    let height = 1081;
    let tiles = manager
        .partition_frame(width, height)
        .expect("Valid partition");
    assert_eq!(tiles.len(), 3);

    assert_eq!(tiles[0].target_height, 360);
    assert_eq!(tiles[1].target_height, 360);
    assert_eq!(tiles[2].target_height, 361);

    let sum_height: u32 = tiles.iter().map(|t| t.target_height).sum();
    assert_eq!(sum_height, height, "Sum of tile heights must equal 1081");

    // Verify last tile touches bottom boundary
    let last = &tiles[2];
    assert_eq!(last.target_y + last.target_height, height);
    assert_eq!(last.halo_bottom, 0);
}

#[test]
fn test_temporal_frame_scheduling_round_robin() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: true,
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::TemporalChunking { chunk_size: 2 },
        max_adapters: 3,
        default_halo_pixels: 0,
    };

    let manager = MultiGpuManager::from_profiles(fleet, config);

    // Chunk size 2 across 3 adapters (IDs 0, 1, 2):
    // Frames 0, 1 -> GPU 0
    // Frames 2, 3 -> GPU 1
    // Frames 4, 5 -> GPU 2
    // Frames 6, 7 -> GPU 0
    assert_eq!(manager.schedule_frame_temporal(0), 0);
    assert_eq!(manager.schedule_frame_temporal(1), 0);
    assert_eq!(manager.schedule_frame_temporal(2), 1);
    assert_eq!(manager.schedule_frame_temporal(3), 1);
    assert_eq!(manager.schedule_frame_temporal(4), 2);
    assert_eq!(manager.schedule_frame_temporal(5), 2);
    assert_eq!(manager.schedule_frame_temporal(6), 0);
    assert_eq!(manager.schedule_frame_temporal(7), 0);
}

#[test]
fn test_compute_offload_decision() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: true,
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::ComputeOffload,
        max_adapters: 3,
        default_halo_pixels: 0,
    };

    let manager = MultiGpuManager::from_profiles(fleet.clone(), config.clone());
    assert!(manager.should_offload_compute());
    assert_eq!(manager.compute_adapter_id(), 1);

    // Single GPU fleet should not offload
    let single_fleet = vec![fleet[0].clone()];
    let single_manager = MultiGpuManager::from_profiles(single_fleet, config);
    assert!(!single_manager.should_offload_compute());
    assert_eq!(single_manager.compute_adapter_id(), 0);
}

#[test]
fn test_single_gpu_fallback_graceful_degradation() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig {
        enabled: false, // Disabled multi-GPU
        prefer_discrete: true,
        strategy: WorkloadPartitionStrategy::SpatialTiling {
            num_tiles: 4,
            halo_pixels: 16,
        },
        max_adapters: 4,
        default_halo_pixels: 16,
    };

    let manager = MultiGpuManager::from_profiles(fleet, config);
    assert_eq!(manager.strategy(), WorkloadPartitionStrategy::SingleGpu);

    let tiles = manager.partition_frame(1920, 1080).expect("Partition");
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].target_width, 1920);
    assert_eq!(tiles[0].target_height, 1080);
    assert_eq!(tiles[0].halo_top, 0);
    assert_eq!(tiles[0].halo_bottom, 0);
}

#[test]
fn test_multi_gpu_error_conditions() {
    let fleet = create_test_fleet();
    let config = MultiGpuConfig::default();
    let manager = MultiGpuManager::from_profiles(fleet, config);

    // Invalid dimensions
    assert_eq!(
        manager.partition_frame(0, 1080),
        Err(MultiGpuError::InvalidDimensions {
            width: 0,
            height: 1080
        })
    );
    assert_eq!(
        manager.partition_frame(1920, 0),
        Err(MultiGpuError::InvalidDimensions {
            width: 1920,
            height: 0
        })
    );

    // Buffer size mismatch during stitch
    let desc = TileDescriptor {
        tile_index: 0,
        adapter_id: 0,
        target_x: 0,
        target_y: 0,
        target_width: 100,
        target_height: 100,
        render_x: 0,
        render_y: 0,
        render_width: 100,
        render_height: 100,
        halo_top: 0,
        halo_bottom: 0,
        halo_left: 0,
        halo_right: 0,
    };
    let too_short_buffer = vec![0u8; 100]; // Expected 100 * 100 * 4 = 40,000 bytes
    let err = MultiGpuManager::stitch_frame_tiles(100, 100, &[(desc, &too_short_buffer)]);
    assert_eq!(
        err,
        Err(MultiGpuError::BufferSizeMismatch {
            tile_index: 0,
            expected: 40000,
            actual: 100
        })
    );

    // Out of bounds tile
    let oob_desc = TileDescriptor {
        tile_index: 0,
        adapter_id: 0,
        target_x: 50,
        target_y: 50,
        target_width: 100,
        target_height: 100,
        render_x: 50,
        render_y: 50,
        render_width: 100,
        render_height: 100,
        halo_top: 0,
        halo_bottom: 0,
        halo_left: 0,
        halo_right: 0,
    };
    let valid_buffer = vec![0u8; 40000];
    let oob_err = MultiGpuManager::stitch_frame_tiles(100, 100, &[(oob_desc, &valid_buffer)]);
    assert_eq!(
        oob_err,
        Err(MultiGpuError::TileOutOfBounds {
            x: 50,
            y: 50,
            width: 100,
            height: 100,
            frame_width: 100,
            frame_height: 100
        })
    );
}
