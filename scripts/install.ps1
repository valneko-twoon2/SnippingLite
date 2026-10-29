# Install Snipping Lite for the current user.
#   irm https://github.com/OWNER/REPO/releases/latest/download/install.ps1 | iex
# downloads the release executable and checks its SHA-256. With -Source it installs a locally
# built executable instead, e.g. pwsh -File scripts/install.ps1 -Source src/target/release/snippinglite.exe
param(
    [string]$Directory = (Join-Path $env:LOCALAPPDATA 'Programs\Snipping Lite'),
    [string]$Source,
    # Replace OWNER/REPO once the repository exists.
    [string]$BaseUrl = 'https://github.com/OWNER/REPO/releases/latest/download'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$exe = Join-Path $Directory 'snippinglite.exe'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $scratch -Force
try {
    if ($Source) {
        if (!(Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Executable not found: $Source" }
        Copy-Item -LiteralPath $Source -Destination (Join-Path $scratch 'snippinglite.exe')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'uninstall.ps1') -Destination $scratch
    } else {
        if ($BaseUrl -like '*OWNER/REPO*') { throw 'The release location is not set yet. Pass -BaseUrl or use -Source.' }
        foreach ($name in 'snippinglite.exe', 'snippinglite.exe.sha256', 'uninstall.ps1', 'LICENSE', 'THIRD-PARTY-NOTICES.txt') {
            Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$name" -OutFile (Join-Path $scratch $name)
        }
        $expected = (Get-Content -LiteralPath (Join-Path $scratch 'snippinglite.exe.sha256') -Raw).Trim().ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $scratch 'snippinglite.exe')).Hash.ToLower()
        if ($actual -ne $expected) { throw "Checksum mismatch for snippinglite.exe (expected $expected, got $actual)." }
    }

    # A running copy keeps its file locked, so stop it before replacing it.
    Get-Process -Name snippinglite -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $exe } | Stop-Process -Force
    $null = New-Item -ItemType Directory -Path $Directory -Force
    Get-ChildItem -LiteralPath $scratch -File | Where-Object { $_.Name -ne 'snippinglite.exe.sha256' } |
        Copy-Item -Destination $Directory -Force
} finally { Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue }
Write-Host "Installed Snipping Lite to $Directory. Start snippinglite.exe to enable its shortcut."
Write-Host "Uninstall with: & '$(Join-Path $Directory 'uninstall.ps1')'"
