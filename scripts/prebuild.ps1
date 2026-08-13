# Dừng Writa đang chạy trước khi dựng bản mới.
#
# ## Vì sao cần
#
# Windows khoá file thực thi đang chạy, nên `cargo build` không ghi đè được và báo
# `failed to remove file ... Access is denied (os error 5)` — một thông báo không hề nhắc
# tới nguyên nhân thật.
#
# Với hầu hết dự án thì đây là chuyện hiếm. Với Writa thì gần như chắc chắn: nó được
# thiết kế để chạy nền cả ngày ở khay hệ thống, nên trạng thái *bình thường* của máy phát
# triển là "app đang chạy".
#
# Script chỉ dừng đúng tiến trình của dự án này, không đụng gì khác.

$ErrorActionPreference = "SilentlyContinue"

$running = Get-Process | Where-Object { $_.ProcessName -eq "writa-app" }
if ($running) {
    foreach ($p in $running) {
        Write-Output "  dung writa-app (pid $($p.Id)) de mo khoa file thuc thi"
    }
    $running | Stop-Process -Force
    # Windows nha khoa file khong hoan toan dong bo voi luc tien trinh chet.
    Start-Sleep -Milliseconds 800
}

exit 0
