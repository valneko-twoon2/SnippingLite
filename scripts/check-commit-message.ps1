param([string]$MessageFile)
$message = Get-Content -Raw -LiteralPath $MessageFile
$problems = @()
# 帰属表記のトレーラーは付けない（AGENTS.md）。大文字と小文字、前後の空白の違いも拒む。
if ($message -match '(?im)^\s*Co-Authored-By\s*:') { $problems += 'Attribution trailers (Co-Authored-By) are not allowed.' }
if ($message -notmatch '^(feat|fix|docs|refactor|test|chore|build|ci|perf)(\([^)]+\))?!?: .+') { $problems += 'Use a Conventional Commit subject (type in English, description in Japanese).' }
# Write-Error だけでは終了コードが 0 のままで、フックがコミットを止めない。
if ($problems.Count -gt 0) {
    $problems | ForEach-Object { [Console]::Error.WriteLine($_) }
    exit 1
}
