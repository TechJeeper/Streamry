# Create a zip of files needed for Streamry macOS Tauri build.
# Run from repo root. On macOS: unzip, npm install, npm run tauri build -- --bundles dmg,app

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$rootDir = Split-Path -Parent $scriptDir
Set-Location $rootDir

$packageJson = Get-Content -Raw -Path "package.json" | ConvertFrom-Json
$version = $packageJson.version

$distDir = "dist-packages"
$stagingName = "streamry-macos-build-$version"
$stagingPath = Join-Path $distDir $stagingName
$zipPath = Join-Path $distDir "streamry-macos-build-$version.zip"

Write-Host "=== Streamry macOS build package ===" -ForegroundColor Cyan
Write-Host "Version: $version"
Write-Host ""

if (-not (Test-Path $distDir)) { New-Item -ItemType Directory -Path $distDir | Out-Null }

if (Test-Path $stagingPath) { Remove-Item $stagingPath -Recurse -Force }
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }

New-Item -ItemType Directory -Path $stagingPath | Out-Null

$rootFiles = @(
    "package.json",
    "package-lock.json",
    "index.html",
    "vite.config.ts",
    "tsconfig.json",
    "tsconfig.node.json",
    "README.md"
)

Write-Host "Copying root files..."
foreach ($f in $rootFiles) {
    if (Test-Path $f) {
        Copy-Item -Path $f -Destination (Join-Path $stagingPath $f) -Force
    }
}

Write-Host "Copying src/"
Copy-Item -Path "src" -Destination (Join-Path $stagingPath "src") -Recurse -Force

if (Test-Path "public") {
    Write-Host "Copying public/"
    Copy-Item -Path "public" -Destination (Join-Path $stagingPath "public") -Recurse -Force
}

# src-tauri without target/ (large build cache)
Write-Host "Copying src-tauri/ (excluding target/)"
$tauriDest = Join-Path $stagingPath "src-tauri"
New-Item -ItemType Directory -Path $tauriDest | Out-Null
Get-ChildItem -Path "src-tauri" -Force | Where-Object { $_.Name -ne "target" } | ForEach-Object {
    Copy-Item -Path $_.FullName -Destination (Join-Path $tauriDest $_.Name) -Recurse -Force
}

$readme = @"
Streamry macOS build package ($version)
=======================================

On macOS:
  1. unzip this file
  2. cd streamry-macos-build-$version
  3. npm install
  4. rustup target add aarch64-apple-darwin x86_64-apple-darwin
  5. npm run tauri build -- --target universal-apple-darwin --bundles dmg,app

Requires: Node 20+, Rust (rustup), Xcode Command Line Tools.

Output: src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg
"@
Set-Content -Path (Join-Path $stagingPath "README-MACOS-BUILD.txt") -Value $readme

Write-Host "Creating zip: $zipPath"
if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
    Write-Error "tar is required for macOS-compatible zips. Use Windows 10+ or install bsdtar."
}
Push-Location $distDir
try {
    if (Test-Path (Split-Path -Leaf $zipPath)) { Remove-Item (Split-Path -Leaf $zipPath) -Force }
    & tar -a -c -f (Split-Path -Leaf $zipPath) $stagingName
    if ($LASTEXITCODE -ne 0) { throw "tar failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

Remove-Item $stagingPath -Recurse -Force

Write-Host ""
Write-Host "Done. Package: $zipPath" -ForegroundColor Green
Write-Host "On macOS: unzip, then npm install; npm run tauri build -- --target universal-apple-darwin --bundles dmg,app"
