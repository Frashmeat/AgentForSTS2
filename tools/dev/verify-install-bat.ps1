$ErrorActionPreference = 'Stop'

$toolsRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$installBat = Join-Path $toolsRoot 'install.bat'
$installWrapperPs1 = Join-Path $toolsRoot 'install.ps1'
$nestedInstallBat = Join-Path $toolsRoot 'install\install.bat'
$installPs1 = Join-Path $toolsRoot 'install\install.ps1'
$wrapperBatContent = Get-Content $installBat -Raw
$wrapperPsContent = Get-Content $installWrapperPs1 -Raw
$nestedBatContent = Get-Content $nestedInstallBat -Raw
$psContent = Get-Content $installPs1 -Raw
$bytes = [System.IO.File]::ReadAllBytes($installBat)

$checks = @(
    @{
        Name = 'bat wrapper forwards to install/install.bat'
        Pattern = 'install\install.bat'
    }
    @{
        Name = 'powershell wrapper forwards to install/install.ps1'
        Pattern = 'install\install.ps1'
    }
    @{
        Name = 'nested install bat forwards to install.ps1'
        Pattern = 'install.ps1'
    }
    @{
        Name = 'powershell entry supports OnlyModDeps'
        Pattern = 'OnlyModDeps'
    }
    @{
        Name = 'powershell entry prewarms rembg model cache'
        Pattern = 'new_session(model)'
    }
    @{
        Name = 'powershell entry handles dotnet install script'
        Pattern = 'dotnet-install.ps1'
    }
    @{
        Name = 'powershell entry writes Godot path into config'
        Pattern = 'Set-GodotPathInConfig'
    }
    @{
        Name = 'powershell entry installs frontend dependencies'
        Pattern = 'Ensure-FrontendDependencies'
    }
    @{
        Name = 'powershell entry exposes local image installation switch'
        Pattern = 'InstallLocalImage'
    }
    @{
        Name = 'nested install bat pauses after execution'
        Pattern = 'pause'
    }
)

$failed = @()
foreach ($check in $checks) {
    if ($check.Name -like 'bat wrapper*') {
        $target = $wrapperBatContent
    } elseif ($check.Name -like 'powershell wrapper*') {
        $target = $wrapperPsContent
    } elseif ($check.Name -like 'nested install bat*') {
        $target = $nestedBatContent
    } else {
        $target = $psContent
    }

    if ($target -notmatch [regex]::Escape($check.Pattern)) {
        $failed += $check.Name
    }
}

if ($failed.Count -gt 0) {
    Write-Error ("install.bat verification failed: missing " + ($failed -join ', '))
}

if ($wrapperBatContent -match 'call "%SCRIPT_DIR%install\\install\.bat" %\*' -eq $false) {
    Write-Error 'install.bat verification failed: wrapper does not forward to install\install.bat'
}

if ($wrapperPsContent -match 'install\\install\.ps1' -eq $false) {
    Write-Error 'install.ps1 verification failed: wrapper does not forward to install\install.ps1'
}

if ($nestedBatContent -match 'powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%install\.ps1" %\*' -eq $false) {
    Write-Error 'install\\install.bat verification failed: wrapper does not forward arguments to nested install.ps1'
}

if ($psContent -match 'npm install --silent') {
    Write-Error 'install.ps1 verification failed: npm install is still silent'
}

if ($psContent -match '--quiet') {
    Write-Error 'install.ps1 verification failed: pip install is still quiet'
}

$crlf = 0
$loneLf = 0
for ($i = 0; $i -lt $bytes.Length; $i++) {
    if ($bytes[$i] -eq 10) {
        if ($i -gt 0 -and $bytes[$i - 1] -eq 13) {
            $crlf++
        } else {
            $loneLf++
        }
    }
}

if ($loneLf -gt 0) {
    Write-Error "install.bat verification failed: found $loneLf lone LF line endings"
}

Write-Host 'install wrapper/install.ps1 verification passed'
