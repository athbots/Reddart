use rd_effects::ShapeMaskParams;
use rd_render::{CompositorLayer, composite_layers_over};

#[test]
fn test_simultaneous_multi_layer_video_compositing() {
    let width = 64u32;
    let height = 64u32;
    let mut frame_buffer = vec![0u8; (width * height * 4) as usize];

    // Track 1 (V1 Background): Solid Blue (0, 0, 255, 255)
    let v1 = CompositorLayer::new(
        [0, 0, 255, 255].repeat((width * height) as usize),
        width,
        height,
    );

    // Track 2 (V2 Overlay): Solid Yellow (255, 255, 0, 255) at 50% opacity
    let mut v2 = CompositorLayer::new(
        [255, 255, 0, 255].repeat((width * height) as usize),
        width,
        height,
    );
    v2.opacity = 0.5;

    composite_layers_over(&mut frame_buffer, width, height, &[v1, v2]);

    // Blending 50% Yellow (255, 255, 0) over Blue (0, 0, 255)
    // Red: 255 * 0.5 = ~128
    // Green: 255 * 0.5 = ~128
    // Blue: 255 * (1 - 0.5) = ~128
    // Alpha: 255
    let pixel = &frame_buffer[0..4];
    assert_eq!(pixel[3], 255);
    assert!(
        (pixel[0] as i16 - 128).abs() <= 2,
        "Red channel was {}",
        pixel[0]
    );
    assert!(
        (pixel[1] as i16 - 128).abs() <= 2,
        "Green channel was {}",
        pixel[1]
    );
    assert!(
        (pixel[2] as i16 - 128).abs() <= 2,
        "Blue channel was {}",
        pixel[2]
    );
}

#[test]
fn test_multi_layer_spatial_transform_and_shape_mask() {
    let width = 100u32;
    let height = 100u32;
    let mut frame_buffer = vec![0u8; (width * height * 4) as usize];

    // Track 1 (V1): Solid Black background
    let v1 = CompositorLayer::new(
        [0, 0, 0, 255].repeat((width * height) as usize),
        width,
        height,
    );

    // Track 2 (V2): Picture-in-picture Solid Red box centered (scale 0.5)
    let mut v2 = CompositorLayer::new(
        [255, 0, 0, 255].repeat((width * height) as usize),
        width,
        height,
    );
    v2.scale_x = 0.5;
    v2.scale_y = 0.5;

    // Track 3 (V3): Picture-in-picture Solid Green box in top-left with circular mask
    let mut v3 = CompositorLayer::new(
        [0, 255, 0, 255].repeat((width * height) as usize),
        width,
        height,
    );
    v3.pos_x = -0.5;
    v3.pos_y = -0.5;
    v3.scale_x = 0.4;
    v3.scale_y = 0.4;
    v3.shape_mask = Some(ShapeMaskParams {
        shape_type: 1, // Circle
        center_x: 0.25,
        center_y: 0.25,
        size_x: 0.2,
        size_y: 0.2,
        feather: 0.01,
        invert: false,
    });

    composite_layers_over(&mut frame_buffer, width, height, &[v1, v2, v3]);

    // Center pixel (50, 50) should be covered by V2 (Red)
    let center_idx = ((50 * width + 50) * 4) as usize;
    assert_eq!(&frame_buffer[center_idx..center_idx + 4], &[255, 0, 0, 255]);

    // Bottom-right corner (90, 90) should be Black (V1 background)
    let br_idx = ((90 * width + 90) * 4) as usize;
    assert_eq!(&frame_buffer[br_idx..br_idx + 4], &[0, 0, 0, 255]);

    // Top-left circle center (25, 25) should have Green from V3
    let tl_idx = ((25 * width + 25) * 4) as usize;
    assert!(
        frame_buffer[tl_idx + 1] > 200,
        "Expected green component at circle center"
    );
}
