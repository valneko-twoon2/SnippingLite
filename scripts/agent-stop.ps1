$ErrorActionPreference = 'Stop'
# A task is not finished while a source file is over the length limit: exit code 2 keeps the
# agent working and shows it the message.
$output = pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-file-length.ps1') 2>&1
if ($LASTEXITCODE -ne 0) {
    [Console]::Error.WriteLine(($output | Out-String))
    exit 2
}
Write-Output 'Review the task status and validation results before stopping.'