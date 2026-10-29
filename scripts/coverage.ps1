# Measure line coverage of the application, including the tests that need a desktop.
# These tests create windows, cover the screen, use the clipboard and show dialogs and menus,
# so keep the machine idle while they run.
# Usage: pwsh -File scripts/coverage.ps1 [-Threshold 95] [-Html]
param(
    [double]$Threshold = 95,
    [switch]$Html
)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    # The tests find windows by their title and take over the screen, so a running Snipping Lite
    # (or its history and settings windows) makes them wait for dialogs that never come.
    $running = @(Get-Process -Name snippinglite -ErrorAction SilentlyContinue)
    if ($running.Count -gt 0) {
        throw "Close Snipping Lite before measuring coverage (process $($running.Id -join ', ') is running)."
    }
    # Test helpers and the end-to-end tests are not part of the product, so they are not measured.
    $ignore = 'test_support|[\\/]tests[\\/]|[\\/](tests|flows|leaks)\.rs$'
    $report = if ($Html) { '--html' } else { '--summary-only' }
    # The coverage data is lost when any test fails, so a failed run is tried once more: menus
    # and focus can be disturbed by something else on the desktop. The audio test needs a
    # sound output that records per-process audio; run it by hand on a machine that has one.
    foreach ($attempt in 1..2) {
        cargo llvm-cov --manifest-path src/Cargo.toml --locked --bin snippinglite `
            --ignore-filename-regex $ignore $report --fail-under-lines $Threshold `
            -- --include-ignored --test-threads=1 --skip records_system_audio_and_only_the_chosen_app
        if ($LASTEXITCODE -eq 0) { break }
    }
    exit $LASTEXITCODE
} finally { Pop-Location }
