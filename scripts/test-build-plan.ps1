[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$buildScript = Join-Path $repoRoot 'build.ps1'
. (Join-Path $PSScriptRoot 'release-build-lib.ps1')

function Read-BuildPlan {
    param(
        [Parameter(Mandatory = $true)][string]$Variant,
        [Parameter(Mandatory = $true)][string]$BuildId
    )
    $json = & $buildScript -Variant $Variant -PlanOnly -BuildId $BuildId
    if ($LASTEXITCODE -ne 0) { throw "plan failed for $Variant" }
    return ($json | ConvertFrom-Json)
}

$baseline = Read-BuildPlan -Variant Baseline -BuildId 'fixture-baseline'
$ml = Read-BuildPlan -Variant Ml -BuildId 'fixture-ml'

if ($baseline.variant -ne 'baseline' -or $baseline.features.Count -ne 0) {
    throw 'baseline plan declared an invalid variant/feature combination'
}
if ($ml.variant -ne 'ml' -or $ml.features.Count -ne 1 -or $ml.features[0] -ne 'ml-rembg') {
    throw 'ML plan did not declare the ml-rembg feature'
}
if ($baseline.targetDir -eq $ml.targetDir -or $baseline.bundleDir -eq $ml.bundleDir) {
    throw 'baseline and ML plans share a build directory'
}
if ($null -eq $baseline.workingTreeClean -or $null -eq $ml.workingTreeClean) {
    throw 'build plan did not report Git working tree cleanliness'
}
try {
    & $buildScript -Variant Baseline -PlanOnly -BuildId 'invalid-features' '--features=e2e' | Out-Null
    throw 'build plan accepted an extra Cargo feature argument'
} catch {
    if ($_.Exception.Message -eq 'build plan accepted an extra Cargo feature argument') {
        throw
    }
}
foreach ($plan in @($baseline, $ml)) {
    if (-not $plan.bundleDir.StartsWith($plan.targetDir, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "bundle directory escaped target directory for $($plan.variant)"
    }
    if ($plan.bundleDir -match 'src-tauri[\\/]target') {
        throw "plan still uses the legacy shared Tauri target directory for $($plan.variant)"
    }
}

$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempRoot = Join-Path $tempBase ("ats-release-build-test-" + [Guid]::NewGuid().ToString('N'))
try {
    $target = Join-Path $tempRoot 'target'
    $bundle = Join-Path $target 'release\bundle'
    $release = Join-Path $tempRoot 'release'
    New-Item -ItemType Directory -Path (Join-Path $bundle 'msi') | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $tempRoot 'legacy-shared-bundle') | Out-Null
    Set-Content -LiteralPath (Join-Path $bundle 'msi\current.msi') -Value 'current candidate'
    Set-Content -LiteralPath (Join-Path $bundle 'ignored.txt') -Value 'not an installer'
    Set-Content -LiteralPath (Join-Path $tempRoot 'legacy-shared-bundle\stale.msi') -Value 'stale'

    $manifest = Publish-IsolatedBundle `
        -TargetDir $target `
        -BundleDir $bundle `
        -ReleaseDir $release `
        -Commit $baseline.commit `
        -Variant 'baseline' `
        -Features @() `
        -BuildId 'fixture-manifest'
    if ($manifest.artifacts.Count -ne 1 -or
        $manifest.artifacts[0].relativePath -ne 'msi/current.msi') {
        throw 'release manifest collected stale or non-installer files'
    }
    $copied = Join-Path $release 'msi\current.msi'
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $copied).Hash.ToLowerInvariant()
    if ($manifest.artifacts[0].sha256 -ne $actualHash) {
        throw 'release manifest hash does not match the copied artifact'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $release 'release-manifest.json'))) {
        throw 'release manifest file was not written'
    }

    $gitFixture = Join-Path $tempRoot 'git-fixture'
    New-Item -ItemType Directory -Path $gitFixture | Out-Null
    & git -C $gitFixture init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'could not initialize Git cleanliness fixture' }
    & git -C $gitFixture config user.name 'AgentTheSpire Test'
    & git -C $gitFixture config user.email 'agentthespire-test@example.invalid'
    & git -C $gitFixture config core.autocrlf false
    $trackedPath = Join-Path $gitFixture 'tracked.txt'
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($trackedPath, "original`n", $utf8NoBom)
    & git -C $gitFixture add tracked.txt
    & git -C $gitFixture commit --quiet -m 'fixture'
    if ($LASTEXITCODE -ne 0) { throw 'could not commit Git cleanliness fixture' }

    if (-not (Test-GitWorkingTreeClean -RepositoryRoot $gitFixture)) {
        throw 'clean Git fixture was reported dirty'
    }
    [System.IO.File]::WriteAllText($trackedPath, "original`n", $utf8NoBom)
    [System.IO.File]::SetLastWriteTimeUtc($trackedPath, [DateTime]::UtcNow.AddSeconds(2))
    if (-not (Test-GitWorkingTreeClean -RepositoryRoot $gitFixture)) {
        throw 'content-identical tracked rewrite was reported dirty'
    }

    [System.IO.File]::WriteAllText($trackedPath, "modified`n", $utf8NoBom)
    if (Test-GitWorkingTreeClean -RepositoryRoot $gitFixture) {
        throw 'tracked working tree change was reported clean'
    }
    & git -C $gitFixture add tracked.txt
    if (Test-GitWorkingTreeClean -RepositoryRoot $gitFixture) {
        throw 'staged change was reported clean'
    }

    [System.IO.File]::WriteAllText($trackedPath, "original`n", $utf8NoBom)
    & git -C $gitFixture add tracked.txt
    [System.IO.File]::WriteAllText((Join-Path $gitFixture 'untracked.txt'), "untracked`n", $utf8NoBom)
    if (Test-GitWorkingTreeClean -RepositoryRoot $gitFixture) {
        throw 'untracked file was reported clean'
    }
} finally {
    $resolvedTemp = [System.IO.Path]::GetFullPath($tempRoot)
    if ($resolvedTemp.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedTemp).StartsWith('ats-release-build-test-')) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host 'build plan tests passed'
