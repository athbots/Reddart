# Sandboxed WebAssembly (Wasm / WASI) Effect & Procedural Generator Plugin Engine

**Document Reference:** RD-WASM-2026-V1  
**Target:** Reddart Video Editor v1.5.0-dev (Phase V Milestone 25)  
**Status:** Implemented, Tested & Verified  
**MSRV:** Rust 1.85.0 (2024 Edition)  

---

## 1. Executive Summary & Design Principles

Reddart v1.5.0 introduces Milestone 25: an in-engine, pure-Rust **Sandboxed WebAssembly (Wasm / WASI) Effect & Procedural Generator Plugin Engine**. This runtime enables creators and third-party visual effects engineers to extend Reddart with custom image processing filters, retro stylization routines, glitch generators, and animated transitions without compromising system security, host memory integrity, or playback stability.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Reddart Sandboxed Wasm Plugin Architecture               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [Host Application: Reddart Studio / Export Pipeline]                      │
│     │                                                                       │
│     ├── Staged Video Frame (RGBA 8-bit / Float)                             │
│     │     │                                                                 │
│     │     ▼ [Hardware Bounds-Checked Copy]                                  │
│     │  ┌──────────────────────────────────────────────────────────────┐     │
│     │  │ Guest Linear Memory (Isolated 64 KiB Wasm Pages)             │     │
│     │  │  - Page 0: Reserved Guest Stack & Local Structures           │     │
│     │  │  - Page 1+: Pixel Frame Staging Buffer (Up to 16 MiB max)    │     │
│     │  │  - Zero Host Pointers / Zero Raw OS Addresses                │     │
│     │  └──────────────────────────────────────────────────────────────┘     │
│     │     ▲                                                                 │
│     └── Pure Rust Stack-Based Wasm Virtual Machine                          │
│           - Instructions: Integer/Float Math, Bitwise, Load/Store, Br/BrIf  │
│           - Gas Metering: 50,000,000 instruction step ceiling per frame     │
│           - Security Boundary: 0 Network Sockets, 0 Filesystem Descriptors  │
│           - Host ABI: reddart_get_param, reddart_log                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Core Security & Performance Guarantees:
1. **Memory Sandboxing:** Linear memory is partitioned in 64 KiB WebAssembly pages up to a strict configurable ceiling (`DEFAULT_MAX_PAGES = 256` = 16 MiB). Out-of-bounds pointer reads or writes fail immediately with `WasmError::OutOfBoundsAccess`, preventing buffer overruns and arbitrary code execution (`CWE-119`, `CWE-787`).
2. **Denial-of-Service & Hang Immunity:** Every instruction cycle decrements an execution gas budget (`DEFAULT_GAS_LIMIT = 50,000,000`). Runaway loops or halting bugs trigger an immediate `WasmError::GasLimitExceeded` trap rather than freezing the rendering thread.
3. **Zero Host I/O Leakage:** Guest bytecode executes within an unprivileged sandbox. It possesses no access to the host operating system filesystem, network stack, or command execution sub-processes.
4. **Deterministic Reproducibility:** Floating-point and integer arithmetic execute deterministically, guaranteeing identical frame output across Windows, macOS, and Linux runners.

---

## 2. WebAssembly Instruction Subset & Memory Model

The Reddart virtual machine (`rd_effects::wasm`) executes standard WebAssembly binary modules (`.wasm`) compliant with the WebAssembly 1.0 Core Specification:

### 2.1 Supported Instruction Spectrum

| Class | Opcode Range | Operations Supported |
|---|---|---|
| **Control Flow** | `0x00`–`0x0F` | `Unreachable`, `Nop`, `Block`, `Loop`, `If`, `Else`, `End`, `Br`, `BrIf`, `Return`, `Call` |
| **Stack Manipulation** | `0x1A`–`0x1B` | `Drop`, `Select` |
| **Variables & Scope** | `0x20`–`0x24` | `LocalGet`, `LocalSet`, `LocalTee`, `GlobalGet`, `GlobalSet` |
| **Linear Memory Access** | `0x28`–`0x40` | `I32Load`, `F32Load`, `I32Load8U`, `I32Store`, `F32Store`, `I32Store8`, `MemorySize`, `MemoryGrow` |
| **Constants** | `0x41`–`0x44` | `I32Const`, `I64Const`, `F32Const`, `F64Const` |
| **Integer Arithmetic** | `0x6A`–`0x76` | `I32Add`, `I32Sub`, `I32Mul`, `I32DivS`, `I32DivU`, `I32RemS`, `I32RemU`, `I32And`, `I32Or`, `I32Xor`, `I32Shl`, `I32ShrS`, `I32ShrU` |
| **Float Arithmetic** | `0x92`–`0x8F` | `F32Add`, `F32Sub`, `F32Mul`, `F32Div`, `F32Min`, `F32Max`, `F32Abs`, `F32Neg`, `F32Sqrt` |
| **Comparisons** | `0x45`–`0x60` | `I32Eqz`, `I32Eq`, `I32Ne`, `I32LtS`, `I32LtU`, `I32GtS`, `I32GtU`, `I32LeS`, `I32LeU`, `I32GeS`, `I32GeU`, `F32Eq`, `F32Ne`, `F32Lt`, `F32Gt`, `F32Le`, `F32Ge` |
| **Type Conversions** | `0xA7`–`0xB3` | `I32WrapI64`, `I32TruncF32S`, `I32TruncF32U`, `F32ConvertI32S`, `F32ConvertI32U` |

### 2.2 Control Flow Stack & Branching Invariants
WebAssembly branches (`br`, `br_if`) reference relative label indices in the structured control stack:
- Index 0 refers to the innermost structured block, loop, or conditional.
- Targeting a `Loop` unwinds the control frame and sets the program counter to the loop header (`start_pc`), repeating the block without recursion.
- Targeting a `Block` or `If` jumps forward to the matching terminator (`end_pc`), exiting the structure.

---

## 3. Declarative Plugin Manifest (`plugin.json`)

Modules embed an arbitrary custom section named `"manifest"` or `"plugin.json"` containing metadata and declarative parameter contracts:

```json
{
  "id": "chromatic_aberration",
  "name": "WASM Chromatic Aberration",
  "version": "1.0.0",
  "author": "Reddart Systems",
  "description": "Spectral color dispersion and lens fringing in sandboxed WebAssembly",
  "category": "distortion",
  "parameters": [
    {
      "id": "intensity",
      "name": "Displacement Intensity",
      "param_type": {
        "float": {
          "min": 0.0,
          "max": 1.0,
          "default": 0.35,
          "step": 0.01
        }
      },
      "description": "Distance of color fringing separation"
    },
    {
      "id": "angle_deg",
      "name": "Dispersion Angle",
      "param_type": {
        "float": {
          "min": 0.0,
          "max": 360.0,
          "default": 0.0,
          "step": 1.0
        }
      },
      "description": "Directional vector of RGB separation"
    }
  ]
}
```

---

## 4. Built-in Production Reference Plugins

Reddart bundles three production-grade WebAssembly plugins built into the engine binary:

1. **`chromatic_aberration` (Category: Distortion):**
   - Simulates refractive chromatic aberration and lens edge fringing by shifting red and blue spectral channels relative to green.
   - Employs a non-destructive backward memory traversal avoiding in-place raster overwrites.
   - Parameters: `intensity` ($0.0\text{--}1.0$), `angle_deg` ($0.0\text{--}360.0^\circ$).
2. **`crt_scanlines` (Category: Stylize):**
   - Recreates retro cathode-ray tube (CRT) beam raster scanlines with alternating horizontal line intensity modulation.
   - Darkens odd scanlines by bitwise shift attenuation ($R, G, B \gg 1$).
   - Parameters: `scanline_count` ($100\text{--}1080$), `intensity` ($0.0\text{--}1.0$).
3. **`vignette_pulse` (Category: Stylize):**
   - Applies temporal animated vignette edge darkening with sinusoidal pulsation.
   - Parameters: `radius` ($0.1\text{--}1.5$), `pulse_speed_hz` ($0.1\text{--}5.0\text{ Hz}$).

---

## 5. Compositor & Export Pipeline Parity

The sandboxed Wasm effect engine is hooked directly into the multi-track compositing pipeline:
- `rd_render::CompositorLayer`: Contains optional `wasm_effect: Option<WasmEffectParams>` evaluated on the pre-transform layer buffer before spatial placement.
- `rd_export::pipeline`: Headless rendering pipeline extracts active `wasm_*` or `wasm:*` effects from `clip.effects` and applies them with microsecond timestamp fidelity (`time_us`).
- Thread-safe global catalog: `global_wasm_registry()` provides dynamic plugin registration, catalog queries, and isolated runtime instantiation.

---

## 6. Verification Suite

The plugin engine is covered by automated integration test suites:
- `crates/rd-effects/tests/wasm_plugin_engine_test.rs`:
  - `test_wasm_binary_validation_and_sections`: Bytecode magic and version verification.
  - `test_wasm_memory_sandboxing_and_bounds_checking`: 64 KiB page allocation, growth, and OOB access prevention.
  - `test_wasm_gas_metering_and_infinite_loop_defense`: Runaway loop detection and gas limit traps.
  - `test_wasm_plugin_manifest_serialization`: Manifest round-trip serialization.
  - `test_chromatic_aberration_plugin_execution`: In-engine color fringing verification.
  - `test_crt_scanlines_plugin_execution`: Scanline raster modulation verification.
  - `test_vignette_pulse_plugin_temporal_modulation`: Time-based parameter animation verification.
  - `test_global_wasm_registry_and_dynamic_instantiation`: Dynamic registry catalog verification.
  - `test_wasm_effect_params_apply_helper`: High-level frame application verification.
- `crates/rd-render/tests/wasm_compositor_test.rs`:
  - `test_compositor_layer_wasm_scanlines_effect`: Compositing DAG integration.
  - `test_compositor_layer_wasm_vignette_effect`: Vignette compositing over canvas.
