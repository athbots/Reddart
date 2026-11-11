# Reddart Video Editor - Windows Distribution Packaging & Validation Script
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package_windows.ps1

$ErrorActionPreference = "Stop"

$Version = "0.2.1"
$DistName = "Reddart-v$Version-Windows-x86_64"
$RootDir = (Get-Item $PSScriptRoot).Parent.FullName
$TargetRelease = Join-Path $RootDir "target\release\reddart.exe"
$DistDir = Join-Path $RootDir "target\dist\$DistName"
$ZipOut = Join-Path $RootDir "ready_to_use\windows\$DistName.zip"
$ShaOut = Join-Path $RootDir "ready_to_use\windows\SHA256SUMS.txt"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Building & Validating Reddart Windows Distribution Package" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Target:  $DistName" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# Step 1: Compile optimized release binary
Write-Host "[1/6] Building release binary via Cargo..." -ForegroundColor Yellow
Push-Location $RootDir
cargo build --release -p reddart-windows
Pop-Location

if (-not (Test-Path $TargetRelease)) {
    Write-Error "Release binary not found at $TargetRelease"
    exit 1
}

# Step 2: Prepare clean staging directory
Write-Host "[2/6] Staging distribution files..." -ForegroundColor Yellow
if (Test-Path $DistDir) {
    Remove-Item -Recurse -Force $DistDir
}
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $DistDir "samples") | Out-Null

# Copy primary binary and metadata
Copy-Item $TargetRelease -Destination (Join-Path $DistDir "Reddart.exe") -Force
Copy-Item (Join-Path $RootDir "ready_to_use\windows\VERSION.txt") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "ready_to_use\windows\README_QUICKSTART.txt") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "ready_to_use\windows\README_WINDOWS.txt") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "LICENSE") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "LICENSE-MIT") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "LICENSE-APACHE") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "ready_to_use\windows\manifest.json") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "ready_to_use\windows\register_file_association.bat") -Destination $DistDir -Force
Copy-Item (Join-Path $RootDir "ready_to_use\windows\unregister_file_association.bat") -Destination $DistDir -Force

# Copy sample project and demo media
if (Test-Path (Join-Path $RootDir "ready_to_use\windows\samples\Demo_Project.reddart")) {
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\samples\Demo_Project.reddart") -Destination (Join-Path $DistDir "samples\Demo_Project.reddart") -Force
}
if (Test-Path (Join-Path $RootDir "ready_to_use\windows\Reddart_Master_Demo.mp4")) {
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\Reddart_Master_Demo.mp4") -Destination (Join-Path $DistDir "Reddart_Master_Demo.mp4") -Force
}

# Normalize file timestamps for reproducible archive generation
$DeterministicDate = [DateTime]::Parse("2026-01-01T00:00:00Z").ToUniversalTime()
Get-ChildItem -Path $DistDir -Recurse | ForEach-Object {
    $_.LastWriteTimeUtc = $DeterministicDate
}

# Step 3: Copy binary to ready_to_use/windows/Reddart.exe
Write-Host "[3/6] Syncing ready_to_use\windows\Reddart.exe..." -ForegroundColor Yellow
Copy-Item $TargetRelease -Destination (Join-Path $RootDir "ready_to_use\windows\Reddart.exe") -Force

# Step 4: Validate staged distribution in isolation
Write-Host "[4/6] Validating staged binary in clean directory..." -ForegroundColor Yellow
$StagedExe = Join-Path $DistDir "Reddart.exe"
& $StagedExe --version
if ($LASTEXITCODE -ne 0) {
    Write-Error "Staged binary failed --version check!"
    exit 1
}

& $StagedExe templates
if ($LASTEXITCODE -ne 0) {
    Write-Error "Staged binary failed templates check!"
    exit 1
}

# Step 5: Create distributable ZIP archive
Write-Host "[5/6] Creating release ZIP archive: $ZipOut..." -ForegroundColor Yellow
if (Test-Path $ZipOut) {
    Remove-Item -Force $ZipOut
}
Compress-Archive -Path "$DistDir\*" -DestinationPath $ZipOut -CompressionLevel Optimal

# Step 6: Generate and synchronize SHA-256 Checksums across all documentation
Write-Host "[6/6] Computing and synchronizing SHA-256 checksums..." -ForegroundColor Yellow
$Hash = Get-FileHash -Path $ZipOut -Algorithm SHA256
$ExeHash = Get-FileHash -Path (Join-Path $RootDir "ready_to_use\windows\Reddart.exe") -Algorithm SHA256

$ZipHex = $Hash.Hash.ToLower()
$ExeHex = $ExeHash.Hash.ToLower()

$ChecksumContent = @"
# Reddart Video Editor v$Version - Release Checksums
$ZipHex  $DistName.zip
$ExeHex  Reddart.exe
"@

Set-Content -Path $ShaOut -Value $ChecksumContent -Encoding utf8

# Synchronize manifest.json
$ManifestPath = Join-Path $RootDir "ready_to_use\windows\manifest.json"
if (Test-Path $ManifestPath) {
    $Manifest = Get-Content $ManifestPath -Raw | ConvertFrom-Json
    $Manifest | Add-Member -NotePropertyName "sha256_executable" -NotePropertyValue $ExeHex -Force
    $Manifest | Add-Member -NotePropertyName "sha256_archive" -NotePropertyValue $ZipHex -Force
    $ManifestJson = $Manifest | ConvertTo-Json -Depth 10
    Set-Content -Path $ManifestPath -Value $ManifestJson -Encoding utf8
}

# Synchronize docs/RELEASE_NOTES_AND_LIMITATIONS.md
$RelNotesPath = Join-Path $RootDir "docs\RELEASE_NOTES_AND_LIMITATIONS.md"
if (Test-Path $RelNotesPath) {
    $RelNotes = Get-Content $RelNotesPath -Raw
    $BinLine = '**Binary Checksum (SHA-256):** `{0}`' -f $ExeHex
    $ArcLine = '**Archive Checksum (SHA-256):** `{0}`' -f $ZipHex

    $RelNotes = [regex]::Replace($RelNotes, '(?m)^\*\*Binary Checksum \(SHA-256\):\*\* `[a-fA-F0-9]+`', $BinLine)
    if ($RelNotes -notmatch '\*\*Archive Checksum \(SHA-256\):\*\*') {
        $RelNotes = $RelNotes -replace '(?m)(\*\*Binary Checksum \(SHA-256\):\*\* .*\r?\n)', "`$1$ArcLine`r`n"
    } else {
        $RelNotes = [regex]::Replace($RelNotes, '(?m)^\*\*Archive Checksum \(SHA-256\):\*\* `[a-fA-F0-9]+`', $ArcLine)
    }
    Set-Content -Path $RelNotesPath -Value $RelNotes -Encoding utf8
}

# Synchronize website/index.html
$WebPath = Join-Path $RootDir "website\index.html"
if (Test-Path $WebPath) {
    $WebContent = Get-Content $WebPath -Raw
    $HashBox = "<div class=`"hash-box`">`r`n            <code>Archive SHA-256: {0}<br>Binary SHA-256: {1}</code>`r`n          </div>" -f $ZipHex, $ExeHex
    $WebContent = [regex]::Replace($WebContent, '(?s)<div class="hash-box">.*?</div>', $HashBox)
    Set-Content -Path $WebPath -Value $WebContent -Encoding utf8
}

Write-Host "==========================================================" -ForegroundColor Green
Write-Host "Distribution Packaging Successfully Completed!" -ForegroundColor Green
Write-Host "Archive:  $ZipOut" -ForegroundColor Green
Write-Host "Size:     $([math]::Round((Get-Item $ZipOut).Length / 1MB, 2)) MB" -ForegroundColor Green
Write-Host "SHA-256:  $($Hash.Hash)" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green

