# Reddart Release Integrity Verification Script
# Validates hash agreement across ready_to_use/windows, manifest.json, docs, and website.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$WindowsDir = Join-Path $RepoRoot "ready_to_use\windows"
$SumsFile = Join-Path $WindowsDir "SHA256SUMS.txt"
$ManifestFile = Join-Path $WindowsDir "manifest.json"
$NotesFile = Join-Path $RepoRoot "docs\RELEASE_NOTES_AND_LIMITATIONS.md"
$WebFile = Join-Path $RepoRoot "website\index.html"

Write-Host "=========================================================="
Write-Host "Verifying Reddart Release Integrity & Checksum Consistency"
Write-Host "=========================================================="

if (-not (Test-Path $SumsFile)) {
    throw "Missing SHA256SUMS.txt at $SumsFile"
}

$ExpectedHashes = @{}
Get-Content $SumsFile | ForEach-Object {
    $line = $_.Trim()
    if ($line -and -not $line.StartsWith("#")) {
        $parts = $line -split "\s+"
        if ($parts.Length -eq 2) {
            $ExpectedHashes[$parts[1]] = $parts[0].ToLower()
        }
    }
}

Write-Host "1. Verifying on-disk file checksums..."
foreach ($filename in $ExpectedHashes.Keys) {
    $targetPath = Join-Path $WindowsDir $filename
    if (-not (Test-Path $targetPath)) {
        throw "Missing expected release asset: $targetPath"
    }
    $actualHash = (Get-FileHash -Path $targetPath -Algorithm SHA256).Hash.ToLower()
    $expectedHash = $ExpectedHashes[$filename]
    if ($actualHash -ne $expectedHash) {
        throw "Checksum mismatch for $filename! Expected $expectedHash, got $actualHash"
    }
    Write-Host "  [OK] $filename ($actualHash)"
}

Write-Host "2. Verifying manifest.json..."
$manifest = Get-Content $ManifestFile -Raw | ConvertFrom-Json
$exeHash = $ExpectedHashes["Reddart.exe"]
$zipEntry = $ExpectedHashes.Keys | Where-Object { $_ -like "Reddart-v*-Windows-x86_64.zip" } | Select-Object -First 1
$zipHash = $ExpectedHashes[$zipEntry]

if ($manifest.sha256_executable.ToLower() -ne $exeHash) {
    throw "manifest.json sha256_executable mismatch!"
}
if ($manifest.sha256_archive.ToLower() -ne $zipHash) {
    throw "manifest.json sha256_archive mismatch!"
}
Write-Host "  [OK] manifest.json matches SHA256SUMS.txt"

Write-Host "3. Verifying docs/RELEASE_NOTES_AND_LIMITATIONS.md..."
$notesText = Get-Content $NotesFile -Raw
if (-not $notesText.Contains($exeHash) -or -not $notesText.Contains($zipHash)) {
    throw "RELEASE_NOTES_AND_LIMITATIONS.md does not contain current hashes!"
}
Write-Host "  [OK] docs/RELEASE_NOTES_AND_LIMITATIONS.md matches"

Write-Host "4. Verifying website/index.html..."
$webText = Get-Content $WebFile -Raw
if (-not $webText.Contains($exeHash) -or -not $webText.Contains($zipHash)) {
    throw "website/index.html does not contain current hashes!"
}
Write-Host "  [OK] website/index.html matches"

Write-Host "=========================================================="
Write-Host "SUCCESS: All release assets and manifests are 100% in sync!"
Write-Host "=========================================================="
