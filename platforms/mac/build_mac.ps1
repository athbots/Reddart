# Reddart — Zero-Lag Video Editor: macOS App Packaging Pipeline
# Delegates to authoritative root packaging script scripts/package_mac.ps1

$ErrorActionPreference = "Stop"

$RootDir = (Get-Item $PSScriptRoot).Parent.Parent.FullName
$PackageScript = Join-Path $RootDir "scripts\package_mac.ps1"

& $PackageScript @args
