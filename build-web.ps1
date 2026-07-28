# AgentTheSpire -- ats-web production build (native binary + optional Docker image)
#
# Examples:
#   .\build-web.ps1                                   Native binary only
#   .\build-web.ps1 -Docker                           Binary + Docker image (tag agentthespire-web:dev)
#   .\build-web.ps1 -Docker -ImageTag x:1.0           Custom tag
#   .\build-web.ps1 -Docker -Platform linux/arm64     Cross-build

[CmdletBinding()]
param(
    [switch]$Docker,
    [string]$ImageTag = 'agentthespire-web:dev',
    [string]$Platform = 'linux/amd64'
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

& "$PSScriptRoot\scripts\ensure-node-deps.ps1" -Root $PSScriptRoot

Write-Host '==> npm run build:web' -ForegroundColor Cyan
npm run build:web
if ($LASTEXITCODE -ne 0) { throw "frontend build failed" }

Write-Host '==> cargo build -p ats-web --release' -ForegroundColor Cyan
cargo build -p ats-web --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$ext = if ($IsWindows) { '.exe' } else { '' }
$binaryPath = Join-Path $PSScriptRoot "target\release\ats-web$ext"
Write-Host "`n==> Native binary: $binaryPath" -ForegroundColor Green

if ($Docker) {
    Write-Host "`n==> Building Docker image $ImageTag (target $Platform)" -ForegroundColor Cyan
    docker buildx build `
        --platform $Platform `
        --tag $ImageTag `
        --load `
        --file Dockerfile `
        .
    if ($LASTEXITCODE -ne 0) { throw "docker build failed" }

    Write-Host "`n==> Docker image built: $ImageTag" -ForegroundColor Green
    Write-Host "    Run with: docker run --rm -p 7860:7860 $ImageTag" -ForegroundColor DarkGray
}
