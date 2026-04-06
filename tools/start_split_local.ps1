param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ForwardedArgs
)

$target = Join-Path $PSScriptRoot "split-local\start_split_local.ps1"
$powershellCommand = Get-Command powershell -ErrorAction SilentlyContinue
if ($null -eq $powershellCommand) {
    throw "未找到 powershell，无法执行：$target"
}
& $powershellCommand.Source -NoProfile -ExecutionPolicy Bypass -File $target @ForwardedArgs
