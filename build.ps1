# AgentTheSpire isolated Tauri candidate build.
#
# Usage:
#   .\build.ps1 -Variant Baseline
#   .\build.ps1 -Variant Ml
#   .\build.ps1 -Variant Ml -PlanOnly -BuildId test-ml

[CmdletBinding()]
param(
    [ValidateSet('Baseline', 'Ml')]
    [string]$Variant = 'Baseline',
    [switch]$PlanOnly,
    [string]$BuildId,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgsExtra
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
. "$PSScriptRoot\scripts\release-build-lib.ps1"

function Assert-BoundedIdentifier {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][int]$MaxLength
    )
    if ([string]::IsNullOrWhiteSpace($Value) -or
        $Value.Length -gt $MaxLength -or
        $Value -notmatch '^[A-Za-z0-9._-]+$') {
        throw "$Name must contain 1-$MaxLength ASCII identifier characters"
    }
}

function Assert-RuntimeBuildIdentity {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$Variant,
        [string[]]$Features = @(),
        [Parameter(Mandatory = $true)][string]$BuildId
    )
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "candidate executable was not produced: $Executable"
    }
    $identityPath = Join-Path `
        (Split-Path -Parent $Executable) `
        ("runtime-build-info-" + [Guid]::NewGuid().ToString('N') + '.json')
    try {
        $identityProcess = Start-Process `
            -FilePath $Executable `
            -ArgumentList @('--write-build-info', ('"' + $identityPath + '"')) `
            -Wait `
            -PassThru `
            -WindowStyle Hidden
        if ($identityProcess.ExitCode -ne 0 -or
            -not (Test-Path -LiteralPath $identityPath -PathType Leaf)) {
            throw 'candidate executable failed to report BuildInfo'
        }
        try {
            $identity = Get-Content -Raw -LiteralPath $identityPath | ConvertFrom-Json
        } catch {
            throw 'candidate executable returned invalid BuildInfo JSON'
        }
    } finally {
        Remove-Item -LiteralPath $identityPath -Force -ErrorAction SilentlyContinue
    }
    $actualFeatures = @($identity.features)
    if ($identity.commit -ne $Commit -or
        $identity.variant -ne $Variant -or
        $identity.buildId -ne $BuildId -or
        $actualFeatures.Count -ne $Features.Count -or
        ($actualFeatures -join ',') -ne ($Features -join ',')) {
        throw 'candidate executable BuildInfo does not match the requested build identity'
    }
}

$variantId = $Variant.ToLowerInvariant()
$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-fA-F]{40}$') {
    throw 'A full Git commit SHA is required for a candidate build'
}
if ([string]::IsNullOrWhiteSpace($BuildId)) {
    $timestamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
    $BuildId = "$timestamp-$variantId-$($commit.Substring(0, 12))"
}
Assert-BoundedIdentifier -Name 'BuildId' -Value $BuildId -MaxLength 96
if ($TauriArgsExtra | Where-Object { $_ -eq '--features' -or $_ -eq '-F' -or $_ -like '--features=*' }) {
    throw 'Cargo features are controlled exclusively by -Variant'
}

$workingTreeChanges = @(& git status --porcelain --untracked-files=normal)
if ($LASTEXITCODE -ne 0) { throw 'Git working tree status could not be determined' }
$workingTreeClean = $workingTreeChanges.Count -eq 0

$features = @()
$tauriArgs = @()
if ($variantId -eq 'ml') {
    $features = @('ml-rembg')
    $tauriArgs += '--features'
    $tauriArgs += 'ml-rembg'
}

$targetDir = Join-Path $PSScriptRoot "artifacts\build\$variantId\$BuildId\target"
$bundleDir = Join-Path $targetDir 'release\bundle'
$releaseDir = Join-Path $PSScriptRoot "artifacts\release\$BuildId\$variantId"
$manifestPath = Join-Path $releaseDir 'release-manifest.json'
$plan = [ordered]@{
    schemaVersion = 1
    commit = $commit
    variant = $variantId
    features = $features
    buildId = $BuildId
    targetDir = $targetDir
    bundleDir = $bundleDir
    releaseDir = $releaseDir
    manifestPath = $manifestPath
    tauriArgs = $tauriArgs
    workingTreeClean = $workingTreeClean
}

if ($PlanOnly) {
    $plan | ConvertTo-Json -Depth 5
    return
}

if (-not $workingTreeClean) {
    throw 'candidate builds require a clean Git working tree'
}

if (Test-Path -LiteralPath $targetDir) {
    throw "isolated target directory already exists for build id: $BuildId"
}
if (Test-Path -LiteralPath $releaseDir) {
    throw "release directory already exists for build id: $BuildId"
}

& "$PSScriptRoot\scripts\ensure-node-deps.ps1" -Root $PSScriptRoot

$previousTargetDir = $env:CARGO_TARGET_DIR
$previousCommit = $env:ATS_BUILD_COMMIT
$previousVariant = $env:ATS_BUILD_VARIANT
$previousBuildId = $env:ATS_BUILD_ID
try {
    $env:CARGO_TARGET_DIR = $targetDir
    $env:ATS_BUILD_COMMIT = $commit
    $env:ATS_BUILD_VARIANT = $variantId
    $env:ATS_BUILD_ID = $BuildId

    Write-Host "==> Building $variantId candidate $BuildId" -ForegroundColor Cyan
    npx tauri build @tauriArgs @TauriArgsExtra
    if ($LASTEXITCODE -ne 0) { throw 'tauri build failed' }
} finally {
    $env:CARGO_TARGET_DIR = $previousTargetDir
    $env:ATS_BUILD_COMMIT = $previousCommit
    $env:ATS_BUILD_VARIANT = $previousVariant
    $env:ATS_BUILD_ID = $previousBuildId
}

$candidateExecutable = Join-Path $targetDir 'release\agentthespire-desktop.exe'
Assert-RuntimeBuildIdentity `
    -Executable $candidateExecutable `
    -Commit $commit `
    -Variant $variantId `
    -Features $features `
    -BuildId $BuildId

$manifest = Publish-IsolatedBundle `
    -TargetDir $targetDir `
    -BundleDir $bundleDir `
    -ReleaseDir $releaseDir `
    -Commit $commit `
    -Variant $variantId `
    -Features $features `
    -BuildId $BuildId

Write-Host "==> Release manifest: $manifestPath" -ForegroundColor Green
$manifest | ConvertTo-Json -Depth 6
