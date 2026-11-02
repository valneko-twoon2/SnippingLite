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
    # バックトレースは長くて原因を押し流すので、選んで出す。全文は成果物のログにある。
    # 注釈は 1 つが約 4 KB までで、超えた分は切られるので、話題ごとに分け、長ければ 4 KB ごとに割る。
    if ($code -ne 0) {
        $lines = @(Get-Content (Join-Path $folder "$Log.log"))
        # 窓の撮影が拒まれた理由（`window capture:` の行）は、重複を除いて先頭から数行。
        $capture = @($lines | Where-Object { $_ -like 'window capture:*' } | Select-Object -Unique -First 8)
        # 失敗したテストは、名前（と所要時間）をすべて並べる。
        $failed = @($lines | Where-Object { $_ -match '^\[(FAILED|failed)\] ' } | ForEach-Object { $_ -replace '^\[[A-Za-z]+\] ', '' })
        # パニックの文は、場所と最初の 1 行だけ。
        $panics = [System.Collections.Generic.List[string]]::new()
        for ($i = 0; $i -lt $lines.Count - 1; $i++) {
            if ($lines[$i] -match 'panicked at') {
                $panics.Add(($lines[$i] -replace '^thread ', ''))
                $panics.Add("  $($lines[$i + 1])")
            }
        }
        $started = $lines | Where-Object { $_ -like '`[開始`]*' } | Select-Object -Last 1
        $tail = @($lines | Where-Object { $_ -notmatch '^\s+(\d+:|at )' } | Select-Object -Last 12)
        $groups = [ordered]@{
            "$Log 窓の撮影" = @($capture + $(if ($started) { "最後に始めたテスト: $started" }))
            "$Log 失敗したテスト $($failed.Count) 件" = $failed
            "$Log パニック" = $panics
            "$Log 末尾" = $tail
        }
        $sent = 0
        foreach ($title in $groups.Keys) {
            $chunk = [System.Collections.Generic.List[string]]::new()
            $size = 0
            foreach ($line in @($groups[$title]) + $null) {
                $text = if ($null -ne $line) { ($line -replace "`r", '') } else { $null }
                if ($chunk.Count -gt 0 -and ($null -eq $text -or $size + $text.Length + 1 -gt 3600)) {
                    if ($sent -lt 10) { Write-Host "::error title=$title::$($chunk -join '%0A')" }
                    $sent++
                    $chunk.Clear(); $size = 0
                }
                if ($null -ne $text) { $chunk.Add($text.Substring(0, [math]::Min($text.Length, 600))); $size += [math]::Min($text.Length, 600) + 1 }
            }
        }
    }
    exit $code
} finally {
    $watch.Done = $true
    Stop-Job $watcher -ErrorAction SilentlyContinue
    Remove-Job $watcher -Force -ErrorAction SilentlyContinue
    Pop-Location
}
