# scripts/build-release.ps1
# Automates local release builds enforcing the Strictly Version-Numbered Executable Invariant.
# Usage: .\scripts\build-release.ps1

$ErrorActionPreference = "Stop"

Write-Host "[1/5] Reading version from Cargo.toml..." -ForegroundColor Cyan
$cargoContent = Get-Content -Path "Cargo.toml" -Raw
if ($cargoContent -match 'version\s*=\s*"([^"]+)"') {
    $version = $matches[1]
} else {
    Write-Error "Failed to parse version from Cargo.toml"
    exit 1
}

$tag = "v$version"
$exeVersioned = "target\release\qshare-$tag-windows-x64.exe"
$zipVersioned = "target\release\qshare-$tag-windows-x64.zip"
$shaVersioned = "target\release\qshare-$tag-windows-x64.sha256"

Write-Host "[2/5] Compiling release binary for $tag with LTO..." -ForegroundColor Cyan
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release --locked
if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo build --release failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

$unversionedExe = "target\release\qshare.exe"
if (-not (Test-Path $unversionedExe)) {
    Write-Error "Expected binary $unversionedExe was not produced by cargo."
    exit 1
}

Write-Host "[3/5] Moving unversioned binary to version-numbered artifact: $exeVersioned..." -ForegroundColor Cyan
Move-Item -Path $unversionedExe -Destination $exeVersioned -Force

Write-Host "[4/5] Creating compressed archive: $zipVersioned..." -ForegroundColor Cyan
Compress-Archive -Path $exeVersioned -DestinationPath $zipVersioned -Force

Write-Host "[5/5] Generating SHA-256 cryptographic hashes: $shaVersioned..." -ForegroundColor Cyan
$hashExe = (Get-FileHash -Path $exeVersioned -Algorithm SHA256).Hash.ToLower()
$hashZip = (Get-FileHash -Path $zipVersioned -Algorithm SHA256).Hash.ToLower()

"$hashExe *qshare-$tag-windows-x64.exe`r`n$hashZip *qshare-$tag-windows-x64.zip" | Set-Content -Path $shaVersioned -Encoding Ascii

Write-Host "`nRelease packaging complete for $tag!" -ForegroundColor Green
Write-Host "  - Executable: $exeVersioned"
Write-Host "  - Archive:    $zipVersioned"
Write-Host "  - Checksums:  $shaVersioned"
