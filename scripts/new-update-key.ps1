# Sinh khoá ký gói cập nhật, và cập nhật luôn khoá công khai trong tauri.conf.json.
#
# ## Vì sao hai việc này phải đi cùng nhau
#
# Cặp khoá gồm hai nửa đi đôi: khoá riêng ký gói lúc phát hành, khoá công khai nhúng
# trong app để kiểm chữ ký. Sinh khoá mới mà quên thay khoá công khai thì app sẽ **từ
# chối mọi bản cập nhật** — và nó thất bại đúng cách khó thấy nhất: im lặng, ở máy user,
# nhiều tháng sau.
#
# ## Mật khẩu
#
# Script không nhận mật khẩu qua tham số dòng lệnh, và đó là chủ ý: tham số dòng lệnh
# nằm lại trong lịch sử shell và trong danh sách tiến trình.
#
# ## Sau khi chạy
#
#   1. Dán nội dung `.keys/writa-update.key` vào GitHub secret TAURI_SIGNING_PRIVATE_KEY
#   2. Dán mật khẩu vừa nhập vào TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#   3. Sao lưu `.keys/` ra chỗ an toàn — mất nó là mất khả năng phát hành bản mới cho
#      những ai đã cài, vĩnh viễn. Không có đường khôi phục.

param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$keyDir = Join-Path $root ".keys"
$keyPath = Join-Path $keyDir "writa-update.key"
$confPath = Join-Path $root "src-tauri\tauri.conf.json"

if ((Test-Path $keyPath) -and -not $Force) {
    Write-Output "Da co khoa o $keyPath"
    Write-Output ""
    Write-Output "Sinh khoa MOI se lam moi ban cai dat hien co khong nhan duoc cap nhat nua,"
    Write-Output "vi chung mang khoa cong khai cu. Chi lam khi khoa cu bi lo, hoac khi chua"
    Write-Output "phat hanh ban nao."
    Write-Output ""
    Write-Output "Chac chan thi chay lai voi:  .\scripts\new-update-key.ps1 -Force"
    exit 1
}

if (-not (Test-Path $keyDir)) { New-Item -ItemType Directory -Path $keyDir | Out-Null }

$pw = Read-Host "Mat khau bao ve khoa rieng (nho ky, khong khoi phuc duoc)" -AsSecureString
$plain = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
    [Runtime.InteropServices.Marshal]::SecureStringToBSTR($pw))

Write-Output ""
Write-Output "Dang sinh cap khoa..."
Push-Location $root
try {
    $env:CI = "true"
    & npx --yes @tauri-apps/cli signer generate -w ".keys/writa-update.key" --ci -p $plain -f 2>&1 |
        Select-String -Pattern "Private|Public" | ForEach-Object { "  " + $_.Line.Trim() }
} finally {
    Pop-Location
}

$pubPath = "$keyPath.pub"
if (-not (Test-Path $pubPath)) { throw "Khong sinh duoc khoa cong khai" }

# Khoá công khai `.pub` có dòng comment ở đầu; `tauri.conf.json` cần **cả file** ở dạng
# base64 một dòng, đúng như `signer generate` in ra.
$pub = (Get-Content $pubPath -Raw).Trim()

$conf = Get-Content $confPath -Raw -Encoding UTF8
$json = $conf | ConvertFrom-Json
$old = $json.plugins.updater.pubkey
if ($old -eq $pub) {
    Write-Output "  khoa cong khai khong doi"
} else {
    # Thay bằng chuỗi thay vì ghi lại cả JSON: `ConvertTo-Json` cua PowerShell 5.1 doi
    # thu tu khoa va thoat ky tu Unicode, lam file kho doc va diff day nhieu.
    $conf = $conf.Replace($old, $pub)
    [System.IO.File]::WriteAllText($confPath, $conf, [System.Text.UTF8Encoding]::new($false))
    Write-Output "  da cap nhat pubkey trong tauri.conf.json"
}

Write-Output ""
Write-Output "=============================================================="
Write-Output "Buoc tiep theo - dat secret tren GitHub"
Write-Output "=============================================================="
Write-Output ""
Write-Output "1. Chep khoa rieng vao clipboard:"
Write-Output "     Get-Content .keys\writa-update.key -Raw | Set-Clipboard"
Write-Output "   Dan vao secret:  TAURI_SIGNING_PRIVATE_KEY"
Write-Output ""
Write-Output "2. Mat khau vua nhap -> secret:  TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
Write-Output ""
Write-Output "3. SAO LUU thu muc .keys\ ra cho an toan (khong phai trong repo)."
Write-Output "   Mat no la mat kha nang phat hanh ban moi cho nguoi da cai. Vinh vien."
