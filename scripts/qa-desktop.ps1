param(
    [string]$Target = "",
    [string]$DataDir = "",
    [switch]$SkipBackendSmoke
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot

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

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), 0)
    $listener.Start()
    try {
        return $listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
}

function Assert-Path {
    param(
        [string]$Path,
        [string]$Label,
        [string]$Hint = "Run .\scripts\build-desktop.ps1 first."
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "$Label not found: $Path. $Hint"
    }
    Write-Host "OK: $Label -> $Path"
}

if ([string]::IsNullOrWhiteSpace($Target)) {
    $Target = Get-HostTarget
}

$exeSuffix = Get-ExeSuffix $Target
$backendBinary = Join-Path $root "target\$Target\release\tutor-web$exeSuffix"
$desktopBinary = Join-Path $root "target\$Target\release\llm-tutor-desktop$exeSuffix"
$sidecarBinary = Join-Path $root "src-tauri\binaries\tutor-web-$Target$exeSuffix"
$bundleDir = Join-Path $root "target\$Target\release\bundle"

Write-Host "Folumi desktop QA"
Write-Host "Root:   $root"
Write-Host "Target: $Target"

Assert-Path $backendBinary "release tutor-web"
Assert-Path $desktopBinary "release desktop binary"
Assert-Path $sidecarBinary "Tauri sidecar binary"

if (Test-Path -LiteralPath $bundleDir) {
    Write-Host "OK: bundle directory -> $bundleDir"
    Get-ChildItem -Path $bundleDir -Recurse -File |
        Where-Object { $_.Extension -in @(".exe", ".msi", ".zip") -or $_.Name -like "*.msi" } |
        ForEach-Object { Write-Host "Artifact: $($_.FullName)" }
}
else {
    Write-Host "WARN: bundle directory not found. This is expected after -NoBundle builds."
}

if (-not $SkipBackendSmoke) {
    if ([string]::IsNullOrWhiteSpace($DataDir)) {
        $DataDir = Join-Path ([System.IO.Path]::GetTempPath()) ("llm-tutor-desktop-qa-" + [System.Guid]::NewGuid().ToString("N"))
    }

    New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
    $port = Get-FreePort
    $baseUrl = "http://127.0.0.1:$port"
    Write-Host ""
    Write-Host "Starting backend smoke test on $baseUrl"
    Write-Host "Data dir: $DataDir"

    $process = Start-Process -FilePath $backendBinary `
        -ArgumentList @("--host", "127.0.0.1", "--port", "$port", "--data-dir", $DataDir) `
        -PassThru `
        -WindowStyle Hidden

    try {
        $deadline = (Get-Date).AddSeconds(45)
        $ready = $false
        while ((Get-Date) -lt $deadline) {
            if ($process.HasExited) {
                throw "tutor-web exited during smoke test with code $($process.ExitCode)."
            }

            try {
                $response = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/sessions" -TimeoutSec 2
                if ($response.StatusCode -eq 200) {
                    $ready = $true
                    break
                }
            }
            catch {
                Start-Sleep -Milliseconds 500
            }
        }

        if (-not $ready) {
            throw "Timed out waiting for /api/sessions."
        }

        Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/knowledge-bases" -TimeoutSec 5 | Out-Null

        $knowledgeBody = @{
            name = "Desktop QA"
            embedding = @{
                provider = "hash"
                model = "local-hash"
                api_key = "test"
                base_url = $null
                embeddings_path = $null
                dimensions = 32
                send_dimensions = $false
            }
        } | ConvertTo-Json -Depth 5
        $createKnowledgeResponse = Invoke-WebRequest `
            -UseBasicParsing `
            -Method Post `
            -Uri "$baseUrl/api/knowledge-bases" `
            -ContentType "application/json" `
            -Body $knowledgeBody `
            -TimeoutSec 5
        if ($createKnowledgeResponse.StatusCode -ne 201) {
            throw "Expected creating a QA knowledge base to return 201, got $($createKnowledgeResponse.StatusCode)."
        }

        Assert-Path (Join-Path $DataDir "sessions") "smoke session data directory"
        Assert-Path `
            (Join-Path $DataDir "knowledge-bases.json") `
            "smoke knowledge store" `
            "The backend responded, but creating the QA knowledge base did not persist the store."

        $settingsBody = @{
            budgetLimitUsd = 2
            requireApproval = $false
            llmConfigs = @()
            activeLlmConfigId = $null
            embeddingConfigs = @()
            activeEmbeddingConfigId = $null
            searchConfigs = @()
            activeSearchConfigId = $null
        } | ConvertTo-Json -Depth 5
        $settingsResponse = Invoke-WebRequest `
            -UseBasicParsing `
            -Method Put `
            -Uri "$baseUrl/api/settings" `
            -ContentType "application/json" `
            -Body $settingsBody `
            -TimeoutSec 5
        if ($settingsResponse.StatusCode -ne 200) {
            throw "Expected saving QA settings to return 200, got $($settingsResponse.StatusCode)."
        }
        Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/settings" -TimeoutSec 5 | Out-Null
        Assert-Path `
            (Join-Path $DataDir "settings.json") `
            "smoke settings store" `
            "The backend responded, but saving QA settings did not persist settings.json."

        Write-Host "OK: backend smoke test passed" -ForegroundColor Green
    }
    finally {
        if ($process -and -not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

Write-Host ""
Write-Host "Desktop QA automation complete." -ForegroundColor Green
