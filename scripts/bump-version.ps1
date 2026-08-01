param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Version
)

$ErrorActionPreference = "Stop"

if ($Version -notmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$') {
    throw "Version must use numeric MAJOR.MINOR.PATCH format, for example 0.1.0. MSI bundles do not accept alpha/beta SemVer identifiers."
}

$root = Split-Path -Parent $PSScriptRoot

function Update-TextFile {
    param(
        [string]$Path,
        [scriptblock]$Updater
    )

    $utf8 = [System.Text.UTF8Encoding]::new($false)
    $text = [System.IO.File]::ReadAllText($Path, $utf8)
    $next = & $Updater $text
    if ($null -eq $next) {
        throw "No version change was made in $Path."
    }
    if ($next -eq $text) {
        Write-Host "No change needed $Path"
        return
    }
    [System.IO.File]::WriteAllText($Path, $next, $utf8)
    Write-Host "Updated $Path"
}

function Replace-First {
    param(
        [string]$Text,
        [string]$Pattern,
        [string]$Replacement
    )

    $regex = [regex]::new($Pattern)
    return $regex.Replace($Text, $Replacement, 1)
}

function Update-JsonVersion {
    param([string]$Path)

    Update-TextFile -Path $Path -Updater {
        param($text)
        Replace-First $text '(?m)^(\s*"version"\s*:\s*)"[^"]+"' "`$1`"$Version`""
    }
}

function Update-PackageLockVersion {
    param([string]$Path)

    Update-TextFile -Path $Path -Updater {
        param($text)
        $next = Replace-First $text '(?m)^(  "version"\s*:\s*)"[^"]+"' "`$1`"$Version`""
        $next = Replace-First $next '(?m)^(      "version"\s*:\s*)"[^"]+"' "`$1`"$Version`""
        $next
    }
}

Update-TextFile -Path (Join-Path $root "Cargo.toml") -Updater {
    param($text)
    $text -replace '(?m)^version\s*=\s*"[^"]+"', "version = `"$Version`""
}
Update-TextFile -Path (Join-Path $root "Cargo.lock") -Updater {
    param($text)
    $next = $text
    foreach ($package in @("llm-tutor-desktop", "tutor-agent", "tutor-rag", "tutor-tools", "tutor-web")) {
        $escapedPackage = [regex]::Escape($package)
        $pattern = '(?m)(^\[\[package\]\]\r?\nname = "{0}"\r?\nversion = ")[^"]+("$)' -f $escapedPackage
        $next = Replace-First $next $pattern "`${1}$Version`${2}"
    }
    $next
}

Update-JsonVersion -Path (Join-Path $root "src-tauri\tauri.conf.json")
Update-JsonVersion -Path (Join-Path $root "web-ui\package.json")
Update-PackageLockVersion -Path (Join-Path $root "web-ui\package-lock.json")
Update-TextFile -Path (Join-Path $root "README.md") -Updater {
    param($text)
    $next = Replace-First $text '(?m)^(> [^\r\n]*`)\d+\.\d+\.\d+(`)' "`${1}$Version`${2}"
    Replace-First $next '(?m)^(> [^\r\n]*`v)\d+\.\d+\.\d+(`)' "`${1}$Version`${2}"
}
Update-TextFile -Path (Join-Path $root "MANUAL.md") -Updater {
    param($text)
    Replace-First $text '(?m)^(> [^\r\n]*?)\d+\.\d+\.\d+( [^\r\n]*)$' "`${1}$Version`${2}"
}
Update-TextFile -Path (Join-Path $root "docs\qa\desktop-release.md") -Updater {
    param($text)
    Replace-First $text '(?m)^(This checklist applies to the current desktop release, including `v)\d+\.\d+\.\d+(`\.)' "`${1}$Version`${2}"
}

Push-Location $root
try {
    & cargo +stable metadata --no-deps --format-version 1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed while refreshing Cargo.lock."
    }
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "Version updated to $Version." -ForegroundColor Green
Write-Host "Recommended next checks:"
Write-Host "  node scripts/check-version.mjs"
Write-Host "  npm run build --prefix web-ui"
