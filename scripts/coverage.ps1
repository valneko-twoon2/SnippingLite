# Measure line coverage of the application, including the tests that need a desktop.
# These tests create windows, cover the screen, use the clipboard and show dialogs and menus,
# so keep the machine idle while they run.
# Usage: pwsh -File scripts/coverage.ps1 [-Threshold 95] [-Html] [-StallSeconds 120]
param(
    [double]$Threshold = 95,
    [switch]$Html,
    # A run whose output stops for this long is killed and tried again.
    [int]$StallSeconds = 120
)
$ErrorActionPreference = 'Stop'

# Runs cargo with its output copied to the console, and kills it when the output stalls.
# Returns the exit code, or -1 for a stalled run.
function Invoke-Watched([string]$Arguments) {
    $out = [IO.Path]::GetTempFileName()
    $err = [IO.Path]::GetTempFileName()
    $process = Start-Process -FilePath cargo -ArgumentList $Arguments -NoNewWindow -PassThru `
        -RedirectStandardOutput $out -RedirectStandardError $err
    $positions = @{ $out = 0L; $err = 0L }
    $last = Get-Date
    $tail = ''
    try {
        while ($true) {
            $done = $process.WaitForExit(1000)
            foreach ($file in $out, $err) {
                $stream = [IO.File]::Open($file, 'Open', 'Read', 'ReadWrite')
                try {
                    if ($stream.Length -gt $positions[$file]) {
                        $null = $stream.Seek($positions[$file], 'Begin')
                        $reader = [IO.StreamReader]::new($stream)
                        $text = $reader.ReadToEnd()
                        $positions[$file] = $stream.Length
                        [Console]::Out.Write($text)
                        $tail = ($text -split "`n" | Where-Object { $_.Trim() } | Select-Object -Last 1)
                        $last = Get-Date
                    }
                } finally { $stream.Dispose() }
            }
            if ($done) { return $process.ExitCode }
            if (((Get-Date) - $last).TotalSeconds -ge $StallSeconds) {
                Write-Warning "No output for $StallSeconds seconds; stopping the run. Last line: $tail"
                taskkill /PID $process.Id /T /F | Out-Null
                return -1
            }
        }
    } finally { Remove-Item -LiteralPath $out, $err -Force -ErrorAction SilentlyContinue }
}

Push-Location (Join-Path $PSScriptRoot '..')
try {
    # The tests find windows by their title and take over the screen, so a running Snipping Lite
    # (or its history and settings windows) makes them wait for dialogs that never come. It is
    # stopped here; start it again afterwards.
    $running = @(Get-Process -Name snippinglite -ErrorAction SilentlyContinue)
    if ($running.Count -gt 0) {
        Write-Output "Stopping Snipping Lite (process $($running.Id -join ', ')) for the tests."
        $running | Stop-Process -Force
        $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
    }
    # Test helpers and the end-to-end tests are not part of the product, so they are not measured.
    # モンキーテストと、テストに状態を見せる窓口（probe.rs）もテストだけのコード。
    $ignore = 'test_support|[\\/]tests[\\/]|[\\/]monkey([\\/]|\.rs$)|[\\/](tests|flows|leaks|probe)\.rs$'
    $report = if ($Html) { '--html' } else { '--summary-only' }
    # The coverage data is lost when any test fails, so a failed or stalled run is tried once
    # more: menus and focus can be disturbed by something else on the desktop. The audio test
    # needs a sound output that records per-process audio; run it by hand on a machine that has one.
    $arguments = "llvm-cov --manifest-path src/Cargo.toml --locked --bin snippinglite " +
        "--ignore-filename-regex `"$ignore`" $report --fail-under-lines $Threshold " +
        '-- --include-ignored --test-threads=1 --skip records_system_audio_and_only_the_chosen_app'
    $code = 1
    foreach ($attempt in 1..2) {
        $code = Invoke-Watched $arguments
        if ($code -eq 0) { break }
    }
    exit $code
} finally { Pop-Location }
