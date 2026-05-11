# scripts/ci-local.ps1 — run the GitHub Actions Rust CI locally inside Docker.
#
# Usage:
#   .\scripts\ci-local.ps1                 # full CI mirror (check + test + tsc + build)
#   .\scripts\ci-local.ps1 -CheckOnly      # just cargo check (fastest)
#   .\scripts\ci-local.ps1 -Clippy         # just cargo clippy
#   .\scripts\ci-local.ps1 -Shell          # drop into interactive bash
#   .\scripts\ci-local.ps1 -Rebuild        # force rebuild the image
#
# First run builds the image (~5-8 min for apt + rustup + node).
# Subsequent runs reuse the image AND a named volume for cargo target +
# registry, so iterating is fast.
#
# Why so little PS logic: we deliberately keep this script thin and push all
# steps into scripts/ci-inner.sh — that way PS 5.1's argument parsing can't
# mangle `-i` / `-v` / etc. that docker run needs.

[CmdletBinding()]
param(
    [switch]$CheckOnly,
    [switch]$Clippy,
    [switch]$Shell,
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$ImageTag = "ats-ci-local:latest"
$TargetVolume = "ats-ci-target"
$CargoRegistryVolume = "ats-ci-cargo-registry"

Write-Host "Repo:  $RepoRoot"
Write-Host "Image: $ImageTag"

# --- 1. Build image if needed ---
$imageExists = [bool](docker images -q $ImageTag)
if ($Rebuild -or -not $imageExists) {
    Write-Host ""
    Write-Host "==> Building image (5-8 min the first time)"
    docker build -f (Join-Path $RepoRoot "Dockerfile.ci") -t $ImageTag $RepoRoot
    if ($LASTEXITCODE -ne 0) { throw "docker build failed" }
}

# --- 2. Ensure cache volumes exist ---
foreach ($vol in @($TargetVolume, $CargoRegistryVolume)) {
    $existing = docker volume ls -q --filter "name=^$vol$"
    if (-not $existing) {
        Write-Host "==> Creating volume: $vol"
        docker volume create $vol | Out-Null
    }
}

# --- 3. Decide mode ---
if ($Shell) {
    $mode = "shell"
} elseif ($CheckOnly) {
    $mode = "check"
} elseif ($Clippy) {
    $mode = "clippy"
} else {
    $mode = "full"
}

Write-Host ""
Write-Host "==> Mode: $mode"
Write-Host "==> Running CI inside container"
Write-Host ""

# --- 4. docker run ---
# Build the command as a single string. Native exe arg parsing is consistent
# across PS 5.1 / 7 when we pass distinct positional args (no splat).
# The repo path gets quoted because Windows paths contain backslashes.
$mountRepo     = "${RepoRoot}:/workspace"
$mountTarget   = "${TargetVolume}:/workspace/target"
$mountRegistry = "${CargoRegistryVolume}:/usr/local/cargo/registry"

if ($mode -eq "shell") {
    # Interactive bash. -it required.
    docker run --rm -it `
        -v "$mountRepo" `
        -v "$mountTarget" `
        -v "$mountRegistry" `
        -e "RUSTFLAGS=-D warnings" `
        -e "CARGO_TERM_COLOR=always" `
        -e "CARGO_INCREMENTAL=0" `
        -w /workspace `
        $ImageTag `
        bash
} else {
    docker run --rm `
        -v "$mountRepo" `
        -v "$mountTarget" `
        -v "$mountRegistry" `
        -e "RUSTFLAGS=-D warnings" `
        -e "CARGO_TERM_COLOR=always" `
        -e "CARGO_INCREMENTAL=0" `
        -w /workspace `
        $ImageTag `
        bash /workspace/scripts/ci-inner.sh $mode
}
$exit = $LASTEXITCODE

Write-Host ""
if ($exit -eq 0) {
    Write-Host "==> CI mirror PASSED (exit $exit) - safe to push"
} else {
    Write-Host "==> CI mirror FAILED (exit $exit) - do NOT push"
}
exit $exit
