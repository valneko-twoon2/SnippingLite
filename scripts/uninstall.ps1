# Remove the executable installed by install.ps1. User settings are preserved.
param([string]$Directory = (Join-Path $env:LOCALAPPDATA 'Programs\Snipping Lite'))
$ErrorActionPreference = 'Stop'
$target = Join-Path $Directory 'snippinglite.exe'
if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Snipping Lite is not installed at $target" }
Remove-Item -LiteralPath $target
Write-Host 'Removed the executable. Settings and other files were preserved.'
