# Install a locally built release executable for the current user.
param(
    [string]$Directory = (Join-Path $env:LOCALAPPDATA 'Programs\Snipping Lite'),
    [string]$Source = (Join-Path (Split-Path $PSScriptRoot -Parent) 'src/target/release/snippinglite.exe')
)
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Executable not found: $Source" }
$null = New-Item -ItemType Directory -Path $Directory -Force
Copy-Item -LiteralPath $Source -Destination (Join-Path $Directory 'snippinglite.exe') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'uninstall.ps1') -Destination (Join-Path $Directory 'uninstall.ps1') -Force
Write-Host "Installed Snipping Lite to $Directory. Start snippinglite.exe to enable its shortcut."
