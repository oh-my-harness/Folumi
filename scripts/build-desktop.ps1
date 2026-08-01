param(
    [string]$Target = "",
    [string[]]$Bundles = @(),
    [switch]$NoBundle
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$tauriReleaseConfig = Join-Path $root "src-tauri\tauri.release.conf.json"
$sidecarDir = Join-Path $root "src-tauri\binaries"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )

    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Script
}

function Get-HostTarget {
    $hostLine = rustc -vV | Where-Object { $_ -like "host:*" } | Select-Object -First 1
    if (-not $hostLine) {
        throw "Unable to determine Rust host target from rustc -vV."
    }
    return $hostLine.Substring("host:".Length).Trim()
}

function Get-ExeSuffix {
    param([string]$BuildTarget)
    if ($BuildTarget -match "windows") {
        return ".exe"
    }
    return ""
}

if ([string]::IsNullOrWhiteSpace($Target)) {
    $Target = Get-HostTarget
}

$exeSuffix = Get-ExeSuffix $Target
$backendBinary = Join-Path $root "target\$Target\release\tutor-web$exeSuffix"
$desktopBinary = Join-Path $root "target\$Target\release\llm-tutor-desktop$exeSuffix"
$sidecarBinary = Join-Path $sidecarDir "tutor-web-$Target$exeSuffix"
$bundleDir = Join-Path $root "target\$Target\release\bundle"

Write-Host "Folumi desktop release build"
Write-Host "Root:   $root"
Write-Host "Target: $Target"

Invoke-Step "Build tutor-web sidecar" {
    Push-Location $root
    try {
        cargo build --release -p tutor-web --target $Target
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $backendBinary)) {
    throw "Expected backend binary was not found: $backendBinary"
}

Invoke-Step "Copy sidecar for Tauri bundle" {
    New-Item -ItemType Directory -Force -Path $sidecarDir | Out-Null
    Copy-Item -LiteralPath $backendBinary -Destination $sidecarBinary -Force
    Write-Host "Sidecar: $sidecarBinary"
}

Invoke-Step "Build Tauri desktop bundle" {
    Push-Location $root
    try {
        $tauriArgs = @("tauri", "build", "--target", $Target, "--config", $tauriReleaseConfig, "--ci")
        if ($NoBundle) {
            $tauriArgs += "--no-bundle"
        }
        elseif ($Bundles.Count -gt 0) {
            $tauriArgs += "--bundles"
            $tauriArgs += ($Bundles -join ",")
        }
        cargo @tauriArgs
    }
    finally {
        Pop-Location
    }
}

Write-Host ""
Write-Host "Desktop build complete." -ForegroundColor Green
Write-Host "App binary: $desktopBinary"
if (Test-Path $bundleDir) {
    Write-Host "Bundle directory: $bundleDir"
    Get-ChildItem -Path $bundleDir -Recurse -File |
        Where-Object { $_.Extension -in @(".exe", ".msi", ".zip", ".nsis") -or $_.Name -like "*.msi" } |
        ForEach-Object { Write-Host "Artifact: $($_.FullName)" }
}
else {
    Write-Host "Bundle directory not found. If you used -NoBundle, this is expected."
}
