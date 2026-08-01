function Publish-IsolatedBundle {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$TargetDir,
        [Parameter(Mandatory = $true)][string]$BundleDir,
        [Parameter(Mandatory = $true)][string]$ReleaseDir,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$Variant,
        [string[]]$Features = @(),
        [Parameter(Mandatory = $true)][string]$BuildId
    )

    if (-not (Test-Path -LiteralPath $BundleDir -PathType Container)) {
        throw "isolated bundle directory was not produced: $BundleDir"
    }
    if (Test-Path -LiteralPath $ReleaseDir) {
        throw "release directory already exists for build id: $BuildId"
    }

    $targetRoot = [System.IO.Path]::GetFullPath($TargetDir).TrimEnd('\', '/')
    $bundleRoot = [System.IO.Path]::GetFullPath($BundleDir).TrimEnd('\', '/')
    if (-not $bundleRoot.StartsWith(
            $targetRoot + [System.IO.Path]::DirectorySeparatorChar,
            [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'bundle directory must be inside the isolated target directory'
    }

    $allowedExtensions = @('.msi', '.exe', '.dmg', '.deb', '.AppImage')
    $bundleFiles = @(
        Get-ChildItem -LiteralPath $BundleDir -Recurse -File |
            Where-Object { $_.Extension -cin $allowedExtensions }
    )
    if ($bundleFiles.Count -eq 0) {
        throw 'the isolated build produced no installer artifacts'
    }

    New-Item -ItemType Directory -Path $ReleaseDir | Out-Null
    $artifactRecords = foreach ($file in $bundleFiles) {
        $fullPath = [System.IO.Path]::GetFullPath($file.FullName)
        if (-not $fullPath.StartsWith(
                $bundleRoot + [System.IO.Path]::DirectorySeparatorChar,
                [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "bundle artifact escaped the isolated bundle root: $fullPath"
        }
        $relativePath = $fullPath.Substring($bundleRoot.Length).TrimStart('\', '/')
        $destination = Join-Path $ReleaseDir $relativePath
        $destinationParent = Split-Path -Parent $destination
        New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null
        Copy-Item -LiteralPath $fullPath -Destination $destination
        $copied = Get-Item -LiteralPath $destination
        [ordered]@{
            relativePath = ($relativePath -replace '\\', '/')
            byteLength = $copied.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
        }
    }

    $manifest = [ordered]@{
        schemaVersion = 1
        commit = $Commit
        variant = $Variant
        features = $Features
        buildId = $BuildId
        createdAt = [DateTime]::UtcNow.ToString('o')
        artifacts = @($artifactRecords)
    }
    $manifestPath = Join-Path $ReleaseDir 'release-manifest.json'
    $manifestTemp = "$manifestPath.tmp"
    $manifestJson = $manifest | ConvertTo-Json -Depth 6
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($manifestTemp, $manifestJson, $utf8NoBom)
    Move-Item -LiteralPath $manifestTemp -Destination $manifestPath
    return $manifest
}
