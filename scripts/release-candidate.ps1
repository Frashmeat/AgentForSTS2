# AgentTheSpire Windows release candidate orchestrator.
#
# Usage:
#   .\scripts\release-candidate.ps1 -Variant All
#   .\scripts\release-candidate.ps1 -Variant Baseline -PlanOnly
#   .\scripts\release-candidate.ps1 -Variant Ml -BuildId rc-local -PlanOnly

[CmdletBinding()]
param(
    [ValidateSet('Baseline', 'Ml', 'All')]
    [string]$Variant = 'All',
    [string]$BuildId,
    [switch]$PlanOnly
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$buildScript = Join-Path $repoRoot 'build.ps1'
. (Join-Path $PSScriptRoot 'release-candidate-lib.ps1')
Set-Location $repoRoot

function Read-CandidateBuildPlan {
    param(
        [Parameter(Mandatory = $true)][string]$VariantName,
        [Parameter(Mandatory = $true)][string]$CandidateBuildId
    )

    $buildVariant = if ($VariantName -eq 'ml') { 'Ml' } else { 'Baseline' }
    $json = & $buildScript -Variant $buildVariant -PlanOnly -BuildId $CandidateBuildId
    if ($LASTEXITCODE -ne 0) {
        throw "build plan failed for variant: $VariantName"
    }
    try {
        return (($json -join [Environment]::NewLine) | ConvertFrom-Json)
    } catch {
        throw "build plan returned invalid JSON for variant: $VariantName"
    }
}

$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-fA-F]{40}$') {
    throw 'A full Git commit SHA is required for a release candidate'
}
if ([string]::IsNullOrWhiteSpace($BuildId)) {
    $timestamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
    $BuildId = "rc-$timestamp-$($commit.Substring(0, 12))"
}
Assert-ReleaseCandidateIdentifier -Name 'BuildId' -Value $BuildId

$variants = @(Resolve-ReleaseCandidateVariants -Variant $Variant)
$buildPlans = @()
foreach ($variantId in $variants) {
    $plan = Read-CandidateBuildPlan -VariantName $variantId -CandidateBuildId $BuildId
    if ($plan.commit -ne $commit -or
        $plan.variant -ne $variantId -or
        $plan.buildId -ne $BuildId) {
        throw "build plan identity mismatch for variant: $variantId"
    }
    $buildPlans += $plan
}

$candidateRoots = @($buildPlans | ForEach-Object {
        [System.IO.Path]::GetFullPath((Split-Path -Parent $_.releaseDir)).TrimEnd('\', '/')
    } | Select-Object -Unique)
if ($candidateRoots.Count -ne 1) {
    throw 'build plans did not resolve to one candidate release root'
}
$candidateRoot = $candidateRoots[0]
$verificationPath = Join-Path $candidateRoot 'release-verification.json'
$verificationReference = ConvertTo-RepositoryRelativePath `
    -RepositoryRoot $repoRoot `
    -Path $verificationPath
$steps = @(Get-ReleaseCandidateSteps -Variants $variants)

$candidatePlan = [pscustomobject][ordered]@{
    schemaVersion = 1
    candidate = [pscustomobject][ordered]@{
        commit = $commit
        buildId = $BuildId
        requestedVariants = @($variants)
    }
    workingTreeClean = @($buildPlans | Where-Object { -not $_.workingTreeClean }).Count -eq 0
    verificationPath = $verificationReference
    steps = @($steps | ForEach-Object { $_.id })
    variants = @($buildPlans | ForEach-Object {
        [pscustomobject][ordered]@{
            variant = $_.variant
            features = @($_.features)
            targetDir = $_.targetDir
            releaseDir = $_.releaseDir
            manifestPath = $_.manifestPath
        }
    })
}

if ($PlanOnly) {
    $candidatePlan | ConvertTo-Json -Depth 8
    return
}

Assert-ReleaseCandidateRootAvailable -CandidateRoot $candidateRoot
$verification = New-ReleaseVerification `
    -Commit $commit `
    -BuildId $BuildId `
    -Variants $variants
$plansByVariant = @{}
foreach ($plan in $buildPlans) {
    $plansByVariant[$plan.variant] = $plan
}

$stepInvoker = {
    param($Step)

    switch ($Step.id) {
        'preflight' {
            $clean = @($buildPlans | Where-Object { -not $_.workingTreeClean }).Count -eq 0
            return [pscustomobject]@{ exitCode = $(if ($clean) { 0 } else { 1 }); variant = $null }
        }
        'frontend-tests' {
            & (Join-Path $repoRoot 'scripts\ensure-node-deps.ps1') -Root $repoRoot | Out-Host
            & npm run test:frontend | Out-Host
            return [pscustomobject]@{ exitCode = $LASTEXITCODE; variant = $null }
        }
        'frontend-build' {
            & npm run build:web | Out-Host
            return [pscustomobject]@{ exitCode = $LASTEXITCODE; variant = $null }
        }
        'workspace-check' {
            & cargo check --workspace --all-targets | Out-Host
            return [pscustomobject]@{ exitCode = $LASTEXITCODE; variant = $null }
        }
        'workspace-test' {
            & cargo test --workspace --all-targets | Out-Host
            return [pscustomobject]@{ exitCode = $LASTEXITCODE; variant = $null }
        }
        'workspace-clippy' {
            & cargo clippy --workspace --all-targets -- -D warnings | Out-Host
            return [pscustomobject]@{ exitCode = $LASTEXITCODE; variant = $null }
        }
        { $_ -eq 'build-baseline' -or $_ -eq 'build-ml' } {
            $variantId = $Step.id.Substring('build-'.Length)
            $buildVariant = if ($variantId -eq 'ml') { 'Ml' } else { 'Baseline' }
            & $buildScript -Variant $buildVariant -BuildId $BuildId | Out-Host
            return [pscustomobject]@{ exitCode = 0; variant = $null }
        }
        { $_ -eq 'verify-baseline' -or $_ -eq 'verify-ml' } {
            $variantId = $Step.id.Substring('verify-'.Length)
            $plan = $plansByVariant[$variantId]
            $manifestReference = ConvertTo-RepositoryRelativePath `
                -RepositoryRoot $repoRoot `
                -Path $plan.manifestPath
            $variantRecord = Read-VerifiedReleaseManifest `
                -ReleaseDir $plan.releaseDir `
                -ManifestReference $manifestReference `
                -Commit $commit `
                -Variant $variantId `
                -Features @($plan.features) `
                -BuildId $BuildId
            return [pscustomobject]@{ exitCode = 0; variant = $variantRecord }
        }
        default {
            return [pscustomobject]@{ exitCode = 2; variant = $null }
        }
    }
}

$result = Invoke-ReleaseCandidateWorkflow `
    -Verification $verification `
    -VerificationPath $verificationPath `
    -StepInvoker $stepInvoker
$result | ConvertTo-Json -Depth 12
if ($result.status -ne 'succeeded') {
    exit 1
}
