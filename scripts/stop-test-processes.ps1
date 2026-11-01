param([switch]$Hook)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
# テストが終わったあとに残った、テスト用の exe を止める。画面を覆うテストが止まったまま残ると、
# 暗いオーバーレイが画面に残って操作できなくなる。対象は `cargo test` が作る
# `src/target/*/deps/*.exe` と、E2E が起動する `%TEMP%\snippinglite-e2e-exe\snippinglite.exe`
# だけで、普段使いの snippinglite.exe や explorer.exe には触れない。
# -Hook のときは Claude Code のフックとして、標準入力のツール呼び出しを読む。テストを走らせる
# コマンドの終わりだけで止め、バックグラウンドで走らせている間は止めない（終了時の Stop フックが止める）。
$root = Split-Path $PSScriptRoot -Parent
if ($Hook) {
    $call = [Console]::In.ReadToEnd() | ConvertFrom-Json -ErrorAction SilentlyContinue
    $command = [string]$call.tool_input.command
    if ($call.tool_input.run_in_background) { exit 0 }
    if ($command -notmatch 'cargo\s+(test|nextest|run)|coverage\.ps1|validate\.ps1|smoke\.ps1|git\s+commit') { exit 0 }
}
$targets = @(
    (Join-Path $root 'src\target') + '\*\deps\*.exe'
    (Join-Path ([IO.Path]::GetTempPath()) 'snippinglite-e2e-exe') + '\*.exe'
)
$stopped = @(Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $path = $_.Path; $path -and ($targets | Where-Object { $path -like $_ }) })
foreach ($process in $stopped) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    [Console]::Error.WriteLine("テスト用のプロセスを止めた: $($process.Name) ($($process.Id))")
}
exit 0
