param(
    [ValidateRange(0, 65535)]
    [int]$Port = 8765,
    [string]$Dataset = '',
    [switch]$NoBrowser
)

$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$server = Join-Path $repoRoot 'tools\locomo_benchmark_ui\server.py'
if (-not (Test-Path -LiteralPath $server -PathType Leaf)) {
    throw "Benchmark server not found: $server"
}

if ($Dataset) {
    $resolvedDataset = (Resolve-Path -LiteralPath $Dataset -ErrorAction Stop).Path
    $env:FOLUMI_LOCOMO_DATASET = $resolvedDataset
}

$arguments = @($server, '--repo-root', $repoRoot, '--port', $Port)
if ($NoBrowser) {
    $arguments += '--no-browser'
}

$python = Get-Command python -ErrorAction SilentlyContinue
if ($python) {
    & $python.Source @arguments
    exit $LASTEXITCODE
}

$pyLauncher = Get-Command py -ErrorAction SilentlyContinue
if ($pyLauncher) {
    & $pyLauncher.Source -3 @arguments
    exit $LASTEXITCODE
}

throw 'Python 3 was not found. Install Python 3 and ensure python or py is available.'
