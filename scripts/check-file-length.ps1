param([switch]$Hook)
$ErrorActionPreference = 'Stop'
# Rejects source files longer than $Limit lines so that no file grows into a grab bag.
# A justified exception is marked with `// length-ok: <reason>` (`# length-ok:` in scripts)
# within the first five lines of the file.
# With -Hook the script is a Claude Code hook: it reads the tool call from stdin, checks only
# the file that was just written and exits with 2 so that the message reaches the agent.
$Limit = 500
$root = Split-Path $PSScriptRoot -Parent
$extensions = '.rs', '.ps1', '.cmd'

function Check-File([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if (!(Test-Path -LiteralPath $full -PathType Leaf)) { return $null }
    if ($full.StartsWith((Join-Path $root 'src/target'))) { return $null }
    if ($extensions -notcontains [IO.Path]::GetExtension($full)) { return $null }
    if ($full.StartsWith((Join-Path $root 'scripts/licenses'))) { return $null }
    $lines = [IO.File]::ReadAllLines($full)
    if ($lines.Count -le $Limit) { return $null }
    if (($lines | Select-Object -First 5) -match 'length-ok:') { return $null }
    $relative = $full.Substring($root.Length + 1).Replace('\', '/')
    "${relative}: $($lines.Count) lines; the limit is $Limit."
}

$advice = @'
Split the file by responsibility until each part is at most 500 lines:
- Rust: move a cohesive group of items, or the `mod tests` block, into a child module file (`foo.rs` + `foo/bar.rs`, declared with `mod bar;`) and re-export with `use bar::*;` when other files use the items. Split long test files by topic into `tests/<topic>.rs` with `use super::*;`.
- Scripts: move helpers into a dot-sourced or called script.
Do not shorten the file by deleting tests, comments or blank lines, and do not bypass the check. Only a file that cannot be split sensibly (for example generated data) may carry a `length-ok: <reason>` marker in its first five lines.
'@

$problems = [Collections.Generic.List[string]]::new()
if ($Hook) {
    $call = [Console]::In.ReadToEnd() | ConvertFrom-Json -ErrorAction SilentlyContinue
    $path = $call.tool_input.file_path
    if ($path) { $problem = Check-File $path; if ($problem) { $problems.Add($problem) } }
} else {
    Push-Location $root
    try {
        $paths = @(git -c core.quotepath=false ls-files --cached --others --exclude-standard)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate repository files.' }
    } finally { Pop-Location }
    foreach ($relative in $paths | Sort-Object -Unique) {
        $problem = Check-File (Join-Path $root $relative)
        if ($problem) { $problems.Add($problem) }
    }
}

if ($problems.Count -gt 0) {
    $message = ($problems -join "`n") + "`n" + $advice
    if ($Hook) { [Console]::Error.WriteLine($message); exit 2 }
    Write-Error $message -ErrorAction Continue
    exit 1
}
if (!$Hook) { Write-Output 'File length check passed.' }
