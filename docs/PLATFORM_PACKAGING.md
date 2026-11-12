# Reddart Cross-Platform Packaging & Release Architecture

**Target Version:** Reddart v1.0.0  
**Supported Platforms:** Windows 10/11 x64, macOS Monterey+ (Universal 2), Linux x86_64 (glibc 2.31+)

---

## 1. Overview & Packaging Philosophy

Reddart provides native, self-contained distribution bundles for each target platform with:
- **Zero runtime build dependencies**: Clean machine launch with verified system libraries.
- **Bit-for-bit deterministic archives**: Fixed timestamp epochs (`2026-01-01T00:00:00Z`) and sorted file hierarchies.
- **Cryptographic verification**: Synchronized `SHA256SUMS.txt` and structured `manifest.json` on all platforms.
- **Strict effect bundling**: All 9 production WGSL shaders and manifests packaged across all platforms.

---

## 2. Platform Packaging Specifications

### 2.1. Windows (`ready_to_use/windows/`)
- **Package Archive:** `Reddart-v1.0.0-Windows-x86_64.zip`
- **Primary Binary:** `Reddart.exe` (x86_64 PE32+ executable)
- **Engine Backends:** Direct3D 12, Vulkan, WASAPI (48 kHz)
- **Packaging Script:** `scripts/package_windows.ps1`
- **Validation:** Automatic extraction, `--version`, `--help`, template listing, and sample project media verification.

### 2.2. macOS Universal (`ready_to_use/mac/`)
- **Package Archive:** `Reddart-v1.0.0-macOS-Universal.zip`
- **App Bundle:** `Reddart.app`
  - `Contents/MacOS/Reddart`: Standard POSIX launcher wrapper.
  - `Contents/MacOS/reddart_bin`: Universal 2 Mach-O binary.
  - `Contents/Frameworks/libreddart.dylib`: Apple Native Engine shared library (`rd-apple`).
  - `Contents/Resources/effects/`: All 9 WGSL shaders & JSON manifests.
  - `Contents/Resources/reddart.h`: C-ABI header for Swift/Metal bridge.
  - `Contents/Info.plist`: High-DPI, automatic graphics switching, mic usage permissions.
- **Engine Backends:** Metal, AppKit/SwiftUI, CoreAudio
- **Packaging Scripts:**
  - POSIX bash: `scripts/package_mac.sh`
  - PowerShell: `scripts/package_mac.ps1` / `platforms/mac/build_mac.ps1`

### 2.3. Linux Standalone (`ready_to_use/linux/`)
- **Package Archive:** `Reddart-v1.0.0-Linux-x86_64.tar.gz` and `.zip`
- **Layout:** FreeDesktop Standard
  - `bin/reddart`: ELF 64-bit LSB executable.
  - `share/applications/reddart.desktop`: FreeDesktop Desktop Entry 1.5 specification.
  - `share/metainfo/app.reddart.Reddart.metainfo.xml`: AppStream 1.0 metadata.
  - `share/icons/hicolor/scalable/apps/reddart.svg`: Scalable vector icon.
  - `share/reddart/effects/`: All 9 WGSL shaders & JSON manifests.
  - `share/reddart/samples/`: Portable demo project and assets.
- **Engine Backends:** Vulkan, Wayland / X11, ALSA / PulseAudio
- **Packaging Scripts:**
  - POSIX bash: `scripts/package_linux.sh`
  - PowerShell: `scripts/package_linux.ps1`

---

## 3. Verification & CI Workflow

Each CI run enforces:
1. `cargo test -p rd-export --test release_integrity_test` validating hashes and manifests across all 3 platforms.
2. `scripts/package_linux.sh --skip-build --verify-reproducibility` on Ubuntu runner.
3. `scripts/package_mac.sh --skip-build` on macOS runner.
4. `scripts/package_windows.ps1 -VerifyReproducibility` on Windows runner.

