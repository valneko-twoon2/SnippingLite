$ErrorActionPreference = 'Stop'
git status --short
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
