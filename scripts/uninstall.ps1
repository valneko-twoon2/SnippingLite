# Removes Snipping Lite for the current user. Settings and history are kept unless
# -RemoveData is given. Also the command behind Settings > Apps.
param(
    [string]$Directory = (Join-Path $env:LOCALAPPDATA 'Programs\Snipping Lite'),
    [switch]$RemoveData,
    [string]$RegistryRoot = 'HKCU:\Software'
)
$ErrorActionPreference = 'Stop'
$Directory = [IO.Path]::GetFullPath($Directory)
$target = Join-Path $Directory 'snippinglite.exe'
# Refuse to touch a folder that is not a Snipping Lite install.
if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Snipping Lite is not installed in $Directory." }

Get-Process -Name snippinglite -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $target } | Stop-Process -Force
Start-Sleep -Milliseconds 300

Remove-Item -LiteralPath "$RegistryRoot\Microsoft\Windows\CurrentVersion\Uninstall\SnippingLite" -Recurse -Force -ErrorAction SilentlyContinue
Remove-ItemProperty -LiteralPath "$RegistryRoot\Microsoft\Windows\CurrentVersion\Run" -Name SnippingLite -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $target
foreach ($name in 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'uninstall.ps1') {
    Remove-Item -LiteralPath (Join-Path $Directory $name) -ErrorAction SilentlyContinue
}
# Only an empty folder is removed, so files the user added stay.
if (!(Get-ChildItem -LiteralPath $Directory -Force)) { Remove-Item -LiteralPath $Directory }
if ($RemoveData) {
    # Settings, the pen choices and the screenshot history. Saved screenshots and recordings are never touched.
    Remove-Item -LiteralPath (Join-Path $env:APPDATA 'Snipping Lite') -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $env:LOCALAPPDATA 'Snipping Lite') -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host 'Snipping Lite, its settings and its history were removed. Saved screenshots and recordings were kept.'
} else {
    Write-Host 'Snipping Lite was removed. Settings and history are kept (use -RemoveData to delete them).'
}
