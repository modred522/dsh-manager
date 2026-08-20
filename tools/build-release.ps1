# build-release.ps1 - package a portable win-x64 zip release in a SEPARATE
# directory INSIDE the project (default <project root>\DSHManagerRelease, which
# is git-ignored). The source tree itself stays untouched.
# Pure ASCII: PowerShell 5.1 reads no-BOM files as ANSI.
#
# Usage:
#   powershell -NoProfile -File tools\build-release.ps1                 # build v1.0.0
#   powershell -NoProfile -File tools\build-release.ps1 -Version 1.1.0  # custom version
#   powershell -NoProfile -File tools\build-release.ps1 -OutDir E:\Builds\DSHManager

param(
    [string]$OutDir = "",
    [string]$Version = "1.0.0"
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot -Parent
if ($OutDir -eq "") { $OutDir = Join-Path $Root "DSHManagerRelease" }

if (-not (Get-Command git -ErrorAction SilentlyContinue)) { throw "git not found. Install Git for Windows first." }
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { throw "npm not found." }

$AppDir = Join-Path $OutDir "app"
$Zip = Join-Path $OutDir "app-src.zip"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (Test-Path $AppDir) { Remove-Item $AppDir -Recurse -Force }
if (Test-Path $Zip) { Remove-Item $Zip -Force }

Write-Host "[1/4] Exporting clean source snapshot from git HEAD..."
& git -C $Root archive --format=zip -o $Zip HEAD
if ($LASTEXITCODE -ne 0) { throw "git archive failed" }
Expand-Archive -Path $Zip -DestinationPath $AppDir -Force
Remove-Item $Zip -Force

Write-Host "[2/4] Writing package metadata for the packaged app..."
$pjPath = Join-Path $AppDir "package.json"
# Read as UTF-8 explicitly: PowerShell 5.1 reads no-BOM files as ANSI.
$pj = [System.IO.File]::ReadAllText($pjPath) | ConvertFrom-Json
$pj.author = "modred522"
$pj.version = $Version
$buildConfig = [ordered]@{
    appId       = "com.modred522.dsh-manager"
    productName = "DSH Manager"
    asar        = $false
    directories = [ordered]@{ output = "../release" }
    files       = @("main.js", "preload.js", "find-dsh.ps1", "renderer/**", "assets/whale.png", "assets/app.ico", "package.json", "LICENSE")
    win         = [ordered]@{ target = @([ordered]@{ target = "zip"; arch = @("x64") }); icon = "assets/app.ico" }
}
# PSCustomObject cannot take new properties by assignment; use Add-Member.
$pj | Add-Member -NotePropertyName 'build' -NotePropertyValue $buildConfig
# Write WITHOUT BOM (electron-rebuild JSON.parse rejects BOM).
[System.IO.File]::WriteAllText($pjPath, ($pj | ConvertTo-Json -Depth 10))

Write-Host "[3/4] Installing electron + electron-builder (all caches redirected)..."
$env:npm_config_cache = Join-Path $OutDir "npm-cache"
$env:electron_config_cache = Join-Path $OutDir "electron-cache"
$env:ELECTRON_BUILDER_CACHE = Join-Path $OutDir "builder-cache"
Push-Location $AppDir
& npm install --no-save
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "npm install failed" }
& npm install --no-save electron-builder
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "electron-builder install failed" }

Write-Host "[4/4] Building win-x64 zip..."
& (Join-Path $AppDir "node_modules\.bin\electron-builder.cmd") --win zip --x64
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "electron-builder failed" }
Pop-Location

Write-Host ""
Write-Host ("DONE. Release zip: " + (Join-Path $OutDir "release"))
