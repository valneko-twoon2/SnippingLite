$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-file-length.ps1')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    pwsh -NoProfile -File (Join-Path $PSScriptRoot 'test-installer.ps1')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-text-alignment.ps1')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo fmt --manifest-path src/Cargo.toml -- --check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo clippy --config src/.cargo/config.toml --manifest-path src/Cargo.toml --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test --config src/.cargo/config.toml --manifest-path src/Cargo.toml --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }
