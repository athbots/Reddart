//! Comprehensive integration test suite for Sandboxed WebAssembly (Wasm / WASI) Effect & Generator Plugin Engine.

use rd_effects::{
    WasmBinaryBuilder, WasmEffectParams, WasmError, WasmInstance, WasmMemory, WasmModule, WasmOp,
    WasmParamType, WasmPluginCategory, WasmPluginInstance, WasmPluginManifest, WasmPluginParam,
    WasmValue, create_chromatic_aberration_plugin, create_crt_scanlines_plugin,
    create_vignette_pulse_plugin, global_wasm_registry,
};
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_wasm_binary_validation_and_sections() {
    let ca_bytes = create_chromatic_aberration_plugin();
    assert!(ca_bytes.len() > 32);

    // Parse valid module
    let module = WasmModule::from_bytes(&ca_bytes).expect("Valid Wasm module should parse");
    assert!(!module.types.is_empty());
    assert!(!module.functions.is_empty());
    assert!(module.get_export("render").is_some());
    assert!(module.get_export("memory").is_some());

    // Validate embedded manifest custom section
    let manifest_json = module
        .get_manifest_json()
        .expect("Manifest section must exist");
    let manifest =
        WasmPluginManifest::from_json(&manifest_json).expect("Manifest JSON must be valid");
    assert_eq!(manifest.id, "chromatic_aberration");
    assert_eq!(manifest.category, WasmPluginCategory::Distortion);

    // Corrupted magic bytes must fail validation
    let mut corrupt_magic = ca_bytes.clone();
    corrupt_magic[0] = 0xFF;
    let err = WasmModule::from_bytes(&corrupt_magic).unwrap_err();
    assert!(matches!(err, WasmError::InvalidMagic(_)));

    // Unsupported version must fail validation
    let mut corrupt_version = ca_bytes.clone();
    corrupt_version[4] = 0x02;
    let err = WasmModule::from_bytes(&corrupt_version).unwrap_err();
    assert!(matches!(err, WasmError::UnsupportedVersion(2)));
}

#[test]
fn test_wasm_memory_sandboxing_and_bounds_checking() {
    let mut memory = WasmMemory::new(1, 4).expect("Initial 1 page allocation should succeed");
    assert_eq!(memory.page_count(), 1);
    assert_eq!(memory.byte_len(), 65_536);

    // In-bounds read and write
    memory.write_u8(100, 42).expect("Write in bounds");
    assert_eq!(memory.read_u8(100).unwrap(), 42);

    memory.write_i32(200, 0x12345678).expect("Write i32");
    assert_eq!(memory.read_i32(200).unwrap(), 0x12345678);

    memory
        .write_f32(300, std::f32::consts::PI)
        .expect("Write f32");
    assert!((memory.read_f32(300).unwrap() - std::f32::consts::PI).abs() < 1e-5);

    // Out-of-bounds access beyond active pages
    let oob_err = memory.write_u8(65_536, 99).unwrap_err();
    assert!(matches!(oob_err, WasmError::OutOfBoundsAccess { .. }));

    let oob_read = memory.read_i32(65_534).unwrap_err();
    assert!(matches!(oob_read, WasmError::OutOfBoundsAccess { .. }));

    // Memory growth
    let old_pages = memory.grow(2).expect("Grow by 2 pages");
    assert_eq!(old_pages, 1);
    assert_eq!(memory.page_count(), 3);
    assert_eq!(memory.byte_len(), 3 * 65_536);

    // Exceeding maximum pages ceiling
    let grow_err = memory.grow(5).unwrap_err();
    assert!(matches!(grow_err, WasmError::MemoryAllocationFailed { .. }));
}

#[test]
fn test_wasm_gas_metering_and_infinite_loop_defense() {
    let manifest = WasmPluginManifest {
        id: "infinite_loop_trap".into(),
        name: "Infinite Loop Test".into(),
        version: "1.0.0".into(),
        author: "Security QA".into(),
        description: "Adversarial module to test gas meter ceiling".into(),
        category: WasmPluginCategory::Generator,
        parameters: Vec::new(),
    };

    let mut builder = WasmBinaryBuilder::new(manifest);
    // Emit infinite loop: loop { br 0 }
    builder.push_op(WasmOp::Loop(None));
    builder.push_op(WasmOp::Br(0));
    builder.push_op(WasmOp::End);

    let wasm_bytes = builder.build();
    let module = Arc::new(WasmModule::from_bytes(&wasm_bytes).expect("Module parses"));
    let mut instance = WasmInstance::new(module).expect("Instantiate");

    // Set tight gas limit of 10,000 steps
    instance.set_gas_limit(10_000);

    let err = instance
        .invoke_export(
            "render",
            &[
                WasmValue::I32(0),
                WasmValue::I32(100),
                WasmValue::I32(100),
                WasmValue::F32(0.0),
            ],
        )
        .unwrap_err();

    assert!(matches!(err, WasmError::GasLimitExceeded { steps: 10_001 }));
}

#[test]
fn test_wasm_plugin_manifest_serialization() {
    let manifest = WasmPluginManifest {
        id: "custom_lens_glow".into(),
        name: "Custom Lens Glow".into(),
        version: "1.2.0".into(),
        author: "Visual FX Studio".into(),
        description: "Anamorphic optical flare and glow".into(),
        category: WasmPluginCategory::Stylize,
        parameters: vec![
            WasmPluginParam {
                id: "threshold".into(),
                name: "Glow Threshold".into(),
                param_type: WasmParamType::Float {
                    min: 0.0,
                    max: 1.0,
                    default: 0.7,
                    step: 0.05,
                },
                description: "Brightness cutoff".into(),
            },
            WasmPluginParam {
                id: "tint".into(),
                name: "Glow Tint".into(),
                param_type: WasmParamType::ColorRgba {
                    default: [1.0, 0.8, 0.4, 1.0],
                },
                description: "Warm optical color".into(),
            },
            WasmPluginParam {
                id: "anamorphic".into(),
                name: "Anamorphic Streak".into(),
                param_type: WasmParamType::Boolean { default: true },
                description: "Horizontal flare aspect".into(),
            },
        ],
    };

    let json = manifest.to_json();
    assert!(json.contains("custom_lens_glow"));
    assert!(json.contains("Anamorphic Streak"));

    let restored = WasmPluginManifest::from_json(&json).expect("Deserialization succeeds");
    assert_eq!(manifest, restored);
}

#[test]
fn test_chromatic_aberration_plugin_execution() {
    let ca_bytes = create_chromatic_aberration_plugin();
    let mut plugin = WasmPluginInstance::load(&ca_bytes, None).expect("Load chromatic aberration");

    let width = 64;
    let height = 64;
    // Solid white square in center
    let mut frame = vec![0u8; (width * height * 4) as usize];
    for y in 20..44 {
        for x in 20..44 {
            let idx = ((y * width + x) * 4) as usize;
            frame[idx] = 255;
            frame[idx + 1] = 255;
            frame[idx + 2] = 255;
            frame[idx + 3] = 255;
        }
    }

    let mut params = HashMap::new();
    params.insert("intensity".into(), 0.5);

    plugin
        .process_frame(&mut frame, width, height, 0, &params)
        .expect("Process frame succeeds");

    // Verify channel displacement occurred: red channel shifted
    let target_idx = ((20 * width + 45) * 4) as usize;
    assert_eq!(frame[target_idx], 255); // Red shifted into previously black area
}

#[test]
fn test_crt_scanlines_plugin_execution() {
    let crt_bytes = create_crt_scanlines_plugin();
    let mut plugin = WasmPluginInstance::load(&crt_bytes, None).expect("Load CRT scanlines");

    let width = 32;
    let height = 32;
    // Uniform white frame (RGBA: 200, 200, 200, 255)
    let mut frame = vec![200u8; (width * height * 4) as usize];
    for i in 0..(width * height) as usize {
        frame[i * 4 + 3] = 255;
    }

    let mut params = HashMap::new();
    params.insert("intensity".into(), 0.5);

    plugin
        .process_frame(&mut frame, width, height, 0, &params)
        .expect("Process frame succeeds");

    // Even row (y = 0) remains bright
    let even_idx = 0;
    assert_eq!(frame[even_idx], 200);

    // Odd row (y = 1) is scanline attenuated: 200 >> 1 = 100
    let odd_idx = (width * 4) as usize;
    assert_eq!(frame[odd_idx], 100);
}

#[test]
fn test_vignette_pulse_plugin_temporal_modulation() {
    let vig_bytes = create_vignette_pulse_plugin();
    let mut plugin = WasmPluginInstance::load(&vig_bytes, None).expect("Load vignette pulse");

    let width = 64;
    let height = 64;
    let mut frame = vec![200u8; (width * height * 4) as usize];

    let mut params = HashMap::new();
    params.insert("radius".into(), 0.75);

    // Process at t = 0
    plugin
        .process_frame(&mut frame, width, height, 0, &params)
        .expect("Process frame t=0");

    // Corner pixel (x=5 < 20) attenuated by 25%: (200 * 3) / 4 = 150
    let corner_idx = 5 * 4;
    assert_eq!(frame[corner_idx], 150);

    // Center pixel (x=32 >= 20) unaffected
    let center_idx = (32 * width + 32) as usize * 4;
    assert_eq!(frame[center_idx], 200);
}

#[test]
fn test_global_wasm_registry_and_dynamic_instantiation() {
    let registry = global_wasm_registry().lock().unwrap();
    let manifests = registry.list_manifests();
    assert!(manifests.len() >= 3);

    let ca = manifests
        .iter()
        .find(|m| m.id == "chromatic_aberration")
        .expect("chromatic_aberration must be registered");
    assert_eq!(ca.name, "WASM Chromatic Aberration");

    let crt = manifests
        .iter()
        .find(|m| m.id == "crt_scanlines")
        .expect("crt_scanlines must be registered");
    assert_eq!(crt.category, WasmPluginCategory::Stylize);

    let mut instance = registry
        .instantiate("chromatic_aberration")
        .expect("Instantiate CA");
    let mut buffer = vec![128u8; 16 * 16 * 4];
    instance
        .process_frame(&mut buffer, 16, 16, 0, &HashMap::new())
        .expect("Run instance");
}

#[test]
fn test_wasm_effect_params_apply_helper() {
    let width = 32;
    let height = 32;
    let mut frame = vec![200u8; (width * height * 4) as usize];
    for i in 0..(width * height) as usize {
        frame[i * 4 + 3] = 255;
    }

    let mut wasm_params = WasmEffectParams::new("crt_scanlines");
    wasm_params.parameters.insert("intensity".into(), 0.5);

    wasm_params
        .apply(&mut frame, width, height)
        .expect("Apply via helper");

    // Even rows = 200
    assert_eq!(frame[0], 200);
    // Odd rows = 100
    assert_eq!(frame[(width * 4) as usize], 100);
}
