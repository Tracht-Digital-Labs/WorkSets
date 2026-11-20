[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("x64", "arm64")]
    [string]$Architecture,

    [Parameter(Mandatory)]
    [ValidatePattern("^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$")]
    [string]$Version,

    [Parameter(Mandatory)]
    [string]$InputDirectory,

    [Parameter(Mandatory)]
    [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"
$resolvedInput = (Resolve-Path -LiteralPath $InputDirectory).Path
$installers = @(Get-ChildItem -LiteralPath $resolvedInput -Filter "*.exe" -File)
if ($installers.Count -ne 1) {
    throw "Expected exactly one NSIS installer in '$resolvedInput', found $($installers.Count)."
}

if (-not (Test-Path -LiteralPath $OutputDirectory)) {
    New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
}

$resolvedOutput = (Resolve-Path -LiteralPath $OutputDirectory).Path
$assetName = "WorkSets_${Version}_windows_${Architecture}_setup.exe"
$destination = Join-Path $resolvedOutput $assetName
Copy-Item -LiteralPath $installers[0].FullName -Destination $destination -Force

$asset = Get-Item -LiteralPath $destination
if ($asset.Length -le 0) {
    throw "Prepared release asset '$destination' is empty."
}

Write-Host "Prepared $($asset.Name) ($($asset.Length) bytes)."
