# Ký Writa bằng chứng chỉ tự ký.
#
# ## Chứng chỉ tự ký được gì, và KHÔNG được gì
#
# Được:
#   - Chứng minh file do đúng máy này dựng ra và **chưa bị sửa** sau đó. Ai có bản
#     Writa cũng kiểm được bằng `Get-AuthenticodeSignature`.
#   - Trên máy đã cài chứng chỉ vào Trusted Root, Windows coi nhà phát hành là hợp lệ
#     và không còn hỏi "Unknown publisher".
#   - Dựng sẵn đường ống ký, để sau này đổi sang chứng chỉ thương mại chỉ là đổi một
#     dấu vân tay.
#
# KHÔNG được:
#   - **Không** làm SmartScreen im lặng. SmartScreen xét uy tín theo lượt tải, và một
#     chứng chỉ không thuộc CA được tin cậy thì không có uy tín nào cả. Máy người lạ
#     vẫn hiện "Windows protected your PC".
#   - Không thay thế được chứng chỉ OV khi phát hành công khai.
#
# ## Cách dùng
#
#   .\scripts\sign.ps1                # tạo cert nếu chưa có, rồi ký exe + installer
#   .\scripts\sign.ps1 -ExportCert    # xuất thêm .cer để cài lên máy khác
#   .\scripts\sign.ps1 -Thumbprint    # chỉ in dấu vân tay (cho tauri.conf.json)
#
# Chứng chỉ nằm ở `Cert:\CurrentUser\My` — không cần quyền admin, và chỉ ảnh hưởng
# tài khoản đang đăng nhập.

param(
    [string]$Subject = "Shiroe Nguyễn",
    [switch]$ExportCert,
    [switch]$ExportPfx,
    [switch]$Thumbprint
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

function Get-WritaCert {
    Get-ChildItem Cert:\CurrentUser\My |
        Where-Object { $_.Subject -eq "CN=$Subject" -and $_.NotAfter -gt (Get-Date) } |
        Sort-Object NotAfter -Descending |
        Select-Object -First 1
}

$cert = Get-WritaCert
if (-not $cert) {
    Write-Output "Chua co chung chi cho '$Subject' - dang tao..."
    # Splat thay vi noi dong bang backtick: backtick + CRLF khong phai lien tuc dong
    # dang tin cay khi file duoc ghi tu cong cu khac.
    $params = @{
        Type               = "CodeSigningCert"
        Subject            = "CN=$Subject"
        KeyUsage           = "DigitalSignature"
        KeyAlgorithm       = "RSA"
        KeyLength          = 3072
        HashAlgorithm      = "SHA256"
        CertStoreLocation  = "Cert:\CurrentUser\My"
        NotAfter           = (Get-Date).AddYears(5)
    }
    $cert = New-SelfSignedCertificate @params
    Write-Output "  da tao: $($cert.Thumbprint)"
} else {
    Write-Output "Dung chung chi san co: $($cert.Thumbprint)"
}
Write-Output "  Subject : $($cert.Subject)"
Write-Output "  Hết hạn : $($cert.NotAfter.ToString('yyyy-MM-dd'))"

if ($Thumbprint) {
    Write-Output ""
    Write-Output "Dán vào src-tauri/tauri.conf.json → bundle.windows.certificateThumbprint:"
    Write-Output "  `"$($cert.Thumbprint)`""
    exit 0
}

if ($ExportPfx) {
    # `.pfx` chứa CẢ KHOÁ RIÊNG. Đây là thứ để dán vào GitHub secret
    # `WINDOWS_CERT_PFX_BASE64`, không phải thứ để gửi cho ai khác.
    $pw = Read-Host "Mat khau bao ve file .pfx" -AsSecureString
    $pfx = Join-Path $root "writa-signing.pfx"
    Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pw | Out-Null
    $b64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($pfx))
    $b64Path = Join-Path $root "writa-signing.pfx.base64.txt"
    Set-Content -Path $b64Path -Value $b64 -Encoding ascii
    Write-Output ""
    Write-Output "Da xuat: $pfx"
    Write-Output "Base64 : $b64Path"
    Write-Output ""
    Write-Output "Dan noi dung file base64 vao GitHub secret WINDOWS_CERT_PFX_BASE64,"
    Write-Output "va mat khau vua nhap vao WINDOWS_CERT_PASSWORD. Sau do XOA hai file nay."
    exit 0
}

if ($ExportCert) {
    $cer = Join-Path $root "writa-signing.cer"
    Export-Certificate -Cert $cert -FilePath $cer -Force | Out-Null
    Write-Output ""
    Write-Output "Đã xuất: $cer"
    Write-Output "Trên máy khác, cài bằng (cần quyền admin):"
    Write-Output "  Import-Certificate -FilePath writa-signing.cer -CertStoreLocation Cert:\LocalMachine\Root"
}

# --- Tìm signtool -----------------------------------------------------------
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "x64" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName

if (-not $signtool) {
    Write-Output ""
    Write-Output "KHÔNG tìm thấy signtool.exe (cần Windows SDK)."
    Write-Output "Dùng Set-AuthenticodeSignature thay thế — không đóng dấu thời gian được."
}

# --- Ký ---------------------------------------------------------------------
$targets = @(
    (Join-Path $root "target\release\writa-app.exe"),
    (Join-Path $root "target\release\bundle\nsis\Writa_0.1.0_x64-setup.exe")
) | Where-Object { Test-Path $_ }

if (-not $targets) {
    Write-Output ""
    Write-Output 'Chua co file nao de ky. Chay "npm run app:build" truoc.'
    exit 1
}

Write-Output ""
foreach ($t in $targets) {
    if ($signtool) {
        # /fd SHA256 = thuật toán băm file. /tr = máy chủ đóng dấu thời gian, để chữ
        # ký còn giá trị sau khi chứng chỉ hết hạn.
        & $signtool sign /sha1 $cert.Thumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $t 2>&1 |
            Select-Object -Last 2 | ForEach-Object { "  $_" }
    } else {
        Set-AuthenticodeSignature -FilePath $t -Certificate $cert -HashAlgorithm SHA256 | Out-Null
    }
    $s = Get-AuthenticodeSignature $t
    Write-Output ("  {0,-30} {1}  [{2}]" -f (Split-Path $t -Leaf), $s.Status, $s.SignerCertificate.Subject)
}

Write-Output ""
Write-Output "Nhắc lại: chữ ký này KHÔNG làm SmartScreen im lặng trên máy người khác."
