$ErrorActionPreference = 'Stop'

$installBat = Join-Path $PSScriptRoot 'install.bat'
$installPs1 = Join-Path $PSScriptRoot 'install.ps1'
$content = Get-Content $installBat -Raw
$psContent = Get-Content $installPs1 -Raw
$bytes = [System.IO.File]::ReadAllBytes($installBat)

$checks = @(
    @{
        Name = 'bat wrapper forwards to install.ps1'
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
        Name = 'bat wrapper pauses after execution'
        Pattern = 'pause'
    }
)

$failed = @()
foreach ($check in $checks) {
    $target = if ($check.Name -like 'bat wrapper*') { $content } else { $psContent }
    if ($target -notmatch [regex]::Escape($check.Pattern)) {
        $failed += $check.Name
    }
}

if ($failed.Count -gt 0) {
    Write-Error ("install.bat verification failed: missing " + ($failed -join ', '))
}

if ($content -match 'powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%install.ps1" %\*' -eq $false) {
    Write-Error 'install.bat verification failed: wrapper does not forward arguments to install.ps1'
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

Write-Host 'install.bat/install.ps1 verification passed'
