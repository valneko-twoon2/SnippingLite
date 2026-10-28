param([string]$Version)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$toml = Join-Path $root 'src/Cargo.toml'
if (-not $Version) { $Version = Read-Host 'New version (X.Y.Z)' }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Version must look like 1.2.3.' }
$text = [IO.File]::ReadAllText($toml)
$text = [regex]::new('(?m)^version = "[^"]+"').Replace($text, "version = `"$Version`"", 1)
[IO.File]::WriteAllText($toml, $text, [Text.UTF8Encoding]::new($false))
Push-Location $root
try {
    cargo check --config src/.cargo/config.toml --manifest-path src/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Could not update Cargo.lock.' }
} finally { Pop-Location }
Write-Host 'Updated Cargo.toml and Cargo.lock. Review the changes.'
