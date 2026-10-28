# 🎬 Reddart

**Zero-Lag Video Editor** — No subscription, no watermark, no ads. Fully offline, private by design.

## Architecture

- **Shared Rust Core** — Project model, render graph, effects, audio mixing
- **WGSL Shaders** — One shader language across all platforms via wgpu
- **Native UI** — egui (Windows), Jetpack Compose (Android), SwiftUI (Mac/iOS)
- **On-Device AI** — All ML inference runs locally, no cloud dependency

## Building (Windows)

```bash
# Prerequisites: Rust toolchain (via rustup)
cargo build --release

# Run the editor
cargo run --release -p reddart-windows
```

## Project Structure

```
reddart/
├── crates/
│   ├── rd-core/        # Project model, undo, autosave
│   ├── rd-render/      # Render graph, cache, scheduler
│   ├── rd-effects/     # WGSL effect library
│   ├── rd-audio/       # Audio mixer
│   ├── rd-decode/      # Decoder abstraction
│   ├── rd-export/      # Export pipeline
│   └── rd-ml/          # ML runtime wrapper
├── platforms/
│   └── windows/        # Windows app (egui + wgpu)
├── effects/            # WGSL shader files + manifests
└── tests/              # Conformance & fuzz tests
```

## License

TBD — See development plan for licensing considerations.
