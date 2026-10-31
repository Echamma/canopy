# Draws Canopy's icon: a deep forest tile with three app leaves joined by branches to the sun hub.
# Writes assets/canopy.png (256 px) and assets/canopy.ico. Uses System.Drawing, which ships with Windows.
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
$assets = Join-Path (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)) "assets"

function Leaf($g, $brush, [float]$x, [float]$y, [float]$s) {
    # Half-size radius on the top-left and bottom-right corners, a sixth on the others.
    $big = $s; $small = $s / 3
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc($x, $y, $big, $big, 180, 90)
    $p.AddArc($x + $s - $small, $y, $small, $small, 270, 90)
    $p.AddArc($x + $s - $big, $y + $s - $big, $big, $big, 0, 90)
    $p.AddArc($x, $y + $s - $small, $small, $small, 90, 90)
    $p.CloseFigure()
    $g.FillPath($brush, $p)
}
function Color($hex) { [System.Drawing.ColorTranslator]::FromHtml($hex) }

function New-IconPng([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    $k = $size / 256.0
    $tile = New-Object System.Drawing.Drawing2D.GraphicsPath
    $r = 56 * $k; $m = 8 * $k; $s = $size - 2 * $m
    $tile.AddArc($m, $m, $r, $r, 180, 90); $tile.AddArc($m + $s - $r, $m, $r, $r, 270, 90)
    $tile.AddArc($m + $s - $r, $m + $s - $r, $r, $r, 0, 90); $tile.AddArc($m, $m + $s - $r, $r, $r, 90, 90); $tile.CloseFigure()
    $g.FillPath((New-Object System.Drawing.SolidBrush (Color "#1F3D2B")), $tile)
    $pen = New-Object System.Drawing.Pen (Color "#8CC63F"), ([float](9 * $k))
    $pen.StartCap = $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $hub = @{ x = 128 * $k; y = 190 * $k }
    foreach ($end in @(@(70, 104), @(128, 84), @(186, 104))) { $g.DrawLine($pen, [float]$hub.x, [float]$hub.y, [float]($end[0] * $k), [float]($end[1] * $k)) }
    Leaf $g (New-Object System.Drawing.SolidBrush (Color "#B85A2E")) (38 * $k) (74 * $k) (60 * $k)
    Leaf $g (New-Object System.Drawing.SolidBrush (Color "#8CC63F")) (94 * $k) (40 * $k) (68 * $k)
    Leaf $g (New-Object System.Drawing.SolidBrush (Color "#2A7596")) (158 * $k) (74 * $k) (60 * $k)
    $g.FillEllipse((New-Object System.Drawing.SolidBrush (Color "#D9A441")), [float](108 * $k), [float](170 * $k), [float](40 * $k), [float](40 * $k))
    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return ,$ms.ToArray()
}

[System.IO.File]::WriteAllBytes((Join-Path $assets "canopy.png"), (New-IconPng 256))
# ICO container with PNG-compressed entries (supported since Windows Vista).
$sizes = 16, 24, 32, 48, 64, 128, 256
$images = $sizes | ForEach-Object { ,(New-IconPng $_) }
$fs = [System.IO.File]::Create((Join-Path $assets "canopy.ico"))
$w = New-Object System.IO.BinaryWriter $fs
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32)
    $w.Write([UInt32]$images[$i].Length); $w.Write([UInt32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $w.Write($img) }
$w.Close()
Write-Host "Wrote assets/canopy.png and assets/canopy.ico"
