# Tests install.ps1 and uninstall.ps1 against a local web server that plays the GitHub release.
# Nothing is installed outside a temporary folder, and the registry entries go under a throwaway key. Usage: pwsh -File scripts/test-installer.ps1
$ErrorActionPreference = 'Stop'
$install = Join-Path $PSScriptRoot 'install.ps1'
$uninstall = Join-Path $PSScriptRoot 'uninstall.ps1'
$work = Join-Path ([IO.Path]::GetTempPath()) ('snippinglite-installer-' + [guid]::NewGuid().ToString('N'))
$release = Join-Path $work 'release'
$null = New-Item -ItemType Directory -Path $release -Force
$failures = [Collections.Generic.List[string]]::new()
$count = 0
$registry = 'HKCU:\Software\SnippingLiteInstallerTest-' + [guid]::NewGuid().ToString('N')
$uninstallKey = "$registry\Microsoft\Windows\CurrentVersion\Uninstall\SnippingLite"
$runKey = "$registry\Microsoft\Windows\CurrentVersion\Run"

function Test([string]$Name, [scriptblock]$Body) {
    $script:count++
    try { & $Body } catch { $script:failures.Add("${Name}: $($_.Exception.Message)") }
}
function Assert([bool]$Condition, [string]$Message) { if (!$Condition) { throw $Message } }
function Run-Script([string]$Script, [string[]]$Arguments) {
    $extra = @('-RegistryRoot', $registry)
    if ((Split-Path $Script -Leaf) -eq 'install.ps1') { $extra += '-NoStart' }
    $output = pwsh -NoProfile -File $Script @Arguments @extra 2>&1 | Out-String
    [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
}
function Publish-Release([string]$Content, [string]$Hash) {
    Set-Content -LiteralPath (Join-Path $release 'snippinglite.exe') -Value $Content -NoNewline
    if (!$Hash) { $Hash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $release 'snippinglite.exe')).Hash }
    Set-Content -LiteralPath (Join-Path $release 'snippinglite.exe.sha256') -Value $Hash -NoNewline
    Copy-Item -LiteralPath $uninstall -Destination $release -Force
}
function Content-Of([string]$Directory) {
    (Get-Content -LiteralPath (Join-Path $Directory 'snippinglite.exe') -Raw)
}

# A server that hands out the files of $release until the job is removed.
$probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$probe.Start()
$port = $probe.LocalEndpoint.Port
$probe.Stop()
$server = Start-Job -ArgumentList $port, $release -ScriptBlock {
    param($port, $root)
    $listener = [Net.HttpListener]::new()
    $listener.Prefixes.Add("http://127.0.0.1:$port/")
    $listener.Start()
    while ($true) {
        $context = $listener.GetContext()
        $file = Join-Path $root ($context.Request.Url.AbsolutePath.TrimStart('/'))
        if (Test-Path -LiteralPath $file -PathType Leaf) {
            $bytes = [IO.File]::ReadAllBytes($file)
            $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } else {
            $context.Response.StatusCode = 404
        }
        $context.Response.Close()
    }
}
$base = "http://127.0.0.1:$port"
for ($i = 0; $i -lt 50; $i++) {
    try { $null = Invoke-WebRequest -UseBasicParsing -Uri "$base/ping" -ErrorAction Stop } catch { if ($_.Exception.Response) { break } }
    Start-Sleep -Milliseconds 100
}

try {
    Publish-Release 'version one' $null

    Test 'downloads, verifies and installs every file' {
        $dir = Join-Path $work 'a'
        $r = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Assert ($r.Code -eq 0) "exit $($r.Code): $($r.Output)"
        foreach ($name in 'snippinglite.exe', 'uninstall.ps1') {
            Assert (Test-Path -LiteralPath (Join-Path $dir $name)) "missing $name"
        }
        Assert (!(Test-Path -LiteralPath (Join-Path $dir 'snippinglite.exe.sha256'))) 'the checksum file must not be installed'
        Assert ((Content-Of $dir) -eq 'version one') 'wrong content'
    }

    Test 'a checksum file in the sha256sum layout is accepted' {
        $dir = Join-Path $work 'sum'
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $release 'snippinglite.exe')).Hash
        Set-Content -LiteralPath (Join-Path $release 'snippinglite.exe.sha256') -Value "$hash  snippinglite.exe"
        $r = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Assert ($r.Code -eq 0) $r.Output
        Publish-Release 'version one' $null
    }

    Test 'installing again replaces the executable' {
        $dir = Join-Path $work 'b'
        $null = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Publish-Release 'version two' $null
        $r = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Assert ($r.Code -eq 0) $r.Output
        Assert ((Content-Of $dir) -eq 'version two') 'not replaced'
        Publish-Release 'version one' $null
    }

    Test 'a checksum mismatch installs nothing' {
        $dir = Join-Path $work 'c'
        Publish-Release 'tampered' ('0' * 64)
        $r = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Assert ($r.Code -ne 0) 'the install must fail'
        Assert ($r.Output -match 'Checksum mismatch') "unexpected message: $($r.Output)"
        Assert (!(Test-Path -LiteralPath $dir)) 'nothing may be created'
        Publish-Release 'version one' $null
    }

    Test 'a missing release file installs nothing' {
        $dir = Join-Path $work 'd'
        Remove-Item -LiteralPath (Join-Path $release 'uninstall.ps1')
        $r = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Assert ($r.Code -ne 0) 'the install must fail'
        Assert (!(Test-Path -LiteralPath $dir)) 'nothing may be created'
        Publish-Release 'version one' $null
    }

    Test '-Source installs a local executable' {
        $dir = Join-Path $work 'f'
        $r = Run-Script $install @('-Directory', $dir, '-Source', (Join-Path $release 'snippinglite.exe'))
        Assert ($r.Code -eq 0) $r.Output
        Assert (Test-Path -LiteralPath (Join-Path $dir 'uninstall.ps1')) 'uninstall.ps1 missing'
    }

    Test '-Source with a missing file fails' {
        $r = Run-Script $install @('-Directory', (Join-Path $work 'g'), '-Source', (Join-Path $work 'nothing.exe'))
        Assert ($r.Code -ne 0 -and $r.Output -match 'not found') "unexpected: $($r.Output)"
    }

    Test 'uninstalling removes the files and an empty folder' {
        $dir = Join-Path $work 'h'
        $null = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        $r = Run-Script (Join-Path $dir 'uninstall.ps1') @('-Directory', $dir)
        Assert ($r.Code -eq 0) $r.Output
        Assert (!(Test-Path -LiteralPath $dir)) 'the empty folder must go'
    }

    Test 'uninstalling keeps files the user added' {
        $dir = Join-Path $work 'i'
        $null = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Set-Content -LiteralPath (Join-Path $dir 'mine.txt') -Value 'keep'
        $r = Run-Script $uninstall @('-Directory', $dir)
        Assert ($r.Code -eq 0) $r.Output
        Assert (Test-Path -LiteralPath (Join-Path $dir 'mine.txt')) 'the user file was removed'
        Assert (!(Test-Path -LiteralPath (Join-Path $dir 'snippinglite.exe'))) 'the executable stayed'
    }

    Test 'the app is listed in Settings > Apps and starts at sign-in' {
        $dir = Join-Path $work 'reg'
        $null = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        $entry = Get-ItemProperty -LiteralPath $uninstallKey
        Assert ($entry.DisplayName -eq 'Snipping Lite') 'no display name'
        Assert ($entry.InstallLocation -eq $dir) 'wrong install location'
        Assert ($entry.UninstallString -like "*$dir*uninstall.ps1*") "uninstall command: $($entry.UninstallString)"
        Assert ($entry.NoModify -eq 1 -and $entry.EstimatedSize -ge 0) 'missing flags'
        $run = (Get-ItemProperty -LiteralPath $runKey).SnippingLite
        Assert ($run -eq "`"$(Join-Path $dir 'snippinglite.exe')`"") "run value: $run"
        $r = Run-Script (Join-Path $dir 'uninstall.ps1') @('-Directory', $dir)
        Assert ($r.Code -eq 0) $r.Output
        Assert (!(Test-Path -LiteralPath $uninstallKey)) 'the Apps entry stayed'
        Assert (!(Get-ItemProperty -LiteralPath $runKey).SnippingLite) 'the sign-in start stayed'
    }

    Test 'uninstalling keeps settings and history unless -RemoveData is given' {
        $dir = Join-Path $work 'data'
        $roaming = Join-Path $work 'roaming'; $local = Join-Path $work 'local'
        $null = New-Item -ItemType Directory -Path "$roaming\Snipping Lite", "$local\Snipping Lite\history" -Force
        Set-Content -LiteralPath "$roaming\Snipping Lite\settings.ini" -Value 'save=1'
        $saved = @($env:APPDATA, $env:LOCALAPPDATA)
        $env:APPDATA = $roaming; $env:LOCALAPPDATA = $local
        try {
            $null = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
            $null = Run-Script $uninstall @('-Directory', $dir)
            Assert (Test-Path -LiteralPath "$roaming\Snipping Lite\settings.ini") 'settings were removed without -RemoveData'
            $null = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
            $r = Run-Script $uninstall @('-Directory', $dir, '-RemoveData')
            Assert ($r.Code -eq 0) $r.Output
            Assert (!(Test-Path -LiteralPath "$roaming\Snipping Lite")) 'settings stayed with -RemoveData'
            Assert (!(Test-Path -LiteralPath "$local\Snipping Lite")) 'history stayed with -RemoveData'
        } finally { $env:APPDATA = $saved[0]; $env:LOCALAPPDATA = $saved[1] }
    }

    Test 'uninstalling something that is not installed fails' {
        $r = Run-Script $uninstall @('-Directory', (Join-Path $work 'nothing-here'))
        Assert ($r.Code -ne 0 -and $r.Output -match 'not installed') "unexpected: $($r.Output)"
    }
} finally {
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $registry -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ -ErrorAction Continue }
    exit 1
}
Write-Output "Installer tests passed ($count)."
