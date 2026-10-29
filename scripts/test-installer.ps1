# Tests install.ps1 and uninstall.ps1 against a local web server that plays the GitHub release.
# Nothing is installed outside a temporary folder. Usage: pwsh -File scripts/test-installer.ps1
$ErrorActionPreference = 'Stop'
$install = Join-Path $PSScriptRoot 'install.ps1'
$uninstall = Join-Path $PSScriptRoot 'uninstall.ps1'
$work = Join-Path ([IO.Path]::GetTempPath()) ('snippinglite-installer-' + [guid]::NewGuid().ToString('N'))
$release = Join-Path $work 'release'
$null = New-Item -ItemType Directory -Path $release -Force
$failures = [Collections.Generic.List[string]]::new()
$count = 0

function Test([string]$Name, [scriptblock]$Body) {
    $script:count++
    try { & $Body } catch { $script:failures.Add("${Name}: $($_.Exception.Message)") }
}
function Assert([bool]$Condition, [string]$Message) { if (!$Condition) { throw $Message } }
function Run-Script([string]$Script, [string[]]$Arguments) {
    $output = pwsh -NoProfile -File $Script @Arguments 2>&1 | Out-String
    [pscustomobject]@{ Code = $LASTEXITCODE; Output = $output }
}
function Publish-Release([string]$Content, [string]$Hash) {
    Set-Content -LiteralPath (Join-Path $release 'snippinglite.exe') -Value $Content -NoNewline
    if (!$Hash) { $Hash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $release 'snippinglite.exe')).Hash }
    Set-Content -LiteralPath (Join-Path $release 'snippinglite.exe.sha256') -Value $Hash -NoNewline
    Copy-Item -LiteralPath $uninstall -Destination $release -Force
    Set-Content -LiteralPath (Join-Path $release 'LICENSE') -Value 'license'
    Set-Content -LiteralPath (Join-Path $release 'THIRD-PARTY-NOTICES.txt') -Value 'notices'
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
        foreach ($name in 'snippinglite.exe', 'uninstall.ps1', 'LICENSE', 'THIRD-PARTY-NOTICES.txt') {
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
        Remove-Item -LiteralPath (Join-Path $release 'LICENSE')
        $r = Run-Script $install @('-Directory', $dir, '-BaseUrl', $base)
        Assert ($r.Code -ne 0) 'the install must fail'
        Assert (!(Test-Path -LiteralPath $dir)) 'nothing may be created'
        Publish-Release 'version one' $null
    }

    Test 'the placeholder repository is refused' {
        $r = Run-Script $install @('-Directory', (Join-Path $work 'e'))
        Assert ($r.Code -ne 0 -and $r.Output -match 'not set yet') "unexpected: $($r.Output)"
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

    Test 'uninstalling something that is not installed fails' {
        $r = Run-Script $uninstall @('-Directory', (Join-Path $work 'nothing-here'))
        Assert ($r.Code -ne 0 -and $r.Output -match 'not installed') "unexpected: $($r.Output)"
    }
} finally {
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ -ErrorAction Continue }
    exit 1
}
Write-Output "Installer tests passed ($count)."
