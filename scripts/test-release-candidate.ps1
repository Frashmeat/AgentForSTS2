[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$candidateScript = Join-Path $PSScriptRoot 'release-candidate.ps1'
. (Join-Path $PSScriptRoot 'release-candidate-lib.ps1')

function Assert-True {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )
    if (-not $Condition) { throw $Message }
}

function Write-FixtureReleaseManifest {
    param(
        [Parameter(Mandatory = $true)][string]$ReleaseDir,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$Variant,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Features,
        [Parameter(Mandatory = $true)][string]$BuildId
    )

    $artifactDir = Join-Path $ReleaseDir 'nsis'
    New-Item -ItemType Directory -Path $artifactDir | Out-Null
    $artifactName = "fixture-$Variant-setup.exe"
    $artifactPath = Join-Path $artifactDir $artifactName
    [System.IO.File]::WriteAllText($artifactPath, "fixture-$Variant-$BuildId")
    $file = Get-Item -LiteralPath $artifactPath
    $manifest = [pscustomobject][ordered]@{
        schemaVersion = 1
        commit = $Commit
        variant = $Variant
        features = @($Features)
        buildId = $BuildId
        createdAt = [DateTime]::UtcNow.ToString('o')
        artifacts = @(
            [pscustomobject][ordered]@{
                relativePath = "nsis/$artifactName"
                byteLength = $file.Length
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifactPath).Hash.ToLowerInvariant()
            }
        )
    }
    $json = $manifest | ConvertTo-Json -Depth 8
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText((Join-Path $ReleaseDir 'release-manifest.json'), $json, $utf8NoBom)
}

$ciWorkflowPath = Join-Path $repoRoot '.github\workflows\rust-ci.yml'
$ciWorkflow = Get-Content -Raw -LiteralPath $ciWorkflowPath
$windowsJobMatch = [regex]::Match(
    $ciWorkflow,
    '(?ms)^  windows-desktop-variants:\s*$(.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\z)'
)
Assert-True -Condition $windowsJobMatch.Success -Message 'Windows desktop variants CI job is missing'
$windowsJob = $windowsJobMatch.Value
Assert-True `
    -Condition ($windowsJob.Contains('run: cargo check -p agentthespire-desktop --no-default-features')) `
    -Message 'Windows CI does not compile the baseline desktop variant'
Assert-True `
    -Condition ($windowsJob.Contains('run: cargo check -p agentthespire-desktop --features ml-rembg')) `
    -Message 'Windows CI does not compile the ML desktop variant'
Assert-True `
    -Condition (-not $windowsJob.Contains('tauri build')) `
    -Message 'ordinary Windows CI must not build a Tauri installer'

$planBuildId = "fixture-plan-$([Guid]::NewGuid().ToString('N'))"
$planCandidateRoot = Join-Path $repoRoot "artifacts\release\$planBuildId"
$planJson = & $candidateScript -Variant All -PlanOnly -BuildId $planBuildId
if ($LASTEXITCODE -ne 0) { throw 'release candidate PlanOnly failed' }
$plan = ($planJson -join [Environment]::NewLine) | ConvertFrom-Json
Assert-True `
    -Condition ($plan.candidate.buildId -eq $planBuildId) `
    -Message 'PlanOnly did not preserve the requested build ID'
Assert-True `
    -Condition (($plan.candidate.requestedVariants -join ',') -eq 'baseline,ml') `
    -Message 'All plan did not contain baseline and ML variants'
Assert-True `
    -Condition ($plan.variants[0].features.Count -eq 0) `
    -Message 'baseline plan declared an unexpected feature'
Assert-True `
    -Condition (($plan.variants[1].features -join ',') -eq 'ml-rembg') `
    -Message 'ML plan did not inherit ml-rembg from build.ps1'
Assert-True `
    -Condition (-not (Test-Path -LiteralPath $planCandidateRoot)) `
    -Message 'PlanOnly created a release candidate directory'

$baselinePlanId = "fixture-baseline-plan-$([Guid]::NewGuid().ToString('N'))"
$baselinePlanJson = & $candidateScript -Variant Baseline -PlanOnly -BuildId $baselinePlanId
$baselinePlan = ($baselinePlanJson -join [Environment]::NewLine) | ConvertFrom-Json
Assert-True `
    -Condition (($baselinePlan.candidate.requestedVariants -join ',') -eq 'baseline') `
    -Message 'Baseline PlanOnly included another variant'
Assert-True `
    -Condition (@($baselinePlan.steps | Where-Object { $_ -like '*-ml' }).Count -eq 0) `
    -Message 'Baseline PlanOnly included an ML step'

$mlPlanId = "fixture-ml-plan-$([Guid]::NewGuid().ToString('N'))"
$mlPlanJson = & $candidateScript -Variant Ml -PlanOnly -BuildId $mlPlanId
$mlPlan = ($mlPlanJson -join [Environment]::NewLine) | ConvertFrom-Json
Assert-True `
    -Condition (($mlPlan.candidate.requestedVariants -join ',') -eq 'ml') `
    -Message 'ML PlanOnly included another variant'
Assert-True `
    -Condition (($mlPlan.variants[0].features -join ',') -eq 'ml-rembg') `
    -Message 'ML PlanOnly lost the authoritative feature set'

$invalidIdentifierRejected = $false
try {
    Assert-ReleaseCandidateIdentifier -Name 'BuildId' -Value '../unsafe'
} catch {
    $invalidIdentifierRejected = $true
}
Assert-True -Condition $invalidIdentifierRejected -Message 'unsafe candidate build ID was accepted'

$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempRoot = Join-Path $tempBase ("ats-release-candidate-test-" + [Guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    $commit = '0123456789abcdef0123456789abcdef01234567'
    $buildId = 'fixture-candidate'
    $baselineDir = Join-Path $tempRoot 'baseline'
    $mlDir = Join-Path $tempRoot 'ml'
    Write-FixtureReleaseManifest `
        -ReleaseDir $baselineDir `
        -Commit $commit `
        -Variant 'baseline' `
        -Features @() `
        -BuildId $buildId
    Write-FixtureReleaseManifest `
        -ReleaseDir $mlDir `
        -Commit $commit `
        -Variant 'ml' `
        -Features @('ml-rembg') `
        -BuildId $buildId

    $successPath = Join-Path $tempRoot 'success\release-verification.json'
    $success = New-ReleaseVerification `
        -Commit $commit `
        -BuildId $buildId `
        -Variants @('baseline', 'ml')
    $successInvoker = {
        param($Step)
        if ($Step.id -eq 'verify-baseline') {
            $variant = Read-VerifiedReleaseManifest `
                -ReleaseDir $baselineDir `
                -ManifestReference 'artifacts/release/fixture-candidate/baseline/release-manifest.json' `
                -Commit $commit `
                -Variant 'baseline' `
                -Features @() `
                -BuildId $buildId
            return [pscustomobject]@{ exitCode = 0; variant = $variant }
        }
        if ($Step.id -eq 'verify-ml') {
            $variant = Read-VerifiedReleaseManifest `
                -ReleaseDir $mlDir `
                -ManifestReference 'artifacts/release/fixture-candidate/ml/release-manifest.json' `
                -Commit $commit `
                -Variant 'ml' `
                -Features @('ml-rembg') `
                -BuildId $buildId
            return [pscustomobject]@{ exitCode = 0; variant = $variant }
        }
        return [pscustomobject]@{ exitCode = 0; variant = $null }
    }
    $successResult = Invoke-ReleaseCandidateWorkflow `
        -Verification $success `
        -VerificationPath $successPath `
        -StepInvoker $successInvoker
    Assert-True -Condition ($successResult.status -eq 'succeeded') -Message 'success fixture did not succeed'
    Assert-True -Condition ($successResult.variants.Count -eq 2) -Message 'success fixture did not verify both variants'
    Assert-True `
        -Condition (@($successResult.steps | Where-Object { $_.status -ne 'succeeded' }).Count -eq 0) `
        -Message 'success fixture left a non-succeeded step'
    $persistedSuccess = Get-Content -Raw -LiteralPath $successPath | ConvertFrom-Json
    Assert-True -Condition ($persistedSuccess.status -eq 'succeeded') -Message 'persisted success JSON is incorrect'
    Assert-True `
        -Condition (@(Get-ChildItem -LiteralPath (Split-Path -Parent $successPath) -Filter '*.tmp-*').Count -eq 0) `
        -Message 'atomic verification write left a temporary file'

    $missingVariantPath = Join-Path $tempRoot 'missing-variant\release-verification.json'
    $missingVariant = New-ReleaseVerification `
        -Commit $commit `
        -BuildId 'fixture-missing-variant' `
        -Variants @('baseline')
    $missingVariantInvoker = {
        param($Step)
        return [pscustomobject]@{ exitCode = 0; variant = $null }
    }
    $missingVariantResult = Invoke-ReleaseCandidateWorkflow `
        -Verification $missingVariant `
        -VerificationPath $missingVariantPath `
        -StepInvoker $missingVariantInvoker
    Assert-True `
        -Condition ($missingVariantResult.status -eq 'failed') `
        -Message 'verify step accepted a missing variant result'
    Assert-True `
        -Condition ($missingVariantResult.failure.stepId -eq 'verify-baseline') `
        -Message 'missing variant result failed at the wrong step'
    Assert-True `
        -Condition ($missingVariantResult.variants.Count -eq 0) `
        -Message 'missing variant result published an invalid variant record'

    $wrongVariantPath = Join-Path $tempRoot 'wrong-variant\release-verification.json'
    $wrongVariant = New-ReleaseVerification `
        -Commit $commit `
        -BuildId 'fixture-wrong-variant' `
        -Variants @('baseline')
    $wrongVariantInvoker = {
        param($Step)
        if ($Step.id -eq 'verify-baseline') {
            return [pscustomobject]@{
                exitCode = 0
                variant = [pscustomobject]@{ variant = 'ml'; verified = $true }
            }
        }
        return [pscustomobject]@{ exitCode = 0; variant = $null }
    }
    $wrongVariantResult = Invoke-ReleaseCandidateWorkflow `
        -Verification $wrongVariant `
        -VerificationPath $wrongVariantPath `
        -StepInvoker $wrongVariantInvoker
    Assert-True `
        -Condition ($wrongVariantResult.status -eq 'failed') `
        -Message 'verify step accepted a mismatched variant result'
    Assert-True `
        -Condition ($wrongVariantResult.failure.stepId -eq 'verify-baseline') `
        -Message 'mismatched variant result failed at the wrong step'

    $baselineOnly = New-ReleaseVerification `
        -Commit $commit `
        -BuildId 'fixture-baseline-only' `
        -Variants @('baseline')
    Assert-True `
        -Condition (@($baselineOnly.steps | Where-Object { $_.id -like '*-ml' }).Count -eq 0) `
        -Message 'baseline-only workflow included an ML step'

    $failurePath = Join-Path $tempRoot 'failure\release-verification.json'
    $failure = New-ReleaseVerification `
        -Commit $commit `
        -BuildId 'fixture-failure' `
        -Variants @('baseline')
    $failureInvoker = {
        param($Step)
        if ($Step.id -eq 'workspace-test') {
            throw 'SECRET_CANARY=C:\private\candidate'
        }
        return [pscustomobject]@{ exitCode = 0; variant = $null }
    }
    $failureResult = Invoke-ReleaseCandidateWorkflow `
        -Verification $failure `
        -VerificationPath $failurePath `
        -StepInvoker $failureInvoker
    Assert-True -Condition ($failureResult.status -eq 'failed') -Message 'failure fixture did not fail'
    Assert-True `
        -Condition ($failureResult.failure.stepId -eq 'workspace-test') `
        -Message 'failure fixture recorded the wrong failed step'
    Assert-True `
        -Condition (@($failureResult.steps | Where-Object { $_.status -eq 'skipped' }).Count -gt 0) `
        -Message 'failure fixture did not skip dependent steps'
    $failureJson = Get-Content -Raw -LiteralPath $failurePath
    Assert-True `
        -Condition (-not $failureJson.Contains('SECRET_CANARY') -and -not $failureJson.Contains('C:\private')) `
        -Message 'verification persisted a raw exception or sensitive path'

    $identityRejected = $false
    try {
        Read-VerifiedReleaseManifest `
            -ReleaseDir $baselineDir `
            -ManifestReference 'baseline/release-manifest.json' `
            -Commit $commit `
            -Variant 'baseline' `
            -Features @() `
            -BuildId 'wrong-build-id' | Out-Null
    } catch {
        $identityRejected = $true
    }
    Assert-True -Condition $identityRejected -Message 'manifest verification accepted an identity mismatch'

    $baselineArtifact = Join-Path $baselineDir 'nsis\fixture-baseline-setup.exe'
    [System.IO.File]::AppendAllText($baselineArtifact, 'tampered')
    $hashRejected = $false
    try {
        Read-VerifiedReleaseManifest `
            -ReleaseDir $baselineDir `
            -ManifestReference 'baseline/release-manifest.json' `
            -Commit $commit `
            -Variant 'baseline' `
            -Features @() `
            -BuildId $buildId | Out-Null
    } catch {
        $hashRejected = $true
    }
    Assert-True -Condition $hashRejected -Message 'manifest verification accepted a tampered artifact'

    $existingRootRejected = $false
    try {
        Assert-ReleaseCandidateRootAvailable -CandidateRoot $tempRoot
    } catch {
        $existingRootRejected = $true
    }
    Assert-True -Condition $existingRootRejected -Message 'existing candidate root was not rejected'
} finally {
    $resolvedTemp = [System.IO.Path]::GetFullPath($tempRoot)
    if ($resolvedTemp.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedTemp).StartsWith('ats-release-candidate-test-')) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host 'release candidate tests passed'
