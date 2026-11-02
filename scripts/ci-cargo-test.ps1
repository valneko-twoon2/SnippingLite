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
# libtest の JSON 形式は、テストの開始も出す。通常の形式は「ok」が付くまで行が終わらないので、止まった
# テストの名前が画面に出ない。JSON の行を、開始・終了・所要時間の行に直して流す。
function Format-TestEvent([string]$Line) {
    if (-not $Line.StartsWith('{')) { return $Line }
    try { $event = $Line | ConvertFrom-Json } catch { return $Line }
    $time = if ($null -ne $event.exec_time) { ' ({0:N2} s)' -f $event.exec_time } else { '' }
    switch ($event.type) {
        'suite' {
            if ($event.event -eq 'started') { return "テスト $($event.test_count) 件を始める" }
            return "$($event.event): 成功 $($event.passed)、失敗 $($event.failed)、無視 $($event.ignored)、除外 $($event.filtered_out)$time"
        }
        'test' {
            if ($event.event -eq 'started') { return "[開始] $($event.name)" }
            $detail = if ($event.stdout) { "`n$($event.stdout.TrimEnd())" } else { '' }
            return "[$($event.event)] $($event.name)$time$detail"
        }
        default { return $Line }
    }
}

Push-Location $root
try {
    # テストの出力は、通ったテストのものも含め、すべて残す。`--nocapture` で隠さず、JSON 形式で
    # 開始と終了を出す（不安定な機能なので `RUSTC_BOOTSTRAP` で許す）。パニックの経路も全部出す。
    $env:RUSTC_BOOTSTRAP = '1'
    $env:RUST_BACKTRACE = 'full'
    $detail = '--nocapture', '--format', 'json', '-Z', 'unstable-options', '--report-time'
    # 標準エラーの行は ErrorRecord になるので、文字列にしてからログと画面へ流す。
    & cargo test --config src/.cargo/config.toml --manifest-path src/Cargo.toml --locked @TestArguments @detail 2>&1 |
        ForEach-Object { Format-TestEvent "$_" } |
        Tee-Object -FilePath (Join-Path $folder "$Log.log")
    $code = $LASTEXITCODE
    # 失敗したら、ログの末尾を注釈にも出す。成果物をダウンロードしなくても、実行の画面と API で原因が読める。
    if ($code -ne 0) {
        $tail = (Get-Content (Join-Path $folder "$Log.log") -Tail 60) -join '%0A'
        Write-Host "::error title=$Log::$($tail -replace "`r", '')"
    }
    exit $code
} finally { Pop-Location }
