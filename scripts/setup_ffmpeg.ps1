# Reddart Video Editor — Clean Machine FFmpeg Setup Script
# Usage: powershell -ExecutionPolicy Bypass -File scripts\setup_ffmpeg.ps1

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Reddart Clean-Machine Environment Setup: FFmpeg & Tools    " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Check if FFmpeg and ffprobe are already discoverable on PATH
$ffmpegCmd = Get-Command "ffmpeg.exe" -ErrorAction SilentlyContinue
$ffprobeCmd = Get-Command "ffprobe.exe" -ErrorAction SilentlyContinue

if ($ffmpegCmd -and $ffprobeCmd) {
    Write-Host "[OK] FFmpeg already available: $($ffmpegCmd.Source)" -ForegroundColor Green
    Write-Host "[OK] FFprobe already available: $($ffprobeCmd.Source)" -ForegroundColor Green
    & ffmpeg -version | Select-Object -First 1
    & ffprobe -version | Select-Object -First 1
    exit 0
}

# 2. Check common installation directories
$commonDirs = @(
    "C:\ProgramData\chocolatey\bin",
    "C:\ffmpeg\bin",
    "C:\tools\ffmpeg\bin",
    "$env:LOCALAPPDATA\Microsoft\WinGet\Links"
)

foreach ($dir in $commonDirs) {
    if (Test-Path (Join-Path $dir "ffmpeg.exe")) {
        Write-Host "[INFO] Found FFmpeg in $dir. Adding to process PATH." -ForegroundColor Yellow
        $env:PATH = "$dir;$env:PATH"
        if ($env:GITHUB_PATH) {
            $dir | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
        }
        & ffmpeg -version | Select-Object -First 1
        exit 0
    }
}

# 3. Attempt automated installation via Chocolatey, Winget, or Direct CDN Download
$installed = $false

# 3a. Try Chocolatey
if (Get-Command "choco.exe" -ErrorAction SilentlyContinue) {
    Write-Host "[INFO] Attempting FFmpeg install via Chocolatey..." -ForegroundColor Cyan
    try {
        & choco install ffmpeg --no-progress -y
        $chocoBin = "C:\ProgramData\chocolatey\bin"
        if (Test-Path (Join-Path $chocoBin "ffmpeg.exe")) {
            $env:PATH = "$chocoBin;$env:PATH"
            if ($env:GITHUB_PATH) {
                $chocoBin | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
            }
            $installed = $true
        }
    } catch {
        Write-Warning "Chocolatey install failed: $_"
    }
}

# 3b. Try Winget if Chocolatey failed
if (-not $installed -and (Get-Command "winget.exe" -ErrorAction SilentlyContinue)) {
    Write-Host "[INFO] Attempting FFmpeg install via Winget..." -ForegroundColor Cyan
    try {
        & winget install Gyan.FFmpeg --accept-source-agreements --accept-package-agreements
        $wingetLinks = "$env:LOCALAPPDATA\Microsoft\WinGet\Links"
        if (Test-Path (Join-Path $wingetLinks "ffmpeg.exe")) {
            $env:PATH = "$wingetLinks;$env:PATH"
            if ($env:GITHUB_PATH) {
                $wingetLinks | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
            }
            $installed = $true
        }
    } catch {
        Write-Warning "Winget install failed: $_"
    }
}

# 3c. Direct GitHub CDN Download Fallback
if (-not $installed) {
    Write-Host "[INFO] Downloading standalone FFmpeg from GitHub CDN..." -ForegroundColor Cyan
    $ffmpegDir = "C:\ffmpeg\bin"
    if (-not (Test-Path $ffmpegDir)) {
        New-Item -ItemType Directory -Force -Path $ffmpegDir | Out-Null
    }
    $zipPath = "$env:TEMP\ffmpeg.zip"
    $downloadUrl = "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing
    
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        foreach ($entry in $zip.Entries) {
            if ($entry.Name -eq "ffmpeg.exe" -or $entry.Name -eq "ffprobe.exe") {
                $destFile = Join-Path $ffmpegDir $entry.Name
                [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $destFile, $true)
            }
        }
    } finally {
        $zip.Dispose()
    }
    Remove-Item -Force $zipPath -ErrorAction SilentlyContinue

    if (Test-Path (Join-Path $ffmpegDir "ffmpeg.exe")) {
        $env:PATH = "$ffmpegDir;$env:PATH"
        if ($env:GITHUB_PATH) {
            $ffmpegDir | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
        }
        $installed = $true
    }
}

# 4. Final Verification
$finalFfmpeg = Get-Command "ffmpeg.exe" -ErrorAction SilentlyContinue
$finalFfprobe = Get-Command "ffprobe.exe" -ErrorAction SilentlyContinue

if (-not $finalFfmpeg -or -not $finalFfprobe) {
    Write-Error "FFmpeg or FFprobe failed to verify after installation attempt."
    exit 1
}

Write-Host "[SUCCESS] FFmpeg and FFprobe configured and verified successfully." -ForegroundColor Green
& ffmpeg -version | Select-Object -First 1
& ffprobe -version | Select-Object -First 1

