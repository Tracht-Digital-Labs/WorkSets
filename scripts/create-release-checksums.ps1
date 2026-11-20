[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern("^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$")]
    [string]$Version,

    [Parameter(Mandatory)]
    [string]$AssetDirectory
)

$ErrorActionPreference = "Stop"
$resolvedAssets = (Resolve-Path -LiteralPath $AssetDirectory).Path
$expectedNames = @(
    "WorkSets_${Version}_windows_x64_setup.exe",
    "WorkSets_${Version}_windows_arm64_setup.exe"
)
$actualInstallers = @(Get-ChildItem -LiteralPath $resolvedAssets -Filter "*.exe" -File)
$actualNames = @($actualInstallers | ForEach-Object { $_.Name })

$difference = @(Compare-Object -ReferenceObject $expectedNames -DifferenceObject $actualNames)
if ($difference.Count -gt 0) {
    $description = $difference | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
    throw "Release installer set is incomplete or unexpected: $($description -join ', ')."
}

$checksumLines = foreach ($installer in ($actualInstallers | Sort-Object Name)) {
    $hash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($installer.Name)"
}

$checksumPath = Join-Path $resolvedAssets "SHA256SUMS.txt"
$checksumLines | Set-Content -LiteralPath $checksumPath -Encoding ascii
Write-Host "Created SHA256SUMS.txt for $($actualInstallers.Count) installers."
