$ErrorActionPreference = 'Stop'
# Rejects UI text that is not vertically centered. A justified exception is
# marked with `// align-ok: <reason>` on the same or the previous line.
$root = Split-Path $PSScriptRoot -Parent
$problems = [Collections.Generic.List[string]]::new()
$anchors = 'LEFT_TOP|LEFT_BOTTOM|RIGHT_TOP|RIGHT_BOTTOM|CENTER_TOP|CENTER_BOTTOM'

function Line-Of([string]$Text, [int]$Index) { ($Text.Substring(0, $Index) -split "`n").Count }

$src = Join-Path $root 'src'
$build = Join-Path $src 'target'
$sources = Get-ChildItem -LiteralPath $src -Filter '*.rs' -Recurse |
    Where-Object { !$_.FullName.StartsWith($build + [IO.Path]::DirectorySeparatorChar) }
foreach ($file in $sources) {
    $text = [IO.File]::ReadAllText($file.FullName)
    $lines = $text -split "`r?`n"
    $relative = 'src/' + $file.FullName.Substring($src.Length + 1).Replace('\', '/')

    # egui: text anchors must be centered vertically.
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "Align2::($anchors)\b") {
            $previous = if ($i -gt 0) { $lines[$i - 1] } else { '' }
            if ($lines[$i] -notmatch 'align-ok' -and $previous -notmatch 'align-ok') {
                $problems.Add("${relative}:$($i + 1): Align2::$($Matches[1]) is not vertically centered; use a *_CENTER anchor.")
            }
        }
    }

    # Win32: every Canvas::text call must pass DT_VCENTER.
    foreach ($call in [regex]::Matches($text, '\bcanvas\s*\.text\((.*?)\);', 'Singleline')) {
        $line = Line-Of $text $call.Index
        $previous = if ($line -gt 1) { $lines[$line - 2] } else { '' }
        if ($call.Value -notmatch 'DT_VCENTER' -and $call.Value -notmatch 'align-ok' -and $previous -notmatch 'align-ok') {
            $problems.Add("${relative}:${line}: canvas.text without DT_VCENTER is not vertically centered.")
        }
    }
}

# The Japanese font sits high in its line box; the tweak that centers it must stay.
$fonts = [IO.File]::ReadAllText((Join-Path $root 'src/settings_ui.rs'))
if ($fonts -notmatch 'JAPANESE_Y_OFFSET' -or $fonts -notmatch 'y_offset_factor') {
    $problems.Add('src/settings_ui.rs: the Japanese font y_offset_factor tweak (JAPANESE_Y_OFFSET) is missing.')
}

if ($problems.Count -gt 0) {
    $problems | ForEach-Object { Write-Error $_ -ErrorAction Continue }
    exit 1
}
Write-Output 'Text alignment check passed.'
