param([switch]$Check)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$utf8 = [Text.UTF8Encoding]::new($false)
$expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)

function Sync-Text([string]$Relative, [string]$Content) {
    $null = $expected.Add($Relative.Replace('\', '/'))
    $path = Join-Path $root $Relative
    $content = ($Content -replace '\r?\n', "`r`n").TrimEnd("`r", "`n") + "`r`n"
    if ($Check) {
        if (!(Test-Path -LiteralPath $path) -or [IO.File]::ReadAllText($path) -cne $content) {
            throw "Harness mirror is stale: $Relative. Run scripts/sync-harness.ps1."
        }
    } else {
        $null = [IO.Directory]::CreateDirectory((Split-Path $path -Parent))
        [IO.File]::WriteAllText($path, $content, $utf8)
    }
}

$source = Join-Path $root '.agents/skills'
foreach ($file in Get-ChildItem -LiteralPath $source -File -Recurse) {
    $relative = [IO.Path]::GetRelativePath($source, $file.FullName)
    Sync-Text ".claude/skills/$relative" ([IO.File]::ReadAllText($file.FullName))
}

$command = 'pwsh -NoProfile -File "$(git rev-parse --show-toplevel)/scripts/agent-stop.ps1"'
$hooks = [ordered]@{ Stop = @([ordered]@{ hooks = @([ordered]@{ type = 'command'; command = $command; timeout = 15 }) }) }
Sync-Text '.codex/hooks.json' ([ordered]@{ hooks = $hooks } | ConvertTo-Json -Depth 10)
# Claude Code also checks the file length right after each edit, so a file is split as soon as it grows too long.
$lengthCommand = 'pwsh -NoProfile -File "$(git rev-parse --show-toplevel)/scripts/check-file-length.ps1" -Hook'
# テストを走らせたコマンドの終わりに、残ったテスト用のプロセスを止める（画面を覆ったまま残さない）。
$stopCommand = 'pwsh -NoProfile -File "$(git rev-parse --show-toplevel)/scripts/stop-test-processes.ps1" -Hook'
$claudeHooks = [ordered]@{
    PostToolUse = @(
        [ordered]@{ matcher = 'Edit|Write|MultiEdit'; hooks = @([ordered]@{ type = 'command'; command = $lengthCommand; timeout = 15 }) }
        [ordered]@{ matcher = 'Bash|PowerShell'; hooks = @([ordered]@{ type = 'command'; command = $stopCommand; timeout = 30 }) }
    )
    Stop = $hooks.Stop
}
Sync-Text '.claude/settings.json' ([ordered]@{ hooks = $claudeHooks } | ConvertTo-Json -Depth 10)

$directory = Join-Path $root '.claude/skills'
if (Test-Path -LiteralPath $directory) {
    foreach ($file in Get-ChildItem -LiteralPath $directory -File -Recurse) {
        $relative = [IO.Path]::GetRelativePath($root, $file.FullName).Replace('\', '/')
        if (!$expected.Contains($relative)) { throw "Remove obsolete mirror explicitly: $relative" }
    }
}
foreach ($unsupported in @('.agents/rules', '.codex/rules', '.codex/skills')) {
    if (Test-Path -LiteralPath (Join-Path $root $unsupported)) {
        throw "Unsupported project harness location: $unsupported"
    }
}
