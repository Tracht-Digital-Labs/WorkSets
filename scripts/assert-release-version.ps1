[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern("^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$")]
    [string]$Tag
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$expectedVersion = $Tag.Substring(1)

$packageJson = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "package.json") | ConvertFrom-Json
$packageLock = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "package-lock.json") | ConvertFrom-Json -AsHashtable -Depth 100
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "src-tauri/tauri.conf.json") | ConvertFrom-Json

$cargoManifestPath = Join-Path $repoRoot "src-tauri/Cargo.toml"
$inPackageSection = $false
$cargoVersion = $null
foreach ($line in Get-Content -LiteralPath $cargoManifestPath) {
    if ($line -match "^\s*\[package\]\s*$") {
        $inPackageSection = $true
        continue
    }

    if ($inPackageSection -and $line -match "^\s*\[") {
        break
    }

    if ($inPackageSection -and $line -match '^\s*version\s*=\s*"(?<version>[^"]+)"') {
        $cargoVersion = $Matches.version
        break
    }
}

if ([string]::IsNullOrWhiteSpace($cargoVersion)) {
    throw "Could not read [package].version from src-tauri/Cargo.toml."
}

$versions = [ordered]@{
    "package.json" = [string]$packageJson.version
    "package-lock.json" = [string]$packageLock["version"]
    "package-lock.json root package" = [string]$packageLock["packages"][""]["version"]
    "src-tauri/Cargo.toml" = [string]$cargoVersion
    "src-tauri/tauri.conf.json" = [string]$tauriConfig.version
}

$mismatches = @($versions.GetEnumerator() | Where-Object { $_.Value -ne $expectedVersion })
if ($mismatches.Count -gt 0) {
    $details = $mismatches | ForEach-Object { "$($_.Key)='$($_.Value)'" }
    throw "Tag '$Tag' expects version '$expectedVersion', but these values differ: $($details -join ', ')."
}

Write-Host "All version sources match $Tag."
