# Renders the same blue tile and selection corners as ui::icon_rgba.
# build.rs embeds src/app.ico for Explorer and Task Manager.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot -Parent

$images = foreach ($size in 16, 20, 24, 32, 40, 48, 64, 128, 256) {
    $bitmap = [Drawing.Bitmap]::new($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([Drawing.Color]::Transparent)
    $tile = [Drawing.Drawing2D.GraphicsPath]::new()
    $diameter = [single]($size * 0.44)
    $edge = [single]($size - $diameter)
    $tile.AddArc(0, 0, $diameter, $diameter, 180, 90)
    $tile.AddArc($edge, 0, $diameter, $diameter, 270, 90)
    $tile.AddArc($edge, $edge, $diameter, $diameter, 0, 90)
    $tile.AddArc(0, $edge, $diameter, $diameter, 90, 90)
    $tile.CloseFigure()
    $brush = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(0, 103, 192))
    $graphics.FillPath($brush, $tile)
    $pen = [Drawing.Pen]::new([Drawing.Color]::White, [single]($size * 0.08))
    foreach ($sx in -1, 1) {
        foreach ($sy in -1, 1) {
            $x = [single]($size * (0.5 + $sx * 0.21))
            $y = [single]($size * (0.5 + $sy * 0.21))
            $innerX = [single]($x - $sx * $size * 0.09)
            $innerY = [single]($y - $sy * $size * 0.09)
            $graphics.DrawLine($pen, $x, $y, $innerX, $y)
            $graphics.DrawLine($pen, $x, $y, $x, $innerY)
        }
    }
    $stream = [IO.MemoryStream]::new()
    $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
    $pen.Dispose(); $tile.Dispose(); $brush.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
    [pscustomobject]@{ Size = $size; Data = $stream.ToArray() }
}

# ICONDIR, one ICONDIRENTRY per image, then the PNG-compressed images.
$out = [IO.MemoryStream]::new()
$writer = [IO.BinaryWriter]::new($out)
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$images.Count)
$offset = 6 + 16 * $images.Count
foreach ($image in $images) {
    $edge = if ($image.Size -ge 256) { 0 } else { $image.Size }
    $writer.Write([byte]$edge); $writer.Write([byte]$edge); $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([uint16]1); $writer.Write([uint16]32)
    $writer.Write([uint32]$image.Data.Length); $writer.Write([uint32]$offset)
    $offset += $image.Data.Length
}
foreach ($image in $images) { $writer.Write($image.Data) }
$writer.Flush()
[IO.File]::WriteAllBytes((Join-Path $root 'src/app.ico'), $out.ToArray())
Write-Host "Wrote src/app.ico ($($images.Count) sizes)."
