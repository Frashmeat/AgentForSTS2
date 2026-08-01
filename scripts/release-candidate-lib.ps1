function Assert-ReleaseCandidateIdentifier {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Value,
        [int]$MaxLength = 96
    )

    if ([string]::IsNullOrWhiteSpace($Value) -or
        $Value.Length -gt $MaxLength -or
        $Value -notmatch '^[A-Za-z0-9._-]+$') {
        throw "$Name must contain 1-$MaxLength ASCII identifier characters"
    }
}

function Resolve-ReleaseCandidateVariants {
    param(
        [Parameter(Mandatory = $true)][string]$Variant
    )

    switch ($Variant.ToLowerInvariant()) {
        'baseline' { return @('baseline') }
        'ml' { return @('ml') }
        'all' { return @('baseline', 'ml') }
        default { throw 'Variant must be Baseline, Ml, or All' }
    }
}

function New-ReleaseCandidateStep {
    param(
        [Parameter(Mandatory = $true)][string]$Id
    )

    return [pscustomobject][ordered]@{
        id = $Id
        status = 'pending'
        startedAt = $null
        completedAt = $null
        exitCode = $null
        summary = $null
    }
}

function Get-ReleaseCandidateSteps {
    param(
        [Parameter(Mandatory = $true)][string[]]$Variants
    )

    $steps = @(
        New-ReleaseCandidateStep -Id 'preflight'
        New-ReleaseCandidateStep -Id 'frontend-tests'
        New-ReleaseCandidateStep -Id 'frontend-build'
        New-ReleaseCandidateStep -Id 'workspace-check'
        New-ReleaseCandidateStep -Id 'workspace-test'
        New-ReleaseCandidateStep -Id 'workspace-clippy'
    )
    foreach ($variant in $Variants) {
        $steps += New-ReleaseCandidateStep -Id "build-$variant"
        $steps += New-ReleaseCandidateStep -Id "verify-$variant"
    }
    return @($steps)
}

function Get-ReleaseCandidateFailureSummary {
    param(
        [Parameter(Mandatory = $true)][string]$StepId
    )

    switch ($StepId) {
        'preflight' { return 'candidate preflight failed' }
        'frontend-tests' { return 'frontend tests failed' }
        'frontend-build' { return 'frontend production build failed' }
        'workspace-check' { return 'Rust workspace check failed' }
        'workspace-test' { return 'Rust workspace tests failed' }
        'workspace-clippy' { return 'Rust workspace clippy failed' }
        'build-baseline' { return 'baseline candidate build failed' }
        'verify-baseline' { return 'baseline release manifest verification failed' }
        'build-ml' { return 'ML candidate build failed' }
        'verify-ml' { return 'ML release manifest verification failed' }
        default { return 'release candidate step failed' }
    }
}

function New-ReleaseVerification {
    param(
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$BuildId,
        [Parameter(Mandatory = $true)][string[]]$Variants
    )

    return [pscustomobject][ordered]@{
        schemaVersion = 1
        candidate = [pscustomobject][ordered]@{
            commit = $Commit
            buildId = $BuildId
            requestedVariants = @($Variants)
        }
        status = 'running'
        startedAt = [DateTime]::UtcNow.ToString('o')
        completedAt = $null
        steps = @(Get-ReleaseCandidateSteps -Variants $Variants)
        variants = @()
        failure = $null
    }
}

function Write-ReleaseVerificationAtomic {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Verification
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $parent = Split-Path -Parent $fullPath
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        New-Item -ItemType Directory -Path $parent | Out-Null
    }

    $operationId = [Guid]::NewGuid().ToString('N')
    $tempPath = "$fullPath.tmp-$operationId"
    $backupPath = "$fullPath.backup-$operationId"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    try {
        $json = $Verification | ConvertTo-Json -Depth 12
        [System.IO.File]::WriteAllText($tempPath, $json, $utf8NoBom)
        if (Test-Path -LiteralPath $fullPath -PathType Leaf) {
            [System.IO.File]::Replace($tempPath, $fullPath, $backupPath)
        } else {
            [System.IO.File]::Move($tempPath, $fullPath)
        }
    } finally {
        Remove-Item -LiteralPath $tempPath -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $backupPath -Force -ErrorAction SilentlyContinue
    }
}

function Assert-ReleaseCandidateRootAvailable {
    param(
        [Parameter(Mandatory = $true)][string]$CandidateRoot
    )

    if (Test-Path -LiteralPath $CandidateRoot) {
        throw "release candidate directory already exists: $CandidateRoot"
    }
}

function Test-PathWithinRoot {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Candidate
    )

    $rootPath = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    $candidatePath = [System.IO.Path]::GetFullPath($Candidate)
    return $candidatePath.StartsWith(
        $rootPath + [System.IO.Path]::DirectorySeparatorChar,
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

function ConvertTo-RepositoryRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$RepositoryRoot,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $root = [System.IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\', '/')
    $candidate = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-PathWithinRoot -Root $root -Candidate $candidate)) {
        throw 'path must be inside the repository root'
    }
    return ($candidate.Substring($root.Length).TrimStart('\', '/') -replace '\\', '/')
}

function Assert-ReleaseManifestFields {
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$Variant,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Features,
        [Parameter(Mandatory = $true)][string]$BuildId
    )

    $properties = @($Manifest.PSObject.Properties.Name)
    foreach ($required in @('schemaVersion', 'commit', 'variant', 'features', 'buildId', 'artifacts')) {
        if ($required -notin $properties) {
            throw "release manifest is missing required field: $required"
        }
    }
    $actualFeatures = @($Manifest.features)
    if ($Manifest.schemaVersion -ne 1 -or
        $Manifest.commit -ne $Commit -or
        $Manifest.variant -ne $Variant -or
        $Manifest.buildId -ne $BuildId -or
        $actualFeatures.Count -ne $Features.Count -or
        ($actualFeatures -join ',') -ne ($Features -join ',')) {
        throw 'release manifest identity does not match the requested candidate'
    }
}

function Read-VerifiedReleaseManifest {
    param(
        [Parameter(Mandatory = $true)][string]$ReleaseDir,
        [Parameter(Mandatory = $true)][string]$ManifestReference,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$Variant,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Features,
        [Parameter(Mandatory = $true)][string]$BuildId
    )

    $releaseRoot = [System.IO.Path]::GetFullPath($ReleaseDir).TrimEnd('\', '/')
    $manifestPath = Join-Path $releaseRoot 'release-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "release manifest was not produced for variant: $Variant"
    }
    try {
        $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    } catch {
        throw "release manifest is not valid JSON for variant: $Variant"
    }
    Assert-ReleaseManifestFields `
        -Manifest $manifest `
        -Commit $Commit `
        -Variant $Variant `
        -Features $Features `
        -BuildId $BuildId

    $allowedExtensions = @('.msi', '.exe', '.dmg', '.deb', '.AppImage')
    $seenPaths = @{}
    $artifactRecords = @()
    foreach ($artifact in @($manifest.artifacts)) {
        $relativePath = [string]$artifact.relativePath
        $normalizedPath = $relativePath -replace '\\', '/'
        if ([string]::IsNullOrWhiteSpace($relativePath) -or
            [System.IO.Path]::IsPathRooted($relativePath) -or
            $normalizedPath -match '(^|/)\.\.?(/|$)') {
            throw "release manifest contains an unsafe artifact path for variant: $Variant"
        }
        if ($seenPaths.ContainsKey($normalizedPath)) {
            throw "release manifest contains a duplicate artifact path for variant: $Variant"
        }
        $seenPaths[$normalizedPath] = $true

        $artifactPath = Join-Path $releaseRoot $relativePath
        if (-not (Test-PathWithinRoot -Root $releaseRoot -Candidate $artifactPath) -or
            -not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
            throw "release artifact is missing or outside the release root for variant: $Variant"
        }
        $file = Get-Item -LiteralPath $artifactPath
        if ($file.Extension -cnotin $allowedExtensions) {
            throw "release manifest contains an unsupported artifact type for variant: $Variant"
        }
        $expectedHash = [string]$artifact.sha256
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifactPath).Hash.ToLowerInvariant()
        if ([Int64]$artifact.byteLength -ne $file.Length -or
            $expectedHash -notmatch '^[0-9a-f]{64}$' -or
            $expectedHash -ne $actualHash) {
            throw "release artifact size or hash does not match the manifest for variant: $Variant"
        }
        $artifactRecords += [pscustomobject][ordered]@{
            relativePath = $normalizedPath
            byteLength = $file.Length
            sha256 = $actualHash
        }
    }
    if ($artifactRecords.Count -eq 0) {
        throw "release manifest contains no artifacts for variant: $Variant"
    }

    return [pscustomobject][ordered]@{
        variant = $Variant
        features = @($Features)
        buildId = $BuildId
        releaseManifest = ($ManifestReference -replace '\\', '/')
        runtimeBuildInfoVerified = $false
        artifacts = @($artifactRecords)
        verified = $true
    }
}

function Invoke-ReleaseCandidateWorkflow {
    param(
        [Parameter(Mandatory = $true)]$Verification,
        [Parameter(Mandatory = $true)][string]$VerificationPath,
        [Parameter(Mandatory = $true)][scriptblock]$StepInvoker
    )

    Write-ReleaseVerificationAtomic -Path $VerificationPath -Verification $Verification
    foreach ($step in @($Verification.steps)) {
        $step.status = 'running'
        $step.startedAt = [DateTime]::UtcNow.ToString('o')
        Write-ReleaseVerificationAtomic -Path $VerificationPath -Verification $Verification

        $stepResult = $null
        $stepFailed = $false
        $exitCode = 1
        try {
            $stepResult = & $StepInvoker $step
            if ($null -ne $stepResult -and $null -ne $stepResult.exitCode) {
                $exitCode = [int]$stepResult.exitCode
            }
            $stepFailed = $exitCode -ne 0
        } catch {
            $stepFailed = $true
            $exitCode = 1
        }

        $isVerifyStep = $step.id.StartsWith('verify-')
        if (-not $stepFailed -and -not $isVerifyStep -and $null -ne $stepResult.variant) {
            $stepFailed = $true
            $exitCode = 1
        }

        if (-not $stepFailed -and $isVerifyStep) {
            $variantId = $step.id.Substring('verify-'.Length)
            $buildStep = @($Verification.steps | Where-Object { $_.id -eq "build-$variantId" })
            if ($buildStep.Count -ne 1 -or
                $buildStep[0].status -ne 'succeeded' -or
                $null -eq $stepResult.variant -or
                $stepResult.variant.variant -ne $variantId -or
                -not $stepResult.variant.verified) {
                $stepFailed = $true
                $exitCode = 1
            } else {
                $stepResult.variant.runtimeBuildInfoVerified = $true
            }
        }

        $step.completedAt = [DateTime]::UtcNow.ToString('o')
        $step.exitCode = $exitCode
        if ($stepFailed) {
            $summary = Get-ReleaseCandidateFailureSummary -StepId $step.id
            $step.status = 'failed'
            $step.summary = $summary
            foreach ($remaining in @($Verification.steps | Where-Object { $_.status -eq 'pending' })) {
                $remaining.status = 'skipped'
                $remaining.summary = "blocked by $($step.id)"
            }
            $Verification.status = 'failed'
            $Verification.completedAt = [DateTime]::UtcNow.ToString('o')
            $Verification.failure = [pscustomobject][ordered]@{
                stepId = $step.id
                exitCode = $exitCode
                summary = $summary
            }
            Write-ReleaseVerificationAtomic -Path $VerificationPath -Verification $Verification
            return $Verification
        }

        $step.status = 'succeeded'
        if ($isVerifyStep) {
            $Verification.variants = @($Verification.variants) + @($stepResult.variant)
        }
        Write-ReleaseVerificationAtomic -Path $VerificationPath -Verification $Verification
    }

    $expectedVariants = @($Verification.candidate.requestedVariants)
    $actualVariants = @($Verification.variants | ForEach-Object { $_.variant })
    $verifiedVariants = @(
        $Verification.variants | Where-Object {
            $_.verified -eq $true -and $_.runtimeBuildInfoVerified -eq $true
        }
    )
    if ($actualVariants.Count -ne $expectedVariants.Count -or
        ($actualVariants -join ',') -ne ($expectedVariants -join ',') -or
        $verifiedVariants.Count -ne $expectedVariants.Count) {
        $verifySteps = @($Verification.steps | Where-Object { $_.id.StartsWith('verify-') })
        $failedStep = $verifySteps[$verifySteps.Count - 1]
        $summary = Get-ReleaseCandidateFailureSummary -StepId $failedStep.id
        $failedStep.status = 'failed'
        $failedStep.exitCode = 1
        $failedStep.completedAt = [DateTime]::UtcNow.ToString('o')
        $failedStep.summary = $summary
        $Verification.status = 'failed'
        $Verification.completedAt = [DateTime]::UtcNow.ToString('o')
        $Verification.failure = [pscustomobject][ordered]@{
            stepId = $failedStep.id
            exitCode = 1
            summary = $summary
        }
        Write-ReleaseVerificationAtomic -Path $VerificationPath -Verification $Verification
        return $Verification
    }

    $Verification.status = 'succeeded'
    $Verification.completedAt = [DateTime]::UtcNow.ToString('o')
    $Verification.failure = $null
    Write-ReleaseVerificationAtomic -Path $VerificationPath -Verification $Verification
    return $Verification
}
