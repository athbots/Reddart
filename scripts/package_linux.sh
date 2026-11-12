#!/usr/bin/env bash
# Reddart Video Editor - Linux Standalone Packaging & Validation Pipeline
# Usage: ./scripts/package_linux.sh [--skip-build] [--verify-reproducibility]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

SKIP_BUILD=0
VERIFY_REPRODUCIBILITY=0

for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        --verify-reproducibility) VERIFY_REPRODUCIBILITY=1 ;;
        *) echo "Unknown option: $arg"; exit 1 ;;
    esac
done

echo "=========================================================="
echo "  REDDART: LINUX STANDALONE PACKAGING & VALIDATION        "
echo "=========================================================="

# 1. Authoritative Version Extraction from workspace Cargo.toml
VERSION=$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/version[[:space:]]*=[[:space:]]*"([^"]+)"/\1/')
if [ -z "${VERSION}" ]; then
    echo "ERROR: Failed to extract workspace version from Cargo.toml" >&2
    exit 1
fi

DIST_NAME="Reddart-v${VERSION}-Linux-x86_64"
DIST_DIR="${ROOT_DIR}/target/dist/${DIST_NAME}"
READY_DIR="${ROOT_DIR}/ready_to_use/linux"
TAR_OUT="${READY_DIR}/${DIST_NAME}.tar.gz"
SHA_OUT="${READY_DIR}/SHA256SUMS.txt"
MANIFEST_OUT="${READY_DIR}/manifest.json"

echo "Authoritative Version: v${VERSION} (from Cargo.toml)"
echo "Target Package:        ${DIST_NAME}"
echo "Output Directory:      ${READY_DIR}"
echo "=========================================================="

# 2. Build Release Executable
if [ "${SKIP_BUILD}" -eq 0 ]; then
    echo "[1/6] Building locked release binary (cargo build --release --locked -p reddart-windows)..."
    cargo build --release --locked -p reddart-windows
fi

BIN_SOURCE="${ROOT_DIR}/target/release/reddart"
if [ ! -f "${BIN_SOURCE}" ]; then
    echo "ERROR: Compiled executable not found at ${BIN_SOURCE}" >&2
    exit 1
fi

# 3. Clean and Stage Linux Distribution Tree
echo "[2/6] Staging FreeDesktop Linux directory layout..."
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}/bin"
mkdir -p "${DIST_DIR}/share/applications"
mkdir -p "${DIST_DIR}/share/metainfo"
mkdir -p "${DIST_DIR}/share/icons/hicolor/scalable/apps"
mkdir -p "${DIST_DIR}/share/reddart/effects"
mkdir -p "${DIST_DIR}/share/reddart/samples"
mkdir -p "${READY_DIR}"

# Copy binary
cp "${BIN_SOURCE}" "${DIST_DIR}/bin/reddart"
chmod 755 "${DIST_DIR}/bin/reddart"

# Copy FreeDesktop specifications
cp "${ROOT_DIR}/platforms/linux/reddart.desktop" "${DIST_DIR}/share/applications/reddart.desktop"
cp "${ROOT_DIR}/platforms/linux/app.reddart.Reddart.metainfo.xml" "${DIST_DIR}/share/metainfo/app.reddart.Reddart.metainfo.xml"
cp "${ROOT_DIR}/platforms/linux/reddart.svg" "${DIST_DIR}/share/icons/hicolor/scalable/apps/reddart.svg"

# Copy all 9 WGSL shaders & effect manifests
cp -r "${ROOT_DIR}/effects/"* "${DIST_DIR}/share/reddart/effects/"

# Copy sample project and assets if present
if [ -d "${ROOT_DIR}/ready_to_use/windows/samples" ]; then
    cp -r "${ROOT_DIR}/ready_to_use/windows/samples/"* "${DIST_DIR}/share/reddart/samples/"
fi

# Generate VERSION.txt
cat <<EOF > "${DIST_DIR}/VERSION.txt"
Reddart Video Editor
Version: ${VERSION}
Platform: Linux (x86_64-unknown-linux-gnu)
Engine: Rust 2024 | Vulkan / Wayland / X11 | wgpu 24 | egui 0.31 | ALSA / PulseAudio
EOF

# Generate README_LINUX.txt
cat <<EOF > "${DIST_DIR}/README_LINUX.txt"
========================================================================
REDDART - ZERO-LAG PROFESSIONAL VIDEO EDITOR (Linux v${VERSION})
========================================================================

Package: ${DIST_NAME}
Architecture: x86_64 Linux (glibc 2.31+ / Vulkan 1.3 / Wayland & X11)
Engine: Rust 2024 + wgpu 24 + egui 0.31 + ALSA/PulseAudio

INSTALLATION:
1. Extract archive:
   tar -xzf ${DIST_NAME}.tar.gz
   cd ${DIST_NAME}

2. Run directly without installation:
   ./bin/reddart

3. Optional system-wide or user desktop install:
   mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
   cp bin/reddart ~/.local/bin/
   cp share/applications/reddart.desktop ~/.local/share/applications/
   cp share/icons/hicolor/scalable/apps/reddart.svg ~/.local/share/icons/hicolor/scalable/apps/
   update-desktop-database ~/.local/share/applications 2>/dev/null || true

SYSTEM REQUIREMENTS:
- GPU: Vulkan 1.2+ capable GPU (Intel Skylake+, AMD GCN 2+, NVIDIA Maxwell+)
- Audio: ALSA or PulseAudio / PipeWire ALSA compatibility layer
- Media Engine: FFmpeg 6.0+ in system PATH for multi-format decoding/encoding
========================================================================
EOF

# 4. Self-Validation
echo "[3/6] Running automated package self-validation..."
"${DIST_DIR}/bin/reddart" --version
"${DIST_DIR}/bin/reddart" --help > /dev/null

if command -v desktop-file-validate > /dev/null 2>&1; then
    echo "  -> Validating desktop entry with desktop-file-validate..."
    desktop-file-validate "${DIST_DIR}/share/applications/reddart.desktop"
    echo "  [OK] Desktop entry valid."
fi

# 5. Create Deterministic Tarball (.tar.gz)
echo "[4/6] Creating deterministic release archive (${TAR_OUT})..."
rm -f "${TAR_OUT}"

# Create tar with clamped mtime and sorted order for reproducible builds
tar --sort=name \
    --mtime='2026-01-01 00:00:00 UTC' \
    --owner=0 --group=0 --numeric-owner \
    -C "${ROOT_DIR}/target/dist" \
    -czf "${TAR_OUT}" "${DIST_NAME}"

# 6. Generate Checksums and Manifest
echo "[5/6] Generating SHA-256 checksums and package manifest..."
TAR_SHA256=$(sha256sum "${TAR_OUT}" | awk '{print $1}')
TAR_SIZE=$(stat -c%s "${TAR_OUT}" 2>/dev/null || stat -f%z "${TAR_OUT}")

cat <<EOF > "${SHA_OUT}"
# Reddart Video Editor v${VERSION} - Linux Release Checksums
# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

${TAR_SHA256}  ${DIST_NAME}.tar.gz
EOF

cat <<EOF > "${MANIFEST_OUT}"
{
  "product": "Reddart Video Editor",
  "version": "${VERSION}",
  "platform": "linux-x86_64",
  "generated_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "archive": {
    "filename": "${DIST_NAME}.tar.gz",
    "sha256": "${TAR_SHA256}",
    "bytes": ${TAR_SIZE}
  },
  "desktop_entry": "share/applications/reddart.desktop",
  "appstream_id": "app.reddart.Reddart",
  "icon": "share/icons/hicolor/scalable/apps/reddart.svg",
  "effects_count": 9
}
EOF

# Copy VERSION.txt and README_LINUX.txt to ready_to_use/linux/
cp "${DIST_DIR}/VERSION.txt" "${READY_DIR}/VERSION.txt"
cp "${DIST_DIR}/README_LINUX.txt" "${READY_DIR}/README_LINUX.txt"

# 7. Reproducibility Proof (if requested)
if [ "${VERIFY_REPRODUCIBILITY}" -eq 1 ]; then
    echo "[6/6] Verifying Bit-for-Bit Deterministic Reproducibility..."
    VERIFY_TAR="/tmp/reddart_verify.tar.gz"
    tar --sort=name \
        --mtime='2026-01-01 00:00:00 UTC' \
        --owner=0 --group=0 --numeric-owner \
        -C "${ROOT_DIR}/target/dist" \
        -czf "${VERIFY_TAR}" "${DIST_NAME}"
    
    VERIFY_SHA256=$(sha256sum "${VERIFY_TAR}" | awk '{print $1}')
    rm -f "${VERIFY_TAR}"
    if [ "${TAR_SHA256}" != "${VERIFY_SHA256}" ]; then
        echo "ERROR: Bit-for-bit reproducibility mismatch!" >&2
        exit 1
    fi
    echo "  [OK] Bit-for-bit deterministic reproducibility verified: ${TAR_SHA256}"
fi

echo "=========================================================="
echo "  LINUX PACKAGING COMPLETE: ${TAR_OUT} (${TAR_SIZE} bytes)"
echo "=========================================================="

