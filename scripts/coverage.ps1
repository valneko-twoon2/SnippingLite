# Measure line coverage of the application, including the tests that need a desktop.
# These tests create windows and use the clipboard, so keep the machine idle while they run.
# Usage: pwsh -File scripts/coverage.ps1 [-Threshold 95] [-Html]
param(
    [double]$Threshold = 95,
    [switch]$Html
)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    # Test helpers and the end-to-end tests are not part of the product, so they are not measured.
    $ignore = 'tests[\\/]e2e|test_support|[\\/]tests\.rs$'
    $report = if ($Html) { '--html' } else { '--summary-only' }
    cargo llvm-cov --manifest-path src/Cargo.toml --locked --bin snippinglite `
        --ignore-filename-regex $ignore $report --fail-under-lines $Threshold `
        -- --include-ignored --test-threads=1
    exit $LASTEXITCODE
} finally { Pop-Location }
