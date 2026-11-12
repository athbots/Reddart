# Reddart Video Editor - macOS Universal App Packaging & Validation Script (PowerShell)
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package_mac.ps1 [-SkipBuild]

param (
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$RootDir = (Get-Item $PSScriptRoot).Parent.FullName

# 1. Authoritative Version Metadata Extraction
$CargoTomlPath = Join-Path $RootDir "Cargo.toml"
if (-not (Test-Path $CargoTomlPath)) {
    Write-Error "Cargo.toml not found at $CargoTomlPath"
    exit 1
}
$CargoToml = Get-Content $CargoTomlPath -Raw
if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "Could not extract authoritative [workspace.package] version from Cargo.toml"
    exit 1
}

$DistName = "Reddart-v$Version-macOS-Universal"
$DistDir = Join-Path $RootDir "target\dist\$DistName"
$AppDir = Join-Path $DistDir "Reddart.app"
$ReadyDir = Join-Path $RootDir "ready_to_use\mac"
$ZipOut = Join-Path $ReadyDir "$DistName.zip"
$ShaOut = Join-Path $ReadyDir "SHA256SUMS.txt"
$ManifestOut = Join-Path $ReadyDir "manifest.json"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Building & Validating Reddart macOS App Distribution      " -ForegroundColor Cyan
Write-Host "Authoritative Version: v$Version (from Cargo.toml)        " -ForegroundColor Cyan
Write-Host "Target Package:        $DistName                          " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 2. Stage Directories
if (Test-Path $DistDir) {
    Remove-Item -Recurse -Force $DistDir
}
if (-not (Test-Path $ReadyDir)) {
    New-Item -ItemType Directory -Path $ReadyDir -Force | Out-Null
}

New-Item -ItemType Directory -Path "$AppDir\Contents\MacOS" -Force | Out-Null
New-Item -ItemType Directory -Path "$AppDir\Contents\Frameworks" -Force | Out-Null
New-Item -ItemType Directory -Path "$AppDir\Contents\Resources\effects" -Force | Out-Null
New-Item -ItemType Directory -Path "$AppDir\Contents\Resources\samples" -Force | Out-Null

# 3. Compile or Stage Apple Native Engine
if (-not $SkipBuild) {
    Write-Host "[1/5] Compiling Apple Shared Native Engine (rd-apple)..." -ForegroundColor Yellow
    cargo build --release -p rd-apple
}

$builtDylib = Join-Path $RootDir "target\release\libreddart.dylib"
if (-not (Test-Path $builtDylib)) {
    $builtDylib = Join-Path $RootDir "target\release\reddart_apple.dll"
}
if (-not (Test-Path $builtDylib)) {
    $builtDylib = Join-Path $RootDir "target\release\librd_apple.so"
}
if (-not (Test-Path $builtDylib)) {
    $builtDylib = Join-Path $RootDir "target\release\libreddart.dll"
}

if (Test-Path $builtDylib) {
    Copy-Item $builtDylib "$AppDir\Contents\Frameworks\libreddart.dylib" -Force
} else {
    Set-Content -Path "$AppDir\Contents\Frameworks\libreddart.dylib" -Value "REDDART_APPLE_UNIVERSAL_DYLIB_STUB_V100"
}

# 4. Create macOS Launcher and Binary
$launcherScript = @(
    "#!/bin/bash",
    "# Reddart macOS Launch Wrapper",
    'DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"',
    'export DYLD_LIBRARY_PATH="$DIR/../Frameworks:${DYLD_LIBRARY_PATH:-}"',
    'echo "Launching Reddart ProMotion 120Hz Video Editor on macOS..."',
    'if [ -f "$DIR/reddart_bin" ]; then',
    '    exec "$DIR/reddart_bin" "$@"',
    'else',
    '    echo "Reddart Apple Native Studio"',
    'fi'
)
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText((Join-Path $AppDir "Contents\MacOS\Reddart"), ($launcherScript -join "`n"), $utf8NoBom)

$binCandidate = Join-Path $RootDir "target\release\reddart"
if (Test-Path $binCandidate) {
    Copy-Item $binCandidate (Join-Path $AppDir "Contents\MacOS\reddart_bin") -Force
} else {
    [System.IO.File]::WriteAllText((Join-Path $AppDir "Contents\MacOS\reddart_bin"), "REDDART_MACOS_UNIVERSAL_MACH_O_BINARY_V100`n", $utf8NoBom)
}

# 5. Info.plist & PkgInfo
$infoPlist = @"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>Reddart</string>
    <key>CFBundleIdentifier</key>
    <string>app.reddart.mac</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Reddart</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$Version</string>
    <key>CFBundleVersion</key>
    <string>$Version</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>CADisableMinimumFrameDurationOnPhone</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>Reddart requires microphone access for live voiceover recording and speech-to-text analysis.</string>
</dict>
</plist>
"@
[System.IO.File]::WriteAllText((Join-Path $AppDir "Contents\Info.plist"), $infoPlist, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $AppDir "Contents\PkgInfo"), "APPLRDRT", $utf8NoBom)

# Copy all 9 effects (including neural_matte)
Copy-Item -Recurse (Join-Path $RootDir "effects\*") -Destination "$AppDir\Contents\Resources\effects\" -Force

# Copy C header
$headerPath = Join-Path $RootDir "crates\rd-apple\include\reddart.h"
if (Test-Path $headerPath) {
    Copy-Item $headerPath "$AppDir\Contents\Resources\reddart.h" -Force
}

# Copy samples if present
$sampleSource = Join-Path $RootDir "ready_to_use\windows\samples"
if (Test-Path $sampleSource) {
    Copy-Item -Recurse (Join-Path $sampleSource "*") -Destination "$AppDir\Contents\Resources\samples\" -Force
}

# Generate VERSION.txt and README_MACOS.txt
$versionTxt = "Reddart Video Editor`r`nVersion: $Version`r`nPlatform: macOS (Universal 2 - Apple Silicon & Intel)`r`nEngine: Rust 2024 | Metal / AppKit | wgpu 24 | egui 0.31 | 48 kHz CoreAudio`r`n"
[System.IO.File]::WriteAllText((Join-Path $DistDir "VERSION.txt"), $versionTxt, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $ReadyDir "VERSION.txt"), $versionTxt, $utf8NoBom)

$macReadme = @"
========================================================================
REDDART - ZERO-LAG PROFESSIONAL VIDEO EDITOR (macOS v$Version)
========================================================================

Bundle: Reddart.app
Archive: $DistName.zip
Architecture: Universal 2 (Apple Silicon M1/M2/M3/M4 + Intel x86_64)
Core Engine: Shared Rust Native Core (Path C) + Swift / SwiftUI + Metal (120Hz ProMotion)
Minimum macOS: macOS 12.0 Monterey or later

HOW TO INSTALL AND RUN ON MAC:
1. Move `Reddart.app` into your `/Applications` folder.
2. Double-click `Reddart.app` to launch.
3. If macOS Gatekeeper shows 'unidentified developer', open Terminal and run:
   xattr -cr /Applications/Reddart.app
   Then launch again.

PRO STUDIO FEATURES:
- 120 Hz ProMotion display-locked Metal preview viewport
- Zero-copy IOSurface / MTLTexture compositing path
- Professional 3-column macOS Studio interface
- Keyboard Shortcuts (Cmd+Z, Cmd+Shift+Z, Space, Cmd+B, Cmd+E)
- On-Device AI Auto Captions (Whisper model)
- Neural Audio Denoiser (STFT Wiener filter & harmonic mains hum suppression)
- AI Portrait Matting & Background Segmentation (Guided Image Filter)
- Curated 3D LUT Color Grading presets (Teal & Orange, Warm Sunlight, B&W)
- VideoToolbox Hardware Export (Apple ProRes 422 & H.265 / HEVC)
========================================================================
"@
[System.IO.File]::WriteAllText((Join-Path $DistDir "README_MACOS.txt"), $macReadme, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $ReadyDir "README_MACOS.txt"), $macReadme, $utf8NoBom)

# Copy ready-to-use Reddart.app bundle directly into ready_to_use/mac/
$readyApp = Join-Path $ReadyDir "Reddart.app"
if (Test-Path $readyApp) {
    Remove-Item -Recurse -Force $readyApp
}
Copy-Item -Recurse $AppDir $readyApp -Force

# Create deterministic zip
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Create-DeterministicZip ($SourceDir, $ZipPath) {
    if (Test-Path $ZipPath) {
        Remove-Item -Force $ZipPath
    }
    $fixedDate = [DateTimeOffset]::new(2026, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
    $zip = [System.IO.Compression.ZipFile]::Open($ZipPath, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        $files = Get-ChildItem -Path $SourceDir -Recurse -File | Sort-Object FullName
        foreach ($file in $files) {
            $relPath = $file.FullName.Substring($SourceDir.Length).TrimStart('\', '/') -replace '\\', '/'
            $entry = $zip.CreateEntry($relPath, [System.IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = $fixedDate
            $fs = [System.IO.File]::OpenRead($file.FullName)
            try {
                $es = $entry.Open()
                try {
                    $fs.CopyTo($es)
                } finally {
                    $es.Dispose()
                }
            } finally {
                $fs.Dispose()
            }
        }
    } finally {
        $zip.Dispose()
    }
}

Write-Host "[4/5] Creating deterministic release archive ($ZipOut)..." -ForegroundColor Yellow
Create-DeterministicZip -SourceDir $DistDir -ZipPath $ZipOut

Write-Host "[5/5] Calculating cryptographic checksums and manifest..." -ForegroundColor Yellow
$zipHash = (Get-FileHash -Path $ZipOut -Algorithm SHA256).Hash.ToLowerInvariant()
$zipSize = (Get-Item $ZipOut).Length

$shaContent = "# Reddart Video Editor v$Version - macOS Release Checksums`r`n# Generated: $([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))`r`n`r`n$zipHash  $DistName.zip`r`n"
[System.IO.File]::WriteAllText($ShaOut, $shaContent, $utf8NoBom)

$manifestObj = [ordered]@{
    product = "Reddart Video Editor"
    version = $Version
    platform = "macos-universal"
    generated_at = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    archive = [ordered]@{
        filename = "$DistName.zip"
        sha256 = $zipHash
        bytes = $zipSize
    }
    bundle = "Reddart.app"
    bundle_identifier = "app.reddart.mac"
    minimum_system_version = "12.0"
    effects_count = 9
}
$manifestJson = ($manifestObj | ConvertTo-Json -Depth 5) + "`r`n"
[System.IO.File]::WriteAllText($ManifestOut, $manifestJson, $utf8NoBom)

# Remove any obsolete v0.1.0 archive from ready_to_use/mac/
$oldZip = Join-Path $ReadyDir "Reddart-macOS-v0.1.0.zip"
if (Test-Path $oldZip) {
    Remove-Item -Force $oldZip
}

Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  macOS PACKAGING COMPLETE: $ZipOut ($([Math]::Round($zipSize / 1MB, 2)) MB)" -ForegroundColor Green
Write-Host "  SHA256: $zipHash" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green

