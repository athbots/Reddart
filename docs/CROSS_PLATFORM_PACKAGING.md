# Cross-Platform Distribution Packaging Architecture

## 1. Executive Summary

Reddart provides an automated, reproducible packaging matrix for macOS, Linux, and Windows. This document details the packaging specifications, bundle architectures, sandboxing permissions, code signing, notarization, and Wayland fractional scaling configurations across platforms.

---

## 2. Multi-Platform Packaging Matrix

| Target Platform | Package Format | Distribution Architecture | Display & Windowing Subsystem |
|---|---|---|---|
| **macOS** | `.app` Bundle, `.dmg` Disk Image, `.zip` | Universal 2 (Apple Silicon M-Series + Intel x86_64) | Cocoa / SwiftUI + Metal (120Hz ProMotion) |
| **Linux** | Flatpak, AppImage, `.tar.gz`, `.zip` | x86_64 Linux (glibc 2.31+) | Wayland (native fractional scaling) / X11 + Vulkan 1.3 |
| **Windows** | Portable `.zip`, WiX MSI Installer | x86_64 Windows (Windows 10 / 11) | Direct3D 12 / DXGI (HDR10 & scRGB) + wgpu |

---

## 3. macOS Distribution Architecture

### 3.1 `Reddart.app` Bundle Directory Layout

```
Reddart.app/
├── Contents/
│   ├── Info.plist                     # Bundle metadata & security permissions
│   ├── PkgInfo                        # Package type identifier (APPLRDRT)
│   ├── MacOS/
│   │   ├── Reddart                    # Shell launch wrapper (sets DYLD_LIBRARY_PATH)
│   │   └── reddart_bin                # Universal 2 compiled Mach-O binary
│   ├── Frameworks/
│   │   └── libreddart.dylib           # Shared Rust engine (rd-apple C ABI bridge)
│   └── Resources/
│       ├── effects/                   # 12 bundled WGSL shader effects
│       ├── reddart.h                  # C header declaration for Swift bindings
│       └── samples/                   # Sample project assets
```

### 3.2 High-Performance Info.plist Directives

The `Info.plist` manifest specifies crucial hardware acceleration keys:
- `NSHighResolutionCapable: true` enables Retina sub-pixel rendering.
- `CADisableMinimumFrameDurationOnPhone: true` unlocks 120Hz ProMotion refresh rates on Apple Silicon displays.
- `NSSupportsAutomaticGraphicsSwitching: true` allows dynamic switching between integrated and discrete GPUs.
- `LSMinimumSystemVersion: 12.0` establishes baseline compatibility with macOS Monterey through macOS Sonoma/Sequoia.

### 3.3 Disk Image Creation (`.dmg`)

Reddart packages a mountable UDZO-compressed disk image using Apple's `hdiutil`:
```bash
./scripts/package_mac.sh --skip-build --create-dmg
```
The packaging pipeline stages `Reddart.app` alongside a symbolic link to `/Applications`, presenting the user with the standard drag-and-drop installation experience.

### 3.4 Hardened Runtime Codesign & Apple Notarization CLI

For production distribution outside the Mac App Store, Reddart integrates with the Apple Developer CLI (`xcrun notarytool` and `stapler`):

```bash
# 1. Sign all dylibs and binaries with hardened runtime and secure timestamp
codesign --force --deep --verify --verbose \
    --options runtime --timestamp \
    --sign "Developer ID Application: Your Studio (TEAM_ID)" \
    ready_to_use/mac/Reddart.app

# 2. Package and sign the DMG disk image
codesign --force --verify --verbose \
    --sign "Developer ID Application: Your Studio (TEAM_ID)" \
    ready_to_use/mac/Reddart-v1.1.0-macOS-Universal.dmg

# 3. Submit to Apple Notarization Service
xcrun notarytool submit ready_to_use/mac/Reddart-v1.1.0-macOS-Universal.dmg \
    --keychain-profile "notarytool-profile" --wait

# 4. Staple ticket to the disk image and application bundle
xcrun stapler staple ready_to_use/mac/Reddart-v1.1.0-macOS-Universal.dmg
xcrun stapler staple ready_to_use/mac/Reddart.app
```

---

## 4. Linux Distribution Architecture

Reddart supports three primary Linux deployment targets: FreeDesktop Flatpak, standalone AppImage, and deterministic portable tarballs.

### 4.1 FreeDesktop Flatpak (`app.reddart.Reddart.yaml`)

The Flatpak manifest conforms to the latest FreeDesktop Platform `24.08` specification:

```yaml
app-id: app.reddart.Reddart
runtime: org.freedesktop.Platform
runtime-version: '24.08'
sdk: org.freedesktop.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
command: reddart

finish-args:
  # Windowing
  - --share=ipc
  - --socket=fallback-x11
  - --socket=wayland

  # Direct GPU rendering
  - --device=dri

  # Low-latency Audio
  - --socket=pulseaudio
  - --filesystem=xdg-run/pipewire-0

  # Media Storage Access
  - --filesystem=host

  # Native Wayland Fractional Scaling
  - --env=GDK_BACKEND=wayland,x11
  - --env=WINIT_UNIX_BACKEND=wayland
```

### 4.2 Linux AppImage Architecture (`AppRun`)

The AppImage bundle encapsulates dependencies and detects Wayland display environments at launch via `platforms/linux/AppRun`:

```bash
# Auto-detect native Wayland session for fractional scaling
if [ -n "${WAYLAND_DISPLAY:-}" ] || [ "${XDG_SESSION_TYPE:-}" = "wayland" ]; then
    export WINIT_UNIX_BACKEND="wayland"
    export GDK_BACKEND="wayland,x11"
    export QT_QPA_PLATFORM="wayland;xcb"
fi

export LD_LIBRARY_PATH="${APPDIR}/usr/lib:${APPDIR}/usr/lib64:${LD_LIBRARY_PATH:-}"
export XDG_DATA_DIRS="${APPDIR}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "${APPDIR}/usr/bin/reddart" "$@"
```

### 4.3 FreeDesktop Compliance

Reddart provides verified desktop assets compliant with FreeDesktop specifications:
- **Desktop Entry (`platforms/linux/reddart.desktop`):** FreeDesktop 1.5 specification compliant, with MIME bindings for MP4, QuickTime, MKV, WebM, and WAV, and desktop quick actions (`NewProject`, `Benchmark`).
- **AppStream Metainfo (`platforms/linux/app.reddart.Reddart.metainfo.xml`):** Provides structured metadata, release notes, Open Age Rating Service (OARS 1.1) rating, and component identification for GNOME Software and KDE Discover.
- **Hicolor SVG Icon (`platforms/linux/reddart.svg`):** Scalable vector icon rendered across GNOME, KDE Plasma, and tiling window managers.

---

## 5. Bit-for-Bit Deterministic Reproducibility

To prevent software supply-chain attacks and ensure cryptographic transparency:
- **Clamped Timestamps:** Tarball packaging clamps timestamps to `2026-01-01 00:00:00 UTC` (`--mtime`).
- **Sorted Directory Order:** Entries are archived in strict lexicographical order (`--sort=name`).
- **Ownership Normalization:** User and group ownership are normalized (`--owner=0 --group=0 --numeric-owner`).
- **Bit-for-Bit Verification:** Running `./scripts/package_linux.sh --skip-build --verify-reproducibility` compiles two independent passes in isolated target directories and asserts identical SHA-256 digests.

---

## 6. Packaging Verification Matrix

Automated verification is enforced by `crates/rd-core/tests/cross_platform_packaging_test.rs`:

| Test Case | Scope | Pass Criteria |
|---|---|---|
| `test_linux_flatpak_manifest_structure_and_permissions` | Sandboxing & Permissions | Wayland, DRI, PulseAudio, and host filesystem sockets granted |
| `test_linux_apprun_wrapper_script` | AppImage Wrapper | Wayland fractional scaling exports, LD_LIBRARY_PATH, and exec handoff |
| `test_linux_desktop_entry_specification` | FreeDesktop Entry | Version 1.5 syntax, categories, MIME types, and action stubs |
| `test_linux_appstream_metainfo` | AppStream XML | OARS 1.1 rating, release notes, license strings, and component ID |
| `test_macos_packaging_scripts_dmg_and_notarization` | macOS Packaging | `hdiutil` DMG parameters, hardened runtime codesign, and notarization CLI |
| `test_linux_packaging_scripts_appimage_and_reproducibility` | Linux Packaging | AppDir staging, reproducible tarball parameters, and manifest structure |
