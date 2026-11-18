//! Integration test verifying sandboxed WebAssembly effect execution in CompositorLayer.

use rd_effects::WasmEffectParams;
use rd_render::composite::{CompositorLayer, composite_layers_over};

#[test]
fn test_compositor_layer_wasm_scanlines_effect() {
    let width = 32;
    let height = 32;
    let mut target = vec![0u8; (width * height * 4) as usize];

    // Solid base layer
    let mut base_pixels = vec![200u8; (width * height * 4) as usize];
    for i in 0..(width * height) as usize {
        base_pixels[i * 4 + 3] = 255;
    }
    let mut layer = CompositorLayer::new(base_pixels, width, height);

    let mut wasm_params = WasmEffectParams::new("crt_scanlines");
    wasm_params.parameters.insert("intensity".into(), 0.5);
    layer.wasm_effect = Some(wasm_params);

    composite_layers_over(&mut target, width, height, &[layer]);

    // Target received composited pixels:
    // Even rows = 200
    assert_eq!(target[0], 200);
    assert_eq!(target[1], 200);
    assert_eq!(target[2], 200);
    assert_eq!(target[3], 255);

    // Odd rows = scanline attenuated (200 >> 1 = 100)
    let odd_row_idx = (width * 4) as usize;
    assert_eq!(target[odd_row_idx], 100);
    assert_eq!(target[odd_row_idx + 1], 100);
    assert_eq!(target[odd_row_idx + 2], 100);
    assert_eq!(target[odd_row_idx + 3], 255);
}

#[test]
fn test_compositor_layer_wasm_vignette_effect() {
    let width = 64;
    let height = 64;
    let mut target = vec![0u8; (width * height * 4) as usize];

    let base_pixels = vec![200u8; (width * height * 4) as usize];
    let mut layer = CompositorLayer::new(base_pixels, width, height);

    let wasm_params = WasmEffectParams::new("vignette_pulse");
    layer.wasm_effect = Some(wasm_params);

    composite_layers_over(&mut target, width, height, &[layer]);

    // Corner pixel attenuated by 25%: (200 * 3) / 4 = 150
    let corner_idx = 5 * 4;
    assert_eq!(target[corner_idx], 150);

    // Center pixel unaffected: 200
    let center_idx = (32 * width + 32) as usize * 4;
    assert_eq!(target[center_idx], 200);
}
