# social-preview.ps1 - render the GitHub social preview card (1280x640 PNG)
# for repo Settings > Social preview upload.
# Must run in STA: powershell -NoProfile -STA -File tools\social-preview.ps1
# NOTE: keep this file pure ASCII (PowerShell 5.1 reads no-BOM files as ANSI).
#
# Optional parameters:
#   -SvgPath <path>  favicon.svg source. Default: auto-resolve from `npm prefix -g`
#   -OutPath <path>  output PNG. Default: <project root>\assets\social-preview.png

param(
    [string]$SvgPath = "",
    [string]$OutPath = ""
)

$ErrorActionPreference = 'Stop'

$Root = Split-Path $PSScriptRoot -Parent
if ($OutPath -eq "") { $OutPath = Join-Path $Root "assets\social-preview.png" }

if ($SvgPath -eq "") {
    $npmPrefix = ""
    try { $npmPrefix = ((& npm prefix -g) | Select-Object -Last 1) } catch { }
    if ($npmPrefix) {
        $candidate = Join-Path $npmPrefix "node_modules\@deepseek-ai\dsh\node_modules\@deepseek-ai\dsh-web-frontend\dist\favicon.svg"
        if (Test-Path $candidate) { $SvgPath = $candidate }
    }
    if ($SvgPath -eq "") {
        throw "favicon.svg not found. Pass -SvgPath: powershell -NoProfile -STA -File tools\social-preview.ps1 -SvgPath 'C:\path\to\favicon.svg'"
    }
}

Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase

$W = 1280
$H = 640
$svg = Get-Content $SvgPath -Raw
$m = [regex]::Match($svg, '<path[^>]*\sd="([^"]+)"')
if (-not $m.Success) { $m = [regex]::Match($svg, '\sd="([^"]+)"') }
if (-not $m.Success) { throw "SVG path 'd' not found" }
$geo = [System.Windows.Media.Geometry]::Parse($m.Groups[1].Value)
$b = $geo.Bounds

$rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap($W, $H, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
$dv = New-Object System.Windows.Media.DrawingVisual
$dc = $dv.RenderOpen()

# background: GitHub-dark vertical gradient
$bgBrush = New-Object System.Windows.Media.LinearGradientBrush
$bgBrush.StartPoint = New-Object System.Windows.Point(0, 0)
$bgBrush.EndPoint = New-Object System.Windows.Point(0, 1)
$bgBrush.GradientStops.Add((New-Object System.Windows.Media.GradientStop([System.Windows.Media.Color]::FromRgb(13, 17, 23), 0)))
$bgBrush.GradientStops.Add((New-Object System.Windows.Media.GradientStop([System.Windows.Media.Color]::FromRgb(28, 33, 44), 1)))
$dc.DrawRectangle($bgBrush, $null, (New-Object System.Windows.Rect(0, 0, $W, $H)))

# accent glow behind the whale
$glow = New-Object System.Windows.Media.RadialGradientBrush
$glow.GradientStops.Add((New-Object System.Windows.Media.GradientStop([System.Windows.Media.Color]::FromArgb(96, 77, 107, 254), 0)))
$glow.GradientStops.Add((New-Object System.Windows.Media.GradientStop([System.Windows.Media.Color]::FromArgb(0, 77, 107, 254), 1)))
$dc.DrawEllipse($glow, $null, (New-Object System.Windows.Point(330, 320)), 250, 250)

# whale in white, fitted into a 320px box centered at (330, 320)
$contentSize = [Math]::Max($b.Width, $b.Height)
if ($contentSize -le 0) { $contentSize = 50 }
$scale = 320 / $contentSize
$offX = 330 - ($b.Width * $scale) / 2.0 - $b.X * $scale
$offY = 320 - ($b.Height * $scale) / 2.0 - $b.Y * $scale
$dc.PushTransform((New-Object System.Windows.Media.TranslateTransform($offX, $offY)))
$dc.PushTransform((New-Object System.Windows.Media.ScaleTransform($scale, $scale)))
$dc.DrawGeometry([System.Windows.Media.Brushes]::White, $null, $geo)
$dc.Pop()
$dc.Pop()

function New-Text([string]$s, [double]$size, $brush, [string]$weight) {
    $typeface = New-Object System.Windows.Media.Typeface("Segoe UI")
    $ft = New-Object System.Windows.Media.FormattedText($s, [System.Globalization.CultureInfo]::InvariantCulture, [System.Windows.FlowDirection]::LeftToRight, $typeface, $size, $brush, 1.25)
    $ft.SetFontWeight([System.Windows.FontWeights]::$weight)
    return $ft
}

$white = [System.Windows.Media.Brushes]::White
$gray1 = New-Object System.Windows.Media.SolidColorBrush([System.Windows.Media.Color]::FromRgb(139, 148, 158))
$gray2 = New-Object System.Windows.Media.SolidColorBrush([System.Windows.Media.Color]::FromRgb(110, 118, 129))

$title = New-Text "DSH Manager" 76 $white "SemiBold"
$sub1 = New-Text "DeepSeek Harness desktop manager" 30 $gray1 "Regular"
$sub2 = New-Text "tray resident  -  one-click control  -  plugin marketplace  -  AI analysis" 22 $gray2 "Regular"

$dc.DrawText($title, (New-Object System.Windows.Point(560, 200)))
$dc.DrawText($sub1, (New-Object System.Windows.Point(562, 330)))
$dc.DrawText($sub2, (New-Object System.Windows.Point(562, 380)))

# tech pills
$pills = @("Electron", "Windows", "MIT")
$x = 562
foreach ($p in $pills) {
    $txt = New-Text $p 20 $white "SemiBold"
    $pw = $txt.Width + 36
    $pillGeo = New-Object System.Windows.Media.RectangleGeometry((New-Object System.Windows.Rect($x, 440, $pw, 40)), 20, 20)
    $pillBrush = New-Object System.Windows.Media.SolidColorBrush([System.Windows.Media.Color]::FromArgb(44, 110, 140, 255))
    $dc.DrawGeometry($pillBrush, $null, $pillGeo)
    $px = $x + 18
    $dc.DrawText($txt, (New-Object System.Windows.Point($px, 448)))
    $x = $x + $pw + 16
}

$dc.Close()
$rtb.Render($dv)

$encoder = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
$encoder.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
$fs = [System.IO.File]::Create($OutPath)
try { $encoder.Save($fs) } finally { $fs.Close() }
Write-Output ("Social preview written: " + $OutPath)
