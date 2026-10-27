param([string]$MessageFile)
$message = Get-Content -Raw -LiteralPath $MessageFile
if ($message -match '(?im)^Co-Authored-By:') { Write-Error 'Attribution trailers are not allowed.' }
if ($message -notmatch '^(feat|fix|docs|refactor|test|chore|build|ci|perf)(\([^)]+\))?!?: .+') { Write-Error 'Use an English Conventional Commit subject.' }
