# CI の画面のテストが失敗したり止められたりしたとき、そのときのデスクトップを ci-logs/<名前>.png に
# 残す。ランナーの画面は見えないので、窓が残っていないか、何が前面にあるかを、これで確かめる。
# 使い方: pwsh -File scripts/ci-screenshot.ps1 -Name screen
param([Parameter(Mandatory)][string]$Name)
$ErrorActionPreference = 'Stop'
$folder = Join-Path (Split-Path $PSScriptRoot -Parent) 'ci-logs'
$null = New-Item -ItemType Directory -Path $folder -Force
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
try {
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    } finally { $graphics.Dispose() }
    $bitmap.Save((Join-Path $folder "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
} finally { $bitmap.Dispose() }
Write-Output "Saved ci-logs/$Name.png ($($bounds.Width)x$($bounds.Height))."
