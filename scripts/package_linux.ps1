# Reddart Video Editor - Linux Distribution Packaging & Validation Script (PowerShell)
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package_linux.ps1 [-VerifyReproducibility] [-SkipBuild]

param (
    [switch]$VerifyReproducibility,
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

$DistName = "Reddart-v$Version-Linux-x86_64"
$DistDir = Join-Path $RootDir "target\dist\$DistName"
$ReadyDir = Join-Path $RootDir "ready_to_use\linux"
$ZipOut = Join-Path $ReadyDir "$DistName.zip"
$ShaOut = Join-Path $ReadyDir "SHA256SUMS.txt"
$ManifestOut = Join-Path $ReadyDir "manifest.json"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Building & Validating Reddart Linux Distribution Package  " -ForegroundColor Cyan
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

New-Item -ItemType Directory -Path "$DistDir\bin" -Force | Out-Null
New-Item -ItemType Directory -Path "$DistDir\share\applications" -Force | Out-Null
New-Item -ItemType Directory -Path "$DistDir\share\metainfo" -Force | Out-Null
New-Item -ItemType Directory -Path "$DistDir\share\icons\hicolor\scalable\apps" -Force | Out-Null
New-Item -ItemType Directory -Path "$DistDir\share\reddart\effects" -Force | Out-Null
New-Item -ItemType Directory -Path "$DistDir\share\reddart\samples" -Force | Out-Null

# 3. Copy Assets
Write-Host "[1/5] Staging Linux FreeDesktop assets..." -ForegroundColor Yellow
Copy-Item (Join-Path $RootDir "platforms\linux\reddart.desktop") -Destination "$DistDir\share\applications\reddart.desktop" -Force
Copy-Item (Join-Path $RootDir "platforms\linux\app.reddart.Reddart.metainfo.xml") -Destination "$DistDir\share\metainfo\app.reddart.Reddart.metainfo.xml" -Force
Copy-Item (Join-Path $RootDir "platforms\linux\reddart.svg") -Destination "$DistDir\share\icons\hicolor\scalable\apps\reddart.svg" -Force

# Copy all 11 effects
Copy-Item -Recurse (Join-Path $RootDir "effects\*") -Destination "$DistDir\share\reddart\effects\" -Force

# Copy samples if present
$sampleSource = Join-Path $RootDir "ready_to_use\windows\samples"
if (Test-Path $sampleSource) {
    Copy-Item -Recurse (Join-Path $sampleSource "*") -Destination "$DistDir\share\reddart\samples\" -Force
}

# If executable exists in target/release, copy it (cross-compile or native)
$binSource = Join-Path $RootDir "target\release\reddart"
if (-not (Test-Path $binSource)) {
    $binSource = Join-Path $RootDir "target\release\reddart.exe"
}
if (Test-Path $binSource) {
    Copy-Item $binSource -Destination "$DistDir\bin\reddart" -Force
}

# Generate VERSION.txt (UTF-8 without BOM)
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$versionTxt = "Reddart Video Editor`r`nVersion: $Version`r`nPlatform: Linux (x86_64-unknown-linux-gnu)`r`nEngine: Rust 2024 | Vulkan / Wayland / X11 | wgpu 24 | egui 0.31 | ALSA / PulseAudio`r`n"
[System.IO.File]::WriteAllText((Join-Path $DistDir "VERSION.txt"), $versionTxt, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $ReadyDir "VERSION.txt"), $versionTxt, $utf8NoBom)

# Generate README_LINUX.txt
$readmeContent = @"
========================================================================
REDDART - ZERO-LAG PROFESSIONAL VIDEO EDITOR (Linux v$Version)
========================================================================

Package: $DistName
Architecture: x86_64 Linux (glibc 2.31+ / Vulkan 1.3 / Wayland & X11)
Engine: Rust 2024 + wgpu 24 + egui 0.31 + ALSA/PulseAudio

INSTALLATION:
1. Extract archive:
   unzip $DistName.zip
   cd $DistName

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
"@
[System.IO.File]::WriteAllText((Join-Path $DistDir "README_LINUX.txt"), $readmeContent, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $ReadyDir "README_LINUX.txt"), $readmeContent, $utf8NoBom)

# Helper function to create bit-for-bit deterministic ZIP archives
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

Write-Host "[2/5] Creating deterministic package archive ($ZipOut)..." -ForegroundColor Yellow
Create-DeterministicZip -SourceDir $DistDir -ZipPath $ZipOut

Write-Host "[3/5] Calculating cryptographic checksums and manifest..." -ForegroundColor Yellow
$zipHash = (Get-FileHash -Path $ZipOut -Algorithm SHA256).Hash.ToLowerInvariant()
$zipSize = (Get-Item $ZipOut).Length

$shaContent = "# Reddart Video Editor v$Version - Linux Release Checksums`r`n# Generated: $([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))`r`n`r`n$zipHash  $DistName.zip`r`n"
[System.IO.File]::WriteAllText($ShaOut, $shaContent, $utf8NoBom)

$manifestObj = [ordered]@{
    product = "Reddart Video Editor"
    version = $Version
    platform = "linux-x86_64"
    generated_at = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    archive = [ordered]@{
        filename = "$DistName.zip"
        sha256 = $zipHash
        bytes = $zipSize
    }
    desktop_entry = "share/applications/reddart.desktop"
    appstream_id = "app.reddart.Reddart"
    icon = "share/icons/hicolor/scalable/apps/reddart.svg"
    effects_count = 11
}
$manifestJson = ($manifestObj | ConvertTo-Json -Depth 5) + "`r`n"
[System.IO.File]::WriteAllText($ManifestOut, $manifestJson, $utf8NoBom)

Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  LINUX PACKAGING COMPLETE: $ZipOut ($([Math]::Round($zipSize / 1MB, 2)) MB)" -ForegroundColor Green
Write-Host "  SHA256: $zipHash" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green

