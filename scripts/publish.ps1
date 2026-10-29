$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    cargo build --config src/.cargo/config.toml --manifest-path src/Cargo.toml --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $null = New-Item -ItemType Directory -Path artifacts -Force
    Copy-Item -LiteralPath src/target/release/snippinglite.exe -Destination artifacts/snippinglite.exe -Force
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath artifacts/snippinglite.exe).Hash.ToLower()
    [IO.File]::WriteAllText("$PWD/artifacts/snippinglite.exe.sha256", $hash, [Text.Encoding]::ASCII)
    # The license and third-party notices are embedded in the executable (tray menu > About).
    Copy-Item -LiteralPath scripts/install.ps1, scripts/uninstall.ps1 -Destination artifacts -Force
    # artifacts/ holds exactly the GitHub release assets.
    $assets = @('snippinglite.exe', 'snippinglite.exe.sha256', 'install.ps1', 'uninstall.ps1')
    Get-ChildItem -LiteralPath artifacts -Force | Where-Object { $assets -notcontains $_.Name } |
        Remove-Item -Recurse -Force
    Write-Host 'Release executable, checksum and install scripts are in artifacts/.'
} finally { Pop-Location }
