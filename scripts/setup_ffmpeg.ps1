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

# 3. Attempt automated installation via Chocolatey or Winget
if (Get-Command "choco.exe" -ErrorAction SilentlyContinue) {
    Write-Host "[INFO] Installing FFmpeg via Chocolatey..." -ForegroundColor Cyan
    & choco install ffmpeg --no-progress -y
    $chocoBin = "C:\ProgramData\chocolatey\bin"
    if (Test-Path (Join-Path $chocoBin "ffmpeg.exe")) {
        $env:PATH = "$chocoBin;$env:PATH"
        if ($env:GITHUB_PATH) {
            $chocoBin | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
        }
    }
} elseif (Get-Command "winget.exe" -ErrorAction SilentlyContinue) {
    Write-Host "[INFO] Installing FFmpeg via Winget..." -ForegroundColor Cyan
    & winget install Gyan.FFmpeg --accept-source-agreements --accept-package-agreements
} else {
    Write-Error "Neither Chocolatey nor Winget found. Please install FFmpeg manually and add it to PATH."
    exit 1
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

