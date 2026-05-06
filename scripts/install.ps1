#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Install drift-wallpaper on Windows.
.DESCRIPTION
    From a checkout: builds from source with cargo.
    From anywhere else: downloads the latest release binary.
.PARAMETER Help
    Show help message.
.PARAMETER Version
    Specific version to install (default: latest).
.PARAMETER InstallDir
    Installation directory (default: $env:LOCALAPPDATA\drift-wallpaper).
.PARAMETER UseRelease
    Force release download even in a clone.
.PARAMETER NoVerify
    Skip SHA256 checksum verification.
.EXAMPLE
    .\install.ps1                     # In a clone → cargo build --release
    .\install.ps1 -UseRelease         # In a clone → download binary instead
    iwr …/install.ps1 | iex           # Download latest release
#>

[CmdletBinding(DefaultParameterSetName = "Default")]
param(
    [Parameter(ParameterSetName = "Help")]
    [switch]$Help,

    [Parameter(ParameterSetName = "Default")]
    [string]$Version,

    [Parameter(ParameterSetName = "Default")]
    [string]$InstallDir,

    [Parameter(ParameterSetName = "Default")]
    [switch]$UseRelease,

    [Parameter(ParameterSetName = "Default")]
    [switch]$NoVerify
)

$ErrorActionPreference = 'Stop'

$Repo = "undivisible/drift-wallpaper"
$ApiBase = "https://api.github.com/repos/$Repo"

if ($Help) {
    Get-Help $MyInvocation.MyCommand.Path -Detailed
    exit 0
}

function Info($msg)   { Write-Host "$msg" -ForegroundColor Cyan }
function Ok($msg)     { Write-Host "✓ $msg" -ForegroundColor Green }
function Warn($msg)   { Write-Host "! $msg" -ForegroundColor Yellow }
function Die($msg)    { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

function Get-LatestVersion {
    $resp = Invoke-RestMethod -Uri "$ApiBase/releases/latest" -Headers @{ "User-Agent" = "drift-installer" }
    return $resp.tag_name
}

function Get-AssetInfo($ver, $platform, $arch) {
    $ext = if ($platform -eq "windows") { "zip" } else { "tar.gz" }
    $assetPattern = "drift-wallpaper-$platform-$arch.$ext"
    $checksumPattern = "drift-wallpaper-$platform-$arch.sha256"

    $release = Invoke-RestMethod -Uri "$ApiBase/releases/tags/$ver" -Headers @{ "User-Agent" = "drift-installer" }
    $asset = $release.assets | Where-Object { $_.name -like $assetPattern }
    $checksumAsset = $release.assets | Where-Object { $_.name -like $checksumPattern }

    if (-not $asset) {
        Die "No asset found for $assetPattern in release $ver"
    }
    return @{
        Url        = $asset.browser_download_url
        ChecksumUrl= $checksumAsset.browser_download_url
        Ext        = $ext
    }
}

function Install-FromRepo($root) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Die "cargo not in PATH — install Rust from https://rustup.rs/ or use -UseRelease to download a binary."
    }
    Info "Building drift-wallpaper from local checkout ($root)…"
    Push-Location $root
    try {
        cargo build --release -p drift-app --locked
    }
    finally {
        Pop-Location
    }
    $installDir = $InstallDir
    if (-not $installDir) {
        $installDir = Join-Path $env:LOCALAPPDATA "drift-wallpaper"
    }
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item "$root\target\release\drift-wallpaper.exe" (Join-Path $installDir "drift-wallpaper.exe") -Force
    Ok "drift-wallpaper installed to $installDir\drift-wallpaper.exe"
    Write-Host ""
    Write-Host "Add to PATH:`n  `$env:PATH += ';' + '$installDir'   # or add via System Settings" -ForegroundColor Yellow
}

function Install-FromRelease($ver) {
    $platform = if ($env:OS -eq "Windows_NT") { "windows" } elseif ($IsLinux) { "linux" } elseif ($IsMacOS) { "macos" } else { Die "Unsupported OS" }
    $arch = if ($platform -eq "windows") {
        if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { Die "32-bit Windows not supported" }
    } else {
        $arch = (uname -m).ToLower()
        if ($arch -eq "x86_64") { "x86_64" } elseif ($arch -eq "aarch64") { "aarch64" } else { Die "Unsupported architecture: $arch" }
    }

    Info "Fetching latest release version…"
    if (-not $Version) {
        $ver = Get-LatestVersion
    } else {
        $ver = $Version
    }
    Info "Using version: $ver"

    $assetInfo = Get-AssetInfo $ver $platform $arch
    $tmpDir = Join-Path $env:TEMP ("drift-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmpDir | Out-Null
    $archivePath = Join-Path $tmpDir "drift-wallpaper.$($assetInfo.Ext)"
    $installPath = Join-Path $tmpDir "drift-wallpaper"

    # Download archive
    Info "Downloading $($assetInfo.Url)"
    Invoke-WebRequest -Uri $assetInfo.Url -OutFile $archivePath -UserAgent "drift-installer"

    # Download and verify checksum if available and not skipping
    if ($assetInfo.ChecksumUrl -and -not $NoVerify) {
        $checksumPath = Join-Path $tmpDir "drift-wallpaper.sha256"
        Info "Downloading checksum $($assetInfo.ChecksumUrl)"
        Invoke-WebRequest -Uri $assetInfo.ChecksumUrl -OutFile $checksumPath -UserAgent "drift-installer"

        Info "Verifying checksum…"
        $expected = (Get-Content $checksumPath | Select-Object -First 1).Split(' ')[0].ToLower()
        $hash = Get-FileHash -Path $archivePath -Algorithm SHA256 | Select-Object -ExpandProperty Hash
        if ($hash.ToLower() -ne $expected) {
            Die "Checksum mismatch! Expected $expected, got $hash"
        }
        Ok "Checksum verified"
    } elseif ($NoVerify) {
        Warn "Skipping checksum verification"
    }

    # Extract
    if ($assetInfo.Ext -eq "zip") {
        Info "Extracting…"
        Expand-Archive -Path $archivePath -DestinationPath $installPath -Force
    } else {
        Info "Extracting…"
        if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
            Die "tar not found — install Git for Windows or a POSIX environment"
        }
        tar xzf $archivePath -C $installPath
    }

    # Find binary
    $binary = Get-ChildItem $installPath -Recurse -File -Filter "drift-wallpaper*" | Select-Object -First 1
    if (-not $binary) {
        Die "Could not find drift-wallpaper binary in archive"
    }

    # Install
    $installDir = $InstallDir
    if (-not $installDir) {
        $installDir = Join-Path $env:LOCALAPPDATA "drift-wallpaper"
    }
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item $binary.FullName (Join-Path $installDir "drift-wallpaper.exe") -Force
    Remove-Item $tmpDir -Recurse -Force

    Ok "drift-wallpaper $ver installed to $installDir\drift-wallpaper.exe"
    Write-Host ""
    Write-Host "Add to PATH:`n  `$env:PATH += ';' + '$installDir'   # or add via System Settings" -ForegroundColor Yellow
}

# ---- Repo detection (from a checkout) ----------------------------------------
$src = $MyInvocation.MyCommand.Path
if ($src -and (Split-Path $src -Leaf) -eq "install.ps1" -and -not $UseRelease) {
    $startDir = Split-Path $src -Parent
    $root = $startDir
    while ($root -ne "" -and $root -ne $null) {
        if (Test-Path (Join-Path $root "crates/drift-app")) { break }
        $parent = Split-Path $root -Parent
        if ($parent -eq $root) { break }
        $root = $parent
    }
    if (Test-Path (Join-Path $root "crates/drift-app")) {
        $installDirParam = if ($InstallDir) { "-InstallDir" ; $InstallDir } else { $null }
        Install-FromRepo $root
        exit 0
    }
}

# ---- Release download path ----------------------------------------------------
if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "drift-wallpaper"
}
Install-FromRelease $Version
