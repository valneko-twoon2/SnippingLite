# CI で cargo test を走らせ、出力を ci-logs/<名前>.log にも残す。失敗したジョブのログを成果物として
# 持ち帰るためのもので、ログがあっても cargo test の終了コードはそのまま返す。
# 使い方: pwsh -File scripts/ci-cargo-test.ps1 -Log screen -TestArguments '--bin','snippinglite','--','--ignored'
# `--` は PowerShell の引数の区切りと取り違えられるので、cargo test への引数は文字列の配列で渡す。
param(
    [Parameter(Mandatory)][string]$Log,
    [string[]]$TestArguments = @()
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$folder = Join-Path $root 'ci-logs'
$null = New-Item -ItemType Directory -Path $folder -Force
Push-Location $root
try {
    # 標準エラーの行は ErrorRecord になるので、文字列にしてからログと画面へ流す。
    & cargo test --config src/.cargo/config.toml --manifest-path src/Cargo.toml --locked @TestArguments 2>&1 |
        ForEach-Object { "$_" } |
        Tee-Object -FilePath (Join-Path $folder "$Log.log")
    $code = $LASTEXITCODE
    # 失敗したら、ログの末尾を注釈にも出す。成果物をダウンロードしなくても、実行の画面と API で原因が読める。
    if ($code -ne 0) {
        $tail = (Get-Content (Join-Path $folder "$Log.log") -Tail 60) -join '%0A'
        Write-Host "::error title=$Log::$($tail -replace "`r", '')"
    }
    exit $code
} finally { Pop-Location }
