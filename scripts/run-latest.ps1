$ErrorActionPreference = 'Stop'
$exe = Join-Path (Split-Path $PSScriptRoot -Parent) 'src/target/release/snippinglite.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw "Build the release binary first: $exe" }
Start-Process -FilePath $exe
