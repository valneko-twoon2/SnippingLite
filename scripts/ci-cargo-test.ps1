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

# 出力が 3 分止まったら、止まったテストの様子（窓、ダイアログの文字、デスクトップの画像）を出して
# ダイアログを閉じる。10 分止まったままなら、テストのプロセスを終わらせる。画面のテストはダイアログが
# 開いたまま戻らないことがあり、そのままだと、ジョブの上限まで何も分からず待つことになる。
$watch = [hashtable]::Synchronized(@{ Last = [DateTime]::UtcNow; Done = $false; Test = '' })
$watcher = Start-ThreadJob -ArgumentList $watch, $PSScriptRoot, $Log -ScriptBlock {
    param($watch, $scripts, $log)
    . (Join-Path $scripts 'ci-hang-report.ps1')
    $reported = 0
    while (-not $watch.Done) {
        Start-Sleep -Seconds 10
        $idle = ([DateTime]::UtcNow - $watch.Last).TotalSeconds
        if ($idle -lt 180) { $reported = 0; continue }
        if ($idle -ge 600) {
            [Console]::WriteLine("::error title=$log::出力が $([int]$idle) 秒止まったので、テストのプロセスを終わらせる")
            Get-TestProcess | Stop-Process -Force -ErrorAction SilentlyContinue
            $watch.Last = [DateTime]::UtcNow
            continue
        }
        if ($idle -ge 180 * ($reported + 1)) {
            $reported++
            $report = Write-HangReport -Name "$log-hang$reported" -Close
            [Console]::WriteLine("::warning title=$log::出力が $([int]$idle) 秒止まっている。最後に始めたテスト: $($watch.Test)%0A$report")
        }
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
        ForEach-Object {
            $watch.Last = [DateTime]::UtcNow
            $line = Format-TestEvent "$_"
            if ($line.StartsWith('[開始] ')) { $watch.Test = $line.Substring(5) }
            $line
        } |
        Tee-Object -FilePath (Join-Path $folder "$Log.log")
    $code = $LASTEXITCODE
    # 失敗したら、原因を注釈にも出す。成果物をダウンロードしなくても、実行の画面と API で読める。
    # バックトレースは長くて原因を押し流すので、パニックの文（と続く数行）、失敗したテスト、
    # 最後に始めたテスト、ログの末尾の短い抜粋だけを選ぶ。全文は成果物のログにある。
    if ($code -ne 0) {
        $lines = @(Get-Content (Join-Path $folder "$Log.log"))
        $summary = [System.Collections.Generic.List[string]]::new()
        for ($i = 0; $i -lt $lines.Count; $i++) {
            if ($lines[$i] -match 'panicked at|^\[(FAILED|failed)\]|^failed:') {
                $summary.AddRange([string[]]$lines[$i..[math]::Min($i + 4, $lines.Count - 1)])
            }
        }
        $started = $lines | Where-Object { $_ -like '`[開始`]*' } | Select-Object -Last 1
        if ($started) { $summary.Add("最後に始めたテスト: $started") }
        $tail = @($lines | Where-Object { $_ -notmatch '^\s+(\d+:|at )' } | Select-Object -Last 15)
        $message = (@($summary | Select-Object -First 40) + '---' + $tail) -join '%0A'
        Write-Host "::error title=$Log::$($message -replace "`r", '')"
    }
    exit $code
} finally {
    $watch.Done = $true
    Stop-Job $watcher -ErrorAction SilentlyContinue
    Remove-Job $watcher -Force -ErrorAction SilentlyContinue
    Pop-Location
}
