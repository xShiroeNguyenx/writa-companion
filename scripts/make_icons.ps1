# Sinh bộ icon cho src-tauri/icons.
#
# Vì sao vẽ bằng script thay vì để file ảnh trong repo: icon ở đây là chỗ giữ chỗ,
# và một script 60 dòng thì sửa màu / sửa chữ được ngay, còn file PNG thì phải mở
# trình vẽ. Khi có bộ nhận diện thật thì thay bằng ảnh và xoá script này.

param(
    [string]$OutDir = (Join-Path $PSScriptRoot "..\src-tauri\icons")
)

Add-Type -AssemblyName System.Drawing

$OutDir = [System.IO.Path]::GetFullPath($OutDir)
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Force -Path $OutDir | Out-Null }

function New-Mark {
    param([int]$Size, [string]$Hex)

    $bmp = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $g.Clear([System.Drawing.Color]::Transparent)

    # Nền bo góc
    $r = [int]($Size * 0.22)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($Size - $d, 0, $d, $d, 270, 90)
    $path.AddArc($Size - $d, $Size - $d, $d, $d, 0, 90)
    $path.AddArc(0, $Size - $d, $d, $d, 90, 90)
    $path.CloseFigure()

    $color = [System.Drawing.ColorTranslator]::FromHtml($Hex)
    $brush = New-Object System.Drawing.SolidBrush($color)
    $g.FillPath($brush, $path)

    # Chữ W
    $fontSize = [float]($Size * 0.58)
    $font = New-Object System.Drawing.Font("Segoe UI", $fontSize, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
    $fmt = New-Object System.Drawing.StringFormat
    $fmt.Alignment = [System.Drawing.StringAlignment]::Center
    $fmt.LineAlignment = [System.Drawing.StringAlignment]::Center
    $white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
    # Nhích lên một chút vì Segoe UI có phần đệm dưới lớn hơn phần trên
    $rect = New-Object System.Drawing.RectangleF(0, -($Size * 0.04), $Size, $Size)
    $g.DrawString("W", $font, $white, $rect, $fmt)

    $brush.Dispose(); $white.Dispose(); $font.Dispose(); $path.Dispose(); $g.Dispose()
    return $bmp
}

$active = "#4F46E5"   # indigo — đang bật
$paused = "#71717A"   # xám    — tạm dừng

foreach ($spec in @(
    @{ File = "32x32.png";      Size = 32;  Color = $active },
    @{ File = "128x128.png";    Size = 128; Color = $active },
    @{ File = "128x128@2x.png"; Size = 256; Color = $active },
    @{ File = "icon.png";       Size = 512; Color = $active },
    @{ File = "tray.png";       Size = 64;  Color = $active },
    @{ File = "tray-paused.png";Size = 64;  Color = $paused }
)) {
    $bmp = New-Mark -Size $spec.Size -Hex $spec.Color
    $path = Join-Path $OutDir $spec.File
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Output "  $($spec.File)  $($spec.Size)x$($spec.Size)"
}

# .ico cho exe. Đi qua GetHicon để ra ICO dạng BMP cổ điển — trình biên dịch tài
# nguyên của Windows chắc chắn đọc được, khác với ICO nhúng PNG vốn tuỳ phiên bản.
$bmp = New-Mark -Size 128 -Hex $active
$hicon = $bmp.GetHicon()
$icon = [System.Drawing.Icon]::FromHandle($hicon)
$icoPath = Join-Path $OutDir "icon.ico"
$fs = [System.IO.File]::Create($icoPath)
$icon.Save($fs)
$fs.Close()
$icon.Dispose()
$bmp.Dispose()
Write-Output "  icon.ico     128x128"

Write-Output "Xong: $OutDir"
