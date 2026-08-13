# Sinh toàn bộ icon của Writa từ một ảnh nguồn.
#
# ## Cách dùng
#
#   .\scripts\make_icons.ps1 -Source D:\Writa-icon.png
#
# ## Việc script làm, và vì sao từng bước cần thiết
#
# 1. **Đệm thành ảnh vuông.** `tauri icon` yêu cầu ảnh vuông; ảnh không vuông bị nó co
#    méo. Đệm bằng vùng trong suốt giữ nguyên tỉ lệ.
# 2. **Gọi `tauri icon`** để sinh bộ icon cho installer và cho cửa sổ (`.ico` nhiều kích
#    thước, các `.png` theo mật độ điểm ảnh).
# 3. **Sinh riêng hai icon khay hệ thống.** `tauri icon` không làm phần này. Bản "tạm
#    dừng" là bản xám: icon khay là chỗ duy nhất user biết Writa đang bật hay tắt **mà
#    không cần mở menu**, nên hai trạng thái phải phân biệt được từ xa, ở cỡ 16 px.
# 4. **Sao sang `ui/public/`** cho tiêu đề cửa sổ cài đặt.

param(
    [string]$Source = "D:\Writa-icon.png"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path $PSScriptRoot -Parent
$iconDir = Join-Path $root "src-tauri\icons"
if (-not (Test-Path $Source)) { throw "Khong thay anh nguon: $Source" }

# --- 1. Đệm thành vuông ------------------------------------------------------
$src = [System.Drawing.Image]::FromFile($Source)
$side = [Math]::Max($src.Width, $src.Height)
Write-Output "Anh nguon : $($src.Width) x $($src.Height)  ->  vuong $side x $side"

$square = New-Object System.Drawing.Bitmap($side, $side, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($square)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.Clear([System.Drawing.Color]::Transparent)
$g.DrawImage($src, [int](($side - $src.Width) / 2), [int](($side - $src.Height) / 2), $src.Width, $src.Height)
$g.Dispose()
$src.Dispose()

$squarePath = Join-Path $env:TEMP "writa-icon-square.png"
$square.Save($squarePath, [System.Drawing.Imaging.ImageFormat]::Png)

# --- 2. Bộ icon của Tauri ---------------------------------------------------
Write-Output ""
Write-Output "Goi 'tauri icon'..."
Push-Location $root
try {
    # KHÔNG dùng `2>&1` ở đây. PowerShell 5.1 bọc từng dòng stderr của native command
    # thành ErrorRecord và đặt `$?` = false, nên với `$ErrorActionPreference = 'Stop'`
    # thì script chết dù `tauri icon` trả về 0 và làm đúng việc.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & npx --yes @tauri-apps/cli icon $squarePath | Out-Null
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    if ($code -ne 0) { throw "tauri icon that bai (ma $code)" }
    Write-Output "  xong"
} finally {
    Pop-Location
}

# `tauri icon` sinh cho MỌI nền tảng. Writa chỉ đóng gói NSIS cho Windows, nên phần còn
# lại là 1,3 MB rác trong repo: `.icns` của macOS, logo của Microsoft Store, thư mục
# android/ios. Bỏ đi — `bundle.icon` trong tauri.conf.json không tham chiếu cái nào.
$khongCan = @("icon.icns", "StoreLogo.png", "64x64.png") +
    (Get-ChildItem $iconDir -Filter "Square*Logo.png" -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
foreach ($n in $khongCan) {
    $f = Join-Path $iconDir $n
    if (Test-Path $f) { Remove-Item $f -Force }
}
foreach ($d in @("android", "ios")) {
    $p = Join-Path $iconDir $d
    if (Test-Path $p) { Remove-Item $p -Recurse -Force }
}
Write-Output "  da bo icon cua nen tang khong dung"

# --- 3. Icon khay hệ thống --------------------------------------------------
function Save-Tray {
    param([System.Drawing.Bitmap]$Src, [string]$Path, [bool]$Gray)

    $size = 64
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $gg = [System.Drawing.Graphics]::FromImage($bmp)
    $gg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $gg.Clear([System.Drawing.Color]::Transparent)

    if ($Gray) {
        # Ma trận chuyển sang xám theo trọng số cảm nhận sáng của mắt người
        # (0,299 / 0,587 / 0,114). Cột alpha giữ nguyên để vùng trong suốt không bị đặc.
        $m = New-Object System.Drawing.Imaging.ColorMatrix
        $m.Matrix00 = 0.299; $m.Matrix01 = 0.299; $m.Matrix02 = 0.299
        $m.Matrix10 = 0.587; $m.Matrix11 = 0.587; $m.Matrix12 = 0.587
        $m.Matrix20 = 0.114; $m.Matrix21 = 0.114; $m.Matrix22 = 0.114
        $m.Matrix33 = 0.55   # mờ thêm: "đang tắt" phải thấy được từ xa, không chỉ nhạt màu
        $m.Matrix44 = 1.0
        $attr = New-Object System.Drawing.Imaging.ImageAttributes
        $attr.SetColorMatrix($m)
        $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
        $gg.DrawImage($Src, $rect, 0, 0, $Src.Width, $Src.Height,
            [System.Drawing.GraphicsUnit]::Pixel, $attr)
        $attr.Dispose()
    } else {
        $gg.DrawImage($Src, 0, 0, $size, $size)
    }

    $gg.Dispose()
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

Write-Output ""
Write-Output "Icon khay he thong:"
Save-Tray -Src $square -Path (Join-Path $iconDir "tray.png") -Gray $false
Write-Output "  tray.png         (dang hoat dong)"
Save-Tray -Src $square -Path (Join-Path $iconDir "tray-paused.png") -Gray $true
Write-Output "  tray-paused.png  (da tam dung - xam va mo)"

# --- 4. Icon cho cửa sổ cài đặt ---------------------------------------------
$pub = Join-Path $root "ui\public"
if (-not (Test-Path $pub)) { New-Item -ItemType Directory -Path $pub | Out-Null }
Copy-Item (Join-Path $iconDir "icon.png") (Join-Path $pub "icon.png") -Force
Write-Output ""
Write-Output "Da sao icon.png sang ui\public\ cho cua so cai dat"

$square.Dispose()
Remove-Item $squarePath -Force -ErrorAction SilentlyContinue

Write-Output ""
Get-ChildItem $iconDir | ForEach-Object { "  {0,-20} {1,8:N0} byte" -f $_.Name, $_.Length }
