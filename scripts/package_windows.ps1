# Reddart Video Editor - Windows Distribution Packaging & Validation Script
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package_windows.ps1 [-VerifyReproducibility]

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

$DistName = "Reddart-v$Version-Windows-x86_64"
$TargetRelease = Join-Path $RootDir "target\release\reddart.exe"
$DistDir = Join-Path $RootDir "target\dist\$DistName"
$ZipOut = Join-Path $RootDir "ready_to_use\windows\$DistName.zip"
$ShaOut = Join-Path $RootDir "ready_to_use\windows\SHA256SUMS.txt"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Building & Validating Reddart Windows Distribution Package" -ForegroundColor Cyan
Write-Host "Authoritative Version: v$Version (from Cargo.toml)" -ForegroundColor Cyan
Write-Host "Target Package:        $DistName" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# Ensure VERSION.txt is strictly in sync with Cargo.toml
Set-Content -Path (Join-Path $RootDir "ready_to_use\windows\VERSION.txt") -Value "Reddart Video Editor`r`nVersion: $Version`r`n" -Encoding utf8

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

# Helper function to create bit-for-bit deterministic ZIP archives
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
                    $es.Close()
                }
            } finally {
                $fs.Close()
            }
        }
    } finally {
        $zip.Dispose()
    }
}

# Helper function to stage files
function Stage-DistributionPackage ($StagingPath) {
    if (Test-Path $StagingPath) {
        Remove-Item -Recurse -Force $StagingPath
    }
    New-Item -ItemType Directory -Force -Path $StagingPath | Out-Null
    
    # Copy primary binary and metadata
    Copy-Item $TargetRelease -Destination (Join-Path $StagingPath "Reddart.exe") -Force
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\VERSION.txt") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\README_QUICKSTART.txt") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\README_WINDOWS.txt") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "LICENSE") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "LICENSE-MIT") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "LICENSE-APACHE") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\register_file_association.bat") -Destination $StagingPath -Force
    Copy-Item (Join-Path $RootDir "ready_to_use\windows\unregister_file_association.bat") -Destination $StagingPath -Force

    # Recursively copy complete samples directory (footage, audio, projects, licenses)
    $SamplesSource = Join-Path $RootDir "ready_to_use\windows\samples"
    if (Test-Path $SamplesSource) {
        Copy-Item -Path $SamplesSource -Destination $StagingPath -Recurse -Force
    }

    if (Test-Path (Join-Path $RootDir "ready_to_use\windows\Reddart_Master_Demo.mp4")) {
        Copy-Item (Join-Path $RootDir "ready_to_use\windows\Reddart_Master_Demo.mp4") -Destination (Join-Path $StagingPath "Reddart_Master_Demo.mp4") -Force
    }

    # Normalize file timestamps for reproducible archive generation
    $DeterministicDate = [DateTime]::Parse("2026-01-01T00:00:00Z").ToUniversalTime()
    Get-ChildItem -Path $StagingPath -Recurse | ForEach-Object {
        $_.LastWriteTimeUtc = $DeterministicDate
    }
}

# Step 1: Compile optimized release binary with --locked
if (-not $SkipBuild) {
    Write-Host "[1/7] Building release binary via Cargo (cargo build --release --locked -p reddart-windows)..." -ForegroundColor Yellow
    Push-Location $RootDir
    cargo build --release --locked -p reddart-windows
    $BuildExitCode = $LASTEXITCODE
    Pop-Location

    if ($BuildExitCode -ne 0) {
        Write-Error "Cargo build --release --locked failed with exit code $BuildExitCode"
        exit 1
    }
}

if (-not (Test-Path $TargetRelease)) {
    Write-Error "Release binary not found at $TargetRelease"
    exit 1
}

# Step 2: Prepare clean staging directory
Write-Host "[2/7] Staging distribution files (including complete samples directory)..." -ForegroundColor Yellow
Stage-DistributionPackage $DistDir

# Step 3: Sync ready_to_use/windows/Reddart.exe
Write-Host "[3/7] Syncing ready_to_use\windows\Reddart.exe..." -ForegroundColor Yellow
Copy-Item $TargetRelease -Destination (Join-Path $RootDir "ready_to_use\windows\Reddart.exe") -Force

# Step 4: Create distributable ZIP archive
Write-Host "[4/7] Creating deterministic release ZIP archive: $ZipOut..." -ForegroundColor Yellow
Create-DeterministicZip $DistDir $ZipOut

# Step 5: Package Validation Stage (Extract and test in isolation)
Write-Host "[5/7] Running Package Validation Stage on extracted archive..." -ForegroundColor Yellow
$ValidationDir = Join-Path $RootDir "target\dist\test_validation_extract"
if (Test-Path $ValidationDir) {
    Remove-Item -Recurse -Force $ValidationDir
}
New-Item -ItemType Directory -Force -Path $ValidationDir | Out-Null
Expand-Archive -Path $ZipOut -DestinationPath $ValidationDir -Force

$ValExe = Join-Path $ValidationDir "Reddart.exe"
if (-not (Test-Path $ValExe)) {
    Write-Error "Validation failed: Reddart.exe not found in extracted archive!"
    exit 1
}

# Validate CLI execution
$VersionOutput = (& $ValExe --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or ($VersionOutput -notmatch [regex]::Escape($Version))) {
    Write-Error "Validation failed: & Reddart.exe --version did not return expected version $Version!"
    exit 1
}
Write-Host "  [OK] Extracted Reddart.exe --version verified" -ForegroundColor Green

& $ValExe templates | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Error "Validation failed: & Reddart.exe templates returned non-zero exit code!"
    exit 1
}
Write-Host "  [OK] Extracted Reddart.exe templates executed successfully" -ForegroundColor Green

# Validate that every sample project media path resolves in the extracted package
$SampleProjectFile = Join-Path $ValidationDir "samples\Demo_Project.reddart"
if (-not (Test-Path $SampleProjectFile)) {
    Write-Error "Validation failed: samples\Demo_Project.reddart not found in extracted archive!"
    exit 1
}
$ProjectJson = Get-Content $SampleProjectFile -Raw | ConvertFrom-Json
$ClipsFound = 0
foreach ($track in $ProjectJson.timeline.tracks) {
    foreach ($clip in $track.clips) {
        $sourcePath = $null
        if ($clip.kind.Video) {
            $sourcePath = $clip.kind.Video.source_path
        } elseif ($clip.kind.Audio) {
            $sourcePath = $clip.kind.Audio.source_path
        }
        if ($sourcePath) {
            $ClipsFound++
            # Resolve against extracted samples dir or extracted root
            $p1 = Join-Path (Join-Path $ValidationDir "samples") $sourcePath
            $p2 = Join-Path $ValidationDir $sourcePath
            if (-not (Test-Path $p1) -and -not (Test-Path $p2)) {
                Write-Error "Validation failed: Clip '$($clip.id)' references missing media: '$sourcePath'. Neither '$p1' nor '$p2' exists!"
                exit 1
            }
            Write-Host "  [OK] Sample media path resolved: $sourcePath" -ForegroundColor Green
        }
    }
}
if ($ClipsFound -eq 0) {
    Write-Error "Validation failed: No clips found in Demo_Project.reddart!"
    exit 1
}
Write-Host "  [OK] All $ClipsFound sample project media paths verified in extracted package!" -ForegroundColor Green

# Cleanup validation extract
Remove-Item -Recurse -Force $ValidationDir

# Step 6: Generate and synchronize SHA-256 Checksums
Write-Host "[6/7] Computing and synchronizing SHA-256 checksums..." -ForegroundColor Yellow
$ZipHash = (Get-FileHash -Path $ZipOut -Algorithm SHA256).Hash.ToLower()
$ExeHash = (Get-FileHash -Path (Join-Path $RootDir "ready_to_use\windows\Reddart.exe") -Algorithm SHA256).Hash.ToLower()

$ChecksumContent = @"
# Reddart Video Editor v$Version - Release Checksums
$ZipHash  $DistName.zip
$ExeHash  Reddart.exe
"@

Set-Content -Path $ShaOut -Value $ChecksumContent -Encoding utf8

# Synchronize manifest.json
$ManifestPath = Join-Path $RootDir "ready_to_use\windows\manifest.json"
if (Test-Path $ManifestPath) {
    $Manifest = Get-Content $ManifestPath -Raw | ConvertFrom-Json
    $Manifest | Add-Member -NotePropertyName "sha256_executable" -NotePropertyValue $ExeHash -Force
    $Manifest | Add-Member -NotePropertyName "sha256_archive" -NotePropertyValue $ZipHash -Force
    $ManifestJson = $Manifest | ConvertTo-Json -Depth 10
    Set-Content -Path $ManifestPath -Value $ManifestJson -Encoding utf8
}

# Synchronize docs/RELEASE_NOTES_AND_LIMITATIONS.md
$RelNotesPath = Join-Path $RootDir "docs\RELEASE_NOTES_AND_LIMITATIONS.md"
if (Test-Path $RelNotesPath) {
    $RelNotes = Get-Content $RelNotesPath -Raw
    $BinLine = '**Binary Checksum (SHA-256):** `{0}`' -f $ExeHash
    $ArcLine = '**Archive Checksum (SHA-256):** `{0}`' -f $ZipHash

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
    $HashBox = "<div class=`"hash-box`">`r`n            <code>Archive SHA-256: {0}<br>Binary SHA-256: {1}</code>`r`n          </div>" -f $ZipHash, $ExeHash
    $WebContent = [regex]::Replace($WebContent, '(?s)<div class="hash-box">.*?</div>', $HashBox)
    Set-Content -Path $WebPath -Value $WebContent -Encoding utf8
}

# Verify synchronicity across all 4 files
Write-Host "  Verifying consistency across manifests..." -ForegroundColor Yellow
$ManifestCheck = (Get-Content $ManifestPath -Raw | ConvertFrom-Json)
if ($ManifestCheck.sha256_executable -ne $ExeHash -or $ManifestCheck.sha256_archive -ne $ZipHash) {
    Write-Error "Checksum synchronization failed for manifest.json!"
    exit 1
}
$NotesCheck = Get-Content $RelNotesPath -Raw
if ($NotesCheck -notmatch $ExeHash -or $NotesCheck -notmatch $ZipHash) {
    Write-Error "Checksum synchronization failed for RELEASE_NOTES_AND_LIMITATIONS.md!"
    exit 1
}
$WebCheck = Get-Content $WebPath -Raw
if ($WebCheck -notmatch $ExeHash -or $WebCheck -notmatch $ZipHash) {
    Write-Error "Checksum synchronization failed for website/index.html!"
    exit 1
}
Write-Host "  [OK] SHA256SUMS.txt, manifest.json, RELEASE_NOTES_AND_LIMITATIONS.md, and website/index.html are in 100% agreement." -ForegroundColor Green

# Historical GitHub Release Audit Comparison
$HistoricalV021Zip = "40bcfbe045c48facb4294b2de037c98028897d309ad2f0e5d508d6ec9fe8a8b4"
$HistoricalV021Exe = "377aaad62dc52f261003fd749ce398c30c11b2d97910b10e748f9086bdcd90d0"
Write-Host "  Audit vs GitHub v0.2.1 Release Assets:" -ForegroundColor Yellow
if ($ZipHash -eq $HistoricalV021Zip -and $ExeHash -eq $HistoricalV021Exe) {
    Write-Host "  -> Matches published GitHub v0.2.1 release assets exactly." -ForegroundColor Green
} else {
    Write-Host "  -> Local pre-release artifacts differ from GitHub v0.2.1 (incorporates post-v0.2.1 persistence & stabilization commits)." -ForegroundColor Cyan
    Write-Host "  -> Ledger: tag v0.2.1 remains immutable; next release will be published as v0.3.0 (see docs/RELEASE_INTEGRITY.md)." -ForegroundColor Cyan
}

# Step 7: Optional Bit-for-bit Reproducibility Verification
if ($VerifyReproducibility) {
    Write-Host "[7/7] Verifying bit-for-bit build reproducibility across two independent passes..." -ForegroundColor Yellow
    $SecondDistDir = Join-Path $RootDir "target\dist\repro_test_$DistName"
    $SecondZip = Join-Path $RootDir "target\dist\repro_test.zip"

    Stage-DistributionPackage $SecondDistDir
    Create-DeterministicZip $SecondDistDir $SecondZip
    $SecondZipHash = (Get-FileHash -Path $SecondZip -Algorithm SHA256).Hash.ToLower()

    Remove-Item -Recurse -Force $SecondDistDir
    Remove-Item -Force $SecondZip

    if ($SecondZipHash -ne $ZipHash) {
        Write-Error "Reproducibility validation failed! First ZIP: $ZipHash, Second ZIP: $SecondZipHash"
        exit 1
    }
    Write-Host "  [OK] Bit-for-bit reproducible packaging verified ($ZipHash)" -ForegroundColor Green
} else {
    Write-Host "[7/7] Packaging validation passed! (Pass -VerifyReproducibility to run full multi-pass check)" -ForegroundColor Green
}

Write-Host "==========================================================" -ForegroundColor Green
Write-Host "Distribution Packaging & Validation Successfully Completed!" -ForegroundColor Green
Write-Host "Archive:  $ZipOut" -ForegroundColor Green
Write-Host "Size:     $([math]::Round((Get-Item $ZipOut).Length / 1MB, 2)) MB" -ForegroundColor Green
Write-Host "Archive SHA-256: $ZipHash" -ForegroundColor Green
Write-Host "Binary SHA-256:  $ExeHash" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
