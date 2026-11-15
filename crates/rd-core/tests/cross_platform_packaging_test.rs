//! Automated Cross-Platform Distribution Packaging Verification Test Suite.
//!
//! Validates:
//! - FreeDesktop Flatpak manifest structure, sandbox isolation, and Wayland/DRI permissions.
//! - Linux AppImage `AppRun` wrapper script with Wayland fractional scaling detection.
//! - Linux `.desktop` file conformity to Desktop Entry Specification 1.5.
//! - AppStream `app.reddart.Reddart.metainfo.xml` metadata compliance.
//! - macOS `.app` bundle, DMG disk image generation, hardened runtime, and Apple Notarization CLI.
//! - Packaging scripts (`package_linux.sh`, `package_mac.sh`) for deterministic and reproducible builds.

use std::fs;
use std::path::PathBuf;

fn find_workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // rd-core is in crates/rd-core -> 2 levels up
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to locate workspace root")
        .to_path_buf()
}

#[test]
fn test_linux_flatpak_manifest_structure_and_permissions() {
    let root = find_workspace_root();
    let flatpak_path = root.join("platforms/linux/app.reddart.Reddart.yaml");
    assert!(
        flatpak_path.exists(),
        "Flatpak manifest missing: {:?}",
        flatpak_path
    );

    let content = fs::read_to_string(&flatpak_path).expect("Failed to read Flatpak manifest");

    // 1. Identification & Runtime
    assert!(content.contains("app-id: app.reddart.Reddart"));
    assert!(content.contains("runtime: org.freedesktop.Platform"));
    assert!(content.contains("runtime-version: '24.08'"));
    assert!(content.contains("sdk: org.freedesktop.Sdk"));
    assert!(content.contains("command: reddart"));

    // 2. Sandboxing & Windowing permissions
    assert!(
        content.contains("- --socket=wayland"),
        "Must grant Wayland socket"
    );
    assert!(
        content.contains("- --socket=fallback-x11"),
        "Must grant X11 fallback"
    );
    assert!(
        content.contains("- --device=dri"),
        "Must grant DRI for Vulkan hardware acceleration"
    );
    assert!(
        content.contains("- --socket=pulseaudio"),
        "Must grant PulseAudio socket for low-latency audio"
    );
    assert!(
        content.contains("- --filesystem=host"),
        "Must grant host filesystem access for media files"
    );

    // 3. Wayland fractional scaling environment
    assert!(content.contains("- --env=WINIT_UNIX_BACKEND=wayland"));
    assert!(content.contains("- --env=GDK_BACKEND=wayland,x11"));

    // 4. Module build instructions
    assert!(content.contains("name: reddart"));
    assert!(content.contains("cargo build --release --locked -p reddart-windows"));
    assert!(content.contains("install -Dm755 target/release/reddart /app/bin/reddart"));
    assert!(content.contains("install -Dm644 platforms/linux/reddart.desktop /app/share/applications/app.reddart.Reddart.desktop"));
    assert!(content.contains("install -Dm644 platforms/linux/app.reddart.Reddart.metainfo.xml /app/share/metainfo/app.reddart.Reddart.metainfo.xml"));
    assert!(content.contains("install -Dm644 platforms/linux/reddart.svg /app/share/icons/hicolor/scalable/apps/app.reddart.Reddart.svg"));
}

#[test]
fn test_linux_apprun_wrapper_script() {
    let root = find_workspace_root();
    let apprun_path = root.join("platforms/linux/AppRun");
    assert!(
        apprun_path.exists(),
        "AppRun script missing: {:?}",
        apprun_path
    );

    let content = fs::read_to_string(&apprun_path).expect("Failed to read AppRun script");

    // 1. Shebang & Environment
    assert!(content.starts_with("#!/usr/bin/env bash") || content.starts_with("#!/bin/bash"));
    assert!(content.contains("export APPDIR="));

    // 2. Wayland Fractional Scaling Detection
    assert!(content.contains("WAYLAND_DISPLAY"));
    assert!(content.contains("XDG_SESSION_TYPE"));
    assert!(content.contains("WINIT_UNIX_BACKEND"));
    assert!(content.contains("GDK_BACKEND"));

    // 3. Dynamic linker and data directory configuration
    assert!(content.contains("export LD_LIBRARY_PATH="));
    assert!(content.contains("export XDG_DATA_DIRS="));
    assert!(content.contains("REDDART_EFFECTS_DIR"));

    // 4. Execution handoff
    assert!(content.contains("exec \"$BIN\" \"$@\""));
}

#[test]
fn test_linux_desktop_entry_specification() {
    let root = find_workspace_root();
    let desktop_path = root.join("platforms/linux/reddart.desktop");
    assert!(
        desktop_path.exists(),
        "reddart.desktop missing: {:?}",
        desktop_path
    );

    let content = fs::read_to_string(&desktop_path).expect("Failed to read reddart.desktop");

    assert!(content.contains("[Desktop Entry]"));
    assert!(content.contains("Type=Application"));
    assert!(content.contains("Name=Reddart"));
    assert!(content.contains("Exec=reddart %F"));
    assert!(content.contains("Icon=reddart"));
    assert!(content.contains("Terminal=false"));
    assert!(content.contains("Categories=AudioVideo;Video;AudioVideoEditing;"));
    assert!(content.contains("video/mp4"));
    assert!(content.contains("video/quicktime"));
    assert!(content.contains("video/x-matroska"));
    assert!(content.contains("Actions=NewProject;Benchmark;"));
    assert!(content.contains("[Desktop Action NewProject]"));
    assert!(content.contains("[Desktop Action Benchmark]"));
}

#[test]
fn test_linux_appstream_metainfo() {
    let root = find_workspace_root();
    let metainfo_path = root.join("platforms/linux/app.reddart.Reddart.metainfo.xml");
    assert!(
        metainfo_path.exists(),
        "metainfo XML missing: {:?}",
        metainfo_path
    );

    let content = fs::read_to_string(&metainfo_path).expect("Failed to read metainfo XML");

    assert!(content.contains("<component type=\"desktop-application\">"));
    assert!(content.contains("<id>app.reddart.Reddart</id>"));
    assert!(content.contains("<metadata_license>CC0-1.0</metadata_license>"));
    assert!(content.contains("<project_license>MIT OR Apache-2.0</project_license>"));
    assert!(content.contains("<name>Reddart</name>"));
    assert!(content.contains("<launchable type=\"desktop-id\">reddart.desktop</launchable>"));
    assert!(content.contains("<content_rating type=\"oars-1.1\" />"));
}

#[test]
fn test_macos_packaging_scripts_dmg_and_notarization() {
    let root = find_workspace_root();
    let pkg_mac = root.join("scripts/package_mac.sh");
    assert!(pkg_mac.exists(), "package_mac.sh missing: {:?}", pkg_mac);

    let content = fs::read_to_string(&pkg_mac).expect("Failed to read package_mac.sh");

    // 1. Argument parsing
    assert!(content.contains("--create-dmg"));
    assert!(content.contains("--sign-identity"));
    assert!(content.contains("--notarize-profile"));

    // 2. Info.plist configuration
    assert!(content.contains("CFBundleIdentifier"));
    assert!(content.contains("app.reddart.mac"));
    assert!(content.contains("CFBundleExecutable"));
    assert!(content.contains("LSMinimumSystemVersion"));
    assert!(content.contains("NSHighResolutionCapable"));
    assert!(content.contains("CADisableMinimumFrameDurationOnPhone"));

    // 3. DMG creation with Applications drag-and-drop symlink
    assert!(content.contains("hdiutil create"));
    assert!(content.contains("-volname \"Reddart\""));
    assert!(content.contains("ln -s /Applications"));

    // 4. Hardened Runtime Codesign & Apple Notarization CLI
    assert!(content.contains("codesign"));
    assert!(content.contains("--options runtime --timestamp"));
    assert!(content.contains("xcrun notarytool submit"));
    assert!(content.contains("xcrun stapler staple"));
}

#[test]
fn test_linux_packaging_scripts_appimage_and_reproducibility() {
    let root = find_workspace_root();
    let pkg_linux = root.join("scripts/package_linux.sh");
    assert!(
        pkg_linux.exists(),
        "package_linux.sh missing: {:?}",
        pkg_linux
    );

    let content = fs::read_to_string(&pkg_linux).expect("Failed to read package_linux.sh");

    // 1. Argument parsing
    assert!(content.contains("--appimage"));
    assert!(content.contains("--flatpak"));
    assert!(content.contains("--skip-build"));
    assert!(content.contains("--verify-reproducibility"));

    // 2. AppDir staging
    assert!(content.contains("Reddart.AppDir"));
    assert!(content.contains("AppRun"));
    assert!(content.contains("reddart.desktop"));
    assert!(content.contains("reddart.svg"));

    // 3. FreeDesktop deterministic tarball with clamped mtime
    assert!(content.contains("tar --sort=name"));
    assert!(content.contains("--mtime="));
    assert!(content.contains("--owner=0 --group=0"));

    // 4. Manifest generation
    assert!(content.contains("flatpak_manifest"));
    assert!(content.contains("appimage_apprun"));
}
