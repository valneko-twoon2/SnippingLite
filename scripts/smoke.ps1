# Starts a built executable the way a user would and checks that it comes up, stays up and
# exits cleanly through its tray command. The app runs against temporary settings and history
# folders, so the real ones are untouched. Close Snipping Lite first: both would want Alt+Shift+S.
# Usage: pwsh -File scripts/smoke.ps1 [-Exe src/target/release/snippinglite.exe]
param([string]$Exe = (Join-Path $PSScriptRoot '..\src\target\release\snippinglite.exe'))
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $Exe -PathType Leaf)) { throw "Executable not found: $Exe" }
Add-Type -Namespace Smoke -Name Native -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern System.IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool PostMessageW(System.IntPtr hwnd, uint msg, System.IntPtr wp, System.IntPtr lp);
'@

$WM_COMMAND = 0x0111
$TRAY_EXIT = 202
$work = Join-Path ([IO.Path]::GetTempPath()) ('snippinglite-smoke-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $work -Force
$info = [Diagnostics.ProcessStartInfo]::new((Resolve-Path -LiteralPath $Exe).Path)
$info.UseShellExecute = $false
foreach ($pair in 'APPDATA=roaming', 'LOCALAPPDATA=local', 'USERPROFILE=profile') {
    $name, $folder = $pair -split '='
    $info.Environment[$name] = Join-Path $work $folder
}
# The installer lists this version in Settings > Apps, so it must match the crate version.
$crate = [regex]::Match([IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\src\Cargo.toml')), '(?m)^version = "([^"]+)"').Groups[1].Value
$reported = (& $Exe --version | Out-String).Trim()
if ($reported -ne $crate) { throw "The executable reports version '$reported', but the crate is $crate." }

$process = $null
try {
    if ([Smoke.Native]::FindWindowW('SnippingLiteMain', [NullString]::Value) -ne [IntPtr]::Zero) {
        throw 'Snipping Lite is already running; close it before the smoke test.'
    }
    $process = [Diagnostics.Process]::Start($info)

    $window = [IntPtr]::Zero
    for ($i = 0; $i -lt 100 -and $window -eq [IntPtr]::Zero -and !$process.HasExited; $i++) {
        Start-Sleep -Milliseconds 100
        $window = [Smoke.Native]::FindWindowW('SnippingLiteMain', [NullString]::Value)
    }
    if ($process.HasExited) { throw "The app exited at start with code $($process.ExitCode)." }
    if ($window -eq [IntPtr]::Zero) { throw 'The main window did not appear within 10 seconds.' }

    Start-Sleep -Seconds 2
    if ($process.HasExited) { throw "The app exited by itself with code $($process.ExitCode)." }

    if (![Smoke.Native]::PostMessageW($window, $WM_COMMAND, [IntPtr]$TRAY_EXIT, [IntPtr]::Zero)) {
        throw 'Could not send the exit command.'
    }
    if (!$process.WaitForExit(10000)) { throw 'The app did not exit after the tray Exit command.' }
    if ($process.ExitCode -ne 0) { throw "The app exited with code $($process.ExitCode)." }
    Write-Output 'Smoke test passed.'
} finally {
    if ($process -and !$process.HasExited) { $process.Kill() }
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
