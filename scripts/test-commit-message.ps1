$ErrorActionPreference = 'Stop'
# コミットメッセージの検査が、拒むべきものを本当に拒む（終了コードが 0 以外になる）かを確かめる。
# 以前は Write-Error だけで終了コードが 0 のままになり、帰属表記のあるコミットが通っていた。
$check = Join-Path $PSScriptRoot 'check-commit-message.ps1'
$cases = @(
    @{ Name = 'conventional'; Message = "feat: 機能を追加`n`n本文"; Ok = $true },
    @{ Name = 'scope and breaking change'; Message = 'fix(ui)!: 修正'; Ok = $true },
    @{ Name = 'trailer named in the body'; Message = "docs: 規約`n`nCo-Authored-By は付けない"; Ok = $true },
    @{ Name = 'co-author trailer'; Message = "feat: 機能`n`nCo-Authored-By: A <a@example.com>"; Ok = $false },
    @{ Name = 'lowercase trailer'; Message = "feat: 機能`n`nco-authored-by: A <a@example.com>"; Ok = $false },
    @{ Name = 'indented trailer'; Message = "feat: 機能`n`n  Co-Authored-By : A"; Ok = $false },
    @{ Name = 'CRLF message'; Message = "feat: 機能`r`n`r`nCo-Authored-By: A`r`n"; Ok = $false },
    @{ Name = 'no type'; Message = '機能を追加'; Ok = $false }
)
$file = Join-Path ([IO.Path]::GetTempPath()) "snipping-lite-commit-message-$PID.txt"
$failures = @()
try {
    foreach ($case in $cases) {
        [IO.File]::WriteAllText($file, $case.Message, [Text.UTF8Encoding]::new($false))
        pwsh -NoProfile -File $check -MessageFile $file 2>$null
        if (($LASTEXITCODE -eq 0) -ne $case.Ok) {
            $failures += "$($case.Name): expected $(if ($case.Ok) { 'accepted' } else { 'rejected' }), exit code $LASTEXITCODE"
        }
    }
} finally { Remove-Item -LiteralPath $file -Force -ErrorAction SilentlyContinue }
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { [Console]::Error.WriteLine($_) }
    exit 1
}
Write-Output "Commit message check tests passed ($($cases.Count))."
