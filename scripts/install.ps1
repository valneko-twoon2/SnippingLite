# Installs or updates Snipping Lite for the current user. No administrator rights needed.
#   irm https://github.com/<owner>/<repo>/releases/latest/download/install.ps1 | iex
# Parameters (when run as a file): -Repo owner/name, -Directory target folder, and
# -Source, a local snippinglite.exe to install instead of downloading a release.
# -BaseUrl, -RegistryRoot and -NoStart exist for scripts/test-installer.ps1.
param(
    [string]$Repo = 'valneko-twoon2/SnippingLite',
    [string]$Directory = (Join-Path $env:LOCALAPPDATA 'Programs\Snipping Lite'),
    [string]$Source,
    [string]$BaseUrl = "https://github.com/$Repo/releases/latest/download",
    [string]$RegistryRoot = 'HKCU:\Software',
    [switch]$NoStart
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.ServicePointManager]::SecurityProtocol

$exe = Join-Path $Directory 'snippinglite.exe'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('snippinglite-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $scratch -Force
try {
    if ($Source) {
        if (!(Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Executable not found: $Source" }
        Copy-Item -LiteralPath $Source -Destination (Join-Path $scratch 'snippinglite.exe')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'uninstall.ps1') -Destination $scratch
    } else {
        foreach ($name in 'snippinglite.exe', 'snippinglite.exe.sha256', 'uninstall.ps1') {
            Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$name" -OutFile (Join-Path $scratch $name)
        }
        # Either the bare hash or the "hash  file name" form of sha256sum.
        $expected = ((Get-Content -LiteralPath (Join-Path $scratch 'snippinglite.exe.sha256') -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $scratch 'snippinglite.exe')).Hash.ToLower()
        if ($actual -ne $expected) { throw "Checksum mismatch for snippinglite.exe (expected $expected, got $actual); nothing was installed." }
    }

    # A running copy keeps its file locked, so stop it before replacing it.
    Get-Process -Name snippinglite -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $exe } | Stop-Process -Force
    Start-Sleep -Milliseconds 300
    $null = New-Item -ItemType Directory -Path $Directory -Force
    Get-ChildItem -LiteralPath $scratch -File | Where-Object { $_.Name -ne 'snippinglite.exe.sha256' } |
        Copy-Item -Destination $Directory -Force

    # List the app in Settings > Apps; this only touches the current user's hive.
    $key = "$RegistryRoot\Microsoft\Windows\CurrentVersion\Uninstall\SnippingLite"
    $null = New-Item -Path $key -Force
    # Settings > Apps can hide entries without a version, so ask the executable.
    $version = ''
    try { $version = (& $exe --version | Out-String).Trim() } catch { }
    $remover = Join-Path $Directory 'uninstall.ps1'
    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$remover`" -Directory `"$Directory`""
    $values = @{
        DisplayName = 'Snipping Lite'; DisplayVersion = $version; Publisher = 'Snipping Lite'
        InstallLocation = $Directory; DisplayIcon = $exe; UninstallString = $command
        InstallDate = (Get-Date -Format 'yyyyMMdd')
    }
    foreach ($name in $values.Keys) {
        if ($values[$name]) { Set-ItemProperty -Path $key -Name $name -Value $values[$name] }
    }
    Set-ItemProperty -Path $key -Name NoModify -Value 1 -Type DWord
    Set-ItemProperty -Path $key -Name NoRepair -Value 1 -Type DWord
    Set-ItemProperty -Path $key -Name EstimatedSize -Value ([int][Math]::Ceiling((Get-Item -LiteralPath $exe).Length / 1KB)) -Type DWord
    # Start at sign-in, so the shortcut is always available.
    $run = "$RegistryRoot\Microsoft\Windows\CurrentVersion\Run"
    $null = New-Item -Path $run -Force
    Set-ItemProperty -Path $run -Name SnippingLite -Value "`"$exe`""
    if (!$NoStart) { Start-Process -FilePath $exe }
} finally { Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue }
Write-Host "Snipping Lite installed to $exe"
Write-Host "Uninstall from Settings > Apps, or run: & '$(Join-Path $Directory 'uninstall.ps1')'"
