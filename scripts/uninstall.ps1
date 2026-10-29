# Remove the files installed by install.ps1. User settings are preserved.
param([string]$Directory = (Join-Path $env:LOCALAPPDATA 'Programs\Snipping Lite'))
$ErrorActionPreference = 'Stop'
$target = Join-Path $Directory 'snippinglite.exe'
if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Snipping Lite is not installed at $target" }
Get-Process -Name snippinglite -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $target } | Stop-Process -Force
Remove-Item -LiteralPath $target
foreach ($name in 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'uninstall.ps1') {
    Remove-Item -LiteralPath (Join-Path $Directory $name) -ErrorAction SilentlyContinue
}
# Only an empty folder is removed, so files the user added stay.
if (!(Get-ChildItem -LiteralPath $Directory -Force)) { Remove-Item -LiteralPath $Directory }
Write-Host 'Removed Snipping Lite. Settings were preserved.'
