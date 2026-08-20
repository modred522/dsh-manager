# Render the DSH whale favicon.svg into multi-size ICO + PNG (black whale).
# Must run in STA: powershell -NoProfile -STA -File tools\render-icon.ps1
# NOTE: keep this file pure ASCII (PowerShell 5.1 reads no-BOM files as ANSI).
#
# Optional parameters:
#   -SvgPath <path>   favicon.svg source. Default: auto-resolve from `npm prefix -g`
#                     (node_modules\@deepseek-ai\dsh\...\dsh-web-frontend\dist\favicon.svg)
#   -OutDir  <path>   output directory. Default: <project root>\assets

param(
    [string]$SvgPath = "",
    [string]$OutDir = ""
)

$ErrorActionPreference = 'Stop'

if ($SvgPath -eq "") {
    $npmPrefix = ""
    try { $npmPrefix = ((& npm prefix -g) | Select-Object -Last 1) } catch { }
    if ($npmPrefix) {
        $candidate = Join-Path $npmPrefix "node_modules\@deepseek-ai\dsh\node_modules\@deepseek-ai\dsh-web-frontend\dist\favicon.svg"
        if (Test-Path $candidate) { $SvgPath = $candidate }
    }
    if ($SvgPath -eq "") {
        throw "favicon.svg not found. Pass -SvgPath explicitly: powershell -NoProfile -STA -File tools\render-icon.ps1 -SvgPath 'C:\path\to\dsh-web-frontend\dist\favicon.svg'"
    }
}

if ($OutDir -eq "") {
    $OutDir = Join-Path (Split-Path $PSScriptRoot -Parent) "assets"
}

$content = Get-Content $SvgPath -Raw
$m = [regex]::Match($content, '<path[^>]*\sd="([^"]+)"')
if (-not $m.Success) { $m = [regex]::Match($content, '\sd="([^"]+)"') }
if (-not $m.Success) { throw "SVG path 'd' not found" }
$d = $m.Groups[1].Value

Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase

$geo = [System.Windows.Media.Geometry]::Parse($d)
$b = $geo.Bounds
Write-Output ("Geometry bounds: X={0} Y={1} W={2} H={3}" -f $b.X, $b.Y, $b.Width, $b.Height)

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function Render-Png([int]$px, [string]$path) {
    $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap($px, $px, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
    $dv = New-Object System.Windows.Media.DrawingVisual
    $dc = $dv.RenderOpen()

    $contentSize = [Math]::Max($b.Width, $b.Height)
    if ($contentSize -le 0) { $contentSize = 50 }
    $scale = ($px * 0.82) / $contentSize
    $offX = ($px - $b.Width * $scale) / 2.0 - $b.X * $scale
    $offY = ($px - $b.Height * $scale) / 2.0 - $b.Y * $scale
    $matrix = New-Object System.Windows.Media.Matrix($scale, 0, 0, $scale, $offX, $offY)
    $dc.PushTransform((New-Object System.Windows.Media.MatrixTransform($matrix)))
    $dc.DrawGeometry([System.Windows.Media.Brushes]::Black, $null, $geo)
    $dc.Pop()
    $dc.Close()

    $rtb.Render($dv)

    $encoder = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
    $encoder.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
    $fs = [System.IO.File]::Create($path)
    try { $encoder.Save($fs) } finally { $fs.Close() }
    Write-Output ("Rendered {0}x{0} -> {1}" -f $px, $path)
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngs = @{}
foreach ($s in $sizes) {
    $p = Join-Path $OutDir ("whale_{0}.png" -f $s)
    Render-Png $s $p
    $pngs[$s] = $p
}

# High-res copy used by UI headers.
Copy-Item $pngs[256] (Join-Path $OutDir "whale.png") -Force

# Assemble multi-size ICO (PNG entries).
$icoPath = $OutDir + "\app.ico"
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter($fs)
try {
    $count = $sizes.Count
    $bw.Write([UInt16]0)      # reserved
    $bw.Write([UInt16]1)      # type = icon
    $bw.Write([UInt16]$count) # image count

    $offset = 6 + 16 * $count
    $entries = @()
    foreach ($s in $sizes) {
        $bytes = [System.IO.File]::ReadAllBytes($pngs[$s])
        $w = if ($s -ge 256) { 0 } else { $s }
        $h = if ($s -ge 256) { 0 } else { $s }
        $entries += [pscustomobject]@{ W = $w; H = $h; Bytes = $bytes; Offset = $offset }
        $offset += $bytes.Length
    }
    foreach ($e in $entries) {
        $bw.Write([Byte]$e.W)
        $bw.Write([Byte]$e.H)
        $bw.Write([Byte]0)     # color count
        $bw.Write([Byte]0)     # reserved
        $bw.Write([UInt16]1)   # planes
        $bw.Write([UInt16]32)  # bpp
        $bw.Write([UInt32]$e.Bytes.Length)
        $bw.Write([UInt32]$e.Offset)
    }
    foreach ($e in $entries) {
        $bw.Write($e.Bytes)
    }
}
finally {
    $bw.Close()
}

Write-Output ("ICO written: {0}" -f $icoPath)
Write-Output "DONE"
