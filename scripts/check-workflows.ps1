# Lints .github/workflows with actionlint, so a typo is found before a tag is pushed.
# actionlint comes from PATH, or a pinned, checksum-verified copy is downloaded once into
# %LOCALAPPDATA%\snippinglite-tools. Without a network the check is skipped with a warning,
# unless -Strict is given (CI).
param([switch]$Strict)
$ErrorActionPreference = 'Stop'
$version = '1.7.7'
$sha256 = '7f12f1801bca3d480d67aaf7774f4c2a6359a3ca8eebe382c95c10c9704aa731'
$root = Split-Path $PSScriptRoot -Parent
$workflows = Join-Path $root '.github/workflows'
if (!(Test-Path -LiteralPath $workflows)) { Write-Output 'No workflows to check.'; exit 0 }

function Find-Actionlint {
    $installed = Get-Command actionlint -ErrorAction SilentlyContinue
    if ($installed) { return $installed.Source }
    $folder = Join-Path $env:LOCALAPPDATA "snippinglite-tools\actionlint-$version"
    $exe = Join-Path $folder 'actionlint.exe'
    if (Test-Path -LiteralPath $exe) { return $exe }
    $zip = Join-Path ([IO.Path]::GetTempPath()) "actionlint-$version.zip"
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -UseBasicParsing -TimeoutSec 60 -OutFile $zip `
        -Uri "https://github.com/rhysd/actionlint/releases/download/v$version/actionlint_${version}_windows_amd64.zip"
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLower() -ne $sha256) {
        Remove-Item -LiteralPath $zip -Force
        throw 'The downloaded actionlint does not match the pinned checksum.'
    }
    $null = New-Item -ItemType Directory -Path $folder -Force
    Expand-Archive -LiteralPath $zip -DestinationPath $folder -Force
    Remove-Item -LiteralPath $zip -Force
    $exe
}

try {
    $actionlint = Find-Actionlint
} catch {
    if ($Strict) { throw }
    Write-Warning "Workflow check skipped: $($_.Exception.Message)"
    exit 0
}
Push-Location $root
try {
    & $actionlint -color
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Output 'Workflow check passed.'
} finally { Pop-Location }
