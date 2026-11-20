[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$failures = [System.Collections.Generic.List[string]]::new()

# SPDX identifiers accepted for distributable WorkSets dependencies. MPL is
# permitted because its file-level obligations are compatible with this use.
$allowedIdentifiers = [System.Collections.Generic.HashSet[string]]::new(
    [string[]]@(
        "0BSD",
        "Apache-2.0",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "BSL-1.0",
        "CC-BY-4.0",
        "CC0-1.0",
        "CDLA-Permissive-2.0",
        "ISC",
        "MIT",
        "MIT-0",
        "MPL-2.0",
        "NCSA",
        "OpenSSL",
        "Python-2.0",
        "Unicode-3.0",
        "Unicode-DFS-2016",
        "Unlicense",
        "WTFPL",
        "Zlib",
        "BlueOak-1.0.0"
    ),
    [System.StringComparer]::OrdinalIgnoreCase
)

$allowedOperators = [System.Collections.Generic.HashSet[string]]::new(
    [string[]]@("LLVM-exception"),
    [System.StringComparer]::OrdinalIgnoreCase
)

function Read-LicensePrimary {
    param(
        [string[]]$Tokens,
        [ref]$Position
    )

    if ($Position.Value -ge $Tokens.Count) {
        throw "Unexpected end of license expression."
    }

    $token = $Tokens[$Position.Value]
    $Position.Value++
    if ($token -eq "(") {
        $result = Read-LicenseOr -Tokens $Tokens -Position $Position
        if ($Position.Value -ge $Tokens.Count -or $Tokens[$Position.Value] -ne ")") {
            throw "Unclosed parenthesis in license expression."
        }
        $Position.Value++
        return $result
    }

    if ($token -eq ")" -or $token -in @("AND", "OR", "WITH")) {
        throw "Unexpected token '$token' in license expression."
    }

    return $allowedIdentifiers.Contains($token)
}

function Read-LicenseWith {
    param(
        [string[]]$Tokens,
        [ref]$Position
    )

    $result = Read-LicensePrimary -Tokens $Tokens -Position $Position
    while ($Position.Value -lt $Tokens.Count -and $Tokens[$Position.Value] -eq "WITH") {
        $Position.Value++
        if ($Position.Value -ge $Tokens.Count) {
            throw "Missing exception after WITH in license expression."
        }
        $exception = $Tokens[$Position.Value]
        $Position.Value++
        $exceptionAllowed = $allowedOperators.Contains($exception)
        $result = $result -and $exceptionAllowed
    }
    return $result
}

function Read-LicenseAnd {
    param(
        [string[]]$Tokens,
        [ref]$Position
    )

    $result = Read-LicenseWith -Tokens $Tokens -Position $Position
    while ($Position.Value -lt $Tokens.Count -and $Tokens[$Position.Value] -eq "AND") {
        $Position.Value++
        $right = Read-LicenseWith -Tokens $Tokens -Position $Position
        $result = $result -and $right
    }
    return $result
}

function Read-LicenseOr {
    param(
        [string[]]$Tokens,
        [ref]$Position
    )

    $result = Read-LicenseAnd -Tokens $Tokens -Position $Position
    while ($Position.Value -lt $Tokens.Count -and $Tokens[$Position.Value] -eq "OR") {
        $Position.Value++
        $right = Read-LicenseAnd -Tokens $Tokens -Position $Position
        $result = $result -or $right
    }
    return $result
}

function Test-LicenseExpression {
    param(
        [Parameter(Mandatory)] [string]$Name,
        [AllowNull()] [string]$Expression,
        [AllowNull()] [string]$LicenseFile,
        [Parameter(Mandatory)] [string]$Ecosystem
    )

    if ([string]::IsNullOrWhiteSpace($Expression)) {
        if (-not [string]::IsNullOrWhiteSpace($LicenseFile)) {
            Write-Warning "$Ecosystem dependency '$Name' declares a license file but no SPDX expression: $LicenseFile"
            return
        }

        $failures.Add("$Ecosystem dependency '$Name' has no declared license.")
        return
    }

    # Several older crates use `/` for a dual-license choice. Normalize it to
    # SPDX `OR`, then evaluate AND/OR/WITH with the correct precedence.
    $normalized = $Expression -replace "\s*/\s*", " OR "
    $tokens = @([regex]::Matches($normalized, "\(|\)|[^\s()]+") | ForEach-Object { $_.Value })
    $position = 0
    try {
        $approved = Read-LicenseOr -Tokens $tokens -Position ([ref]$position)
        if ($position -ne $tokens.Count) {
            throw "Unexpected token '$($tokens[$position])'."
        }
    } catch {
        $failures.Add("$Ecosystem dependency '$Name' has an invalid license expression '$Expression': $($_.Exception.Message)")
        return
    }

    if (-not $approved) {
        $failures.Add("$Ecosystem dependency '$Name' has no approved option in license expression '$Expression'.")
    }
}

$packageLockPath = Join-Path $repoRoot "package-lock.json"
if (-not (Test-Path -LiteralPath $packageLockPath -PathType Leaf)) {
    throw "package-lock.json is required for a reproducible license check."
}

$packageLock = Get-Content -Raw -LiteralPath $packageLockPath | ConvertFrom-Json -AsHashtable -Depth 100
$npmCount = 0
foreach ($entry in $packageLock["packages"].GetEnumerator()) {
    if ([string]::IsNullOrEmpty($entry.Key) -or $entry.Value["link"] -eq $true) {
        continue
    }

    $npmCount++
    $displayName = $entry.Key -replace "^.*node_modules/", ""
    Test-LicenseExpression `
        -Name $displayName `
        -Expression ([string]$entry.Value["license"]) `
        -LicenseFile $null `
        -Ecosystem "npm"
}

$cargoCount = 0
$cargoManifests = @(
    "src-tauri/Cargo.toml",
    "fixtures/windows-app/Cargo.toml"
)
foreach ($manifest in $cargoManifests) {
    Push-Location $repoRoot
    try {
        $metadataOutput = & cargo metadata `
            --locked `
            --format-version 1 `
            --manifest-path $manifest
        if ($LASTEXITCODE -ne 0) {
            throw "cargo metadata failed for '$manifest' with exit code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
    }

    $cargoMetadata = ($metadataOutput -join [Environment]::NewLine) | ConvertFrom-Json -Depth 100
    foreach ($package in $cargoMetadata.packages) {
        $cargoCount++
        Test-LicenseExpression `
            -Name "$($package.name)@$($package.version) ($manifest)" `
            -Expression ([string]$package.license) `
            -LicenseFile ([string]$package.license_file) `
            -Ecosystem "Cargo"
    }
}

if ($failures.Count -gt 0) {
    $failures | Sort-Object -Unique | ForEach-Object { Write-Error $_ -ErrorAction Continue }
    throw "Dependency license policy failed with $($failures.Count) finding(s)."
}

Write-Host "License policy passed for $npmCount npm packages and $cargoCount Cargo packages."
