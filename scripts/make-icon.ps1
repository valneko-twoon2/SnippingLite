# Renders src/app.svg into the executable icon and the shared 64px RGBA bitmap.
# All application surfaces use this same capture-frame-and-scissors design.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot -Parent
[xml]$source = Get-Content -LiteralPath (Join-Path $root 'src/app.svg') -Raw
$invariant = [Globalization.CultureInfo]::InvariantCulture

function Read-Number([string]$value) {
    [single]::Parse($value, $invariant)
}

function Read-Points([string]$value) {
    [Drawing.PointF[]]@($value.Trim() -split '\s+' | ForEach-Object {
        $pair = $_ -split ','
        [Drawing.PointF]::new((Read-Number $pair[0]), (Read-Number $pair[1]))
    })
}

$images = foreach ($size in 16, 20, 24, 32, 40, 48, 64, 128, 256) {
    $large = [Drawing.Bitmap]::new(($size * 4), ($size * 4), [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($large)
    $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([Drawing.Color]::Transparent)
    $graphics.ScaleTransform([single]($size * 4 / 128), [single]($size * 4 / 128))
    foreach ($shape in $source.DocumentElement.ChildNodes) {
        switch ($shape.LocalName) {
            'polyline' {
                $pen = [Drawing.Pen]::new([Drawing.ColorTranslator]::FromHtml($shape.stroke), (Read-Number $shape.'stroke-width'))
                $pen.StartCap = $pen.EndCap = [Drawing.Drawing2D.LineCap]::Round
                $pen.LineJoin = [Drawing.Drawing2D.LineJoin]::Round
                $graphics.DrawLines($pen, [Drawing.PointF[]](Read-Points $shape.points))
                $pen.Dispose()
            }
            'polygon' {
                $brush = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml($shape.fill))
                $graphics.FillPolygon($brush, [Drawing.PointF[]](Read-Points $shape.points))
                $brush.Dispose()
            }
            'circle' {
                $radius = Read-Number $shape.r
                $circle = [Drawing.RectangleF]::new(((Read-Number $shape.cx) - $radius), ((Read-Number $shape.cy) - $radius), (2 * $radius), (2 * $radius))
                if ($shape.fill -ne 'none') {
                    $brush = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml($shape.fill))
                    $graphics.FillEllipse($brush, $circle)
                    $brush.Dispose()
                }
                if ($shape.stroke -ne 'none') {
                    $pen = [Drawing.Pen]::new([Drawing.ColorTranslator]::FromHtml($shape.stroke), (Read-Number $shape.'stroke-width'))
                    $graphics.DrawEllipse($pen, $circle)
                    $pen.Dispose()
                }
            }
            default { throw "Unsupported icon geometry: $($shape.LocalName)" }
        }
    }
    $graphics.Dispose()
    $bitmap = [Drawing.Bitmap]::new($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.DrawImage($large, [Drawing.Rectangle]::new(0, 0, $size, $size))
    if ($size -eq 64) {
        $rgba = [byte[]]::new(64 * 64 * 4)
        for ($y = 0; $y -lt 64; $y++) {
            for ($x = 0; $x -lt 64; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                $offset = ($y * 64 + $x) * 4
                $rgba[$offset] = $pixel.R
                $rgba[$offset + 1] = $pixel.G
                $rgba[$offset + 2] = $pixel.B
                $rgba[$offset + 3] = $pixel.A
            }
        }
        [IO.File]::WriteAllBytes((Join-Path $root 'src/app.rgba'), $rgba)
    }
    $stream = [IO.MemoryStream]::new()
    $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose(); $large.Dispose(); $bitmap.Dispose()
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
