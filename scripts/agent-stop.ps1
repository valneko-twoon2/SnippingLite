$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
# A task is not finished while a source file is over the length limit: exit code 2 keeps the
# agent working and shows it the message.
$output = pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-file-length.ps1') 2>&1
if ($LASTEXITCODE -ne 0) {
    [Console]::Error.WriteLine(($output | Out-String))
    exit 2
}
# src/ 以下の変更は、1 つ直すたびにコミットする。未コミットの変更が残っている間は終われない。
$pending = git status --short -- src 2>&1
if ($LASTEXITCODE -eq 0 -and $pending) {
    [Console]::Error.WriteLine("src/ に未コミットの変更がある。validate.ps1 を実行し、作業対象のファイルだけをステージしてコミットしてから終えること。`n" + ($pending | Out-String))
    exit 2
}
Write-Output 'Review the task status and validation results before stopping.'