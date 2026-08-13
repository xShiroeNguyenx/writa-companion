# Đổi số version của Writa ở mọi chỗ đang giữ nó.
#
#   .\scripts\bump-version.ps1 0.1.1
#
# ## Vì sao việc này cần một script chứ không phải sửa tay
#
# Số version nằm ở **bốn** file, và tính năng tự cập nhật so sánh version trong
# `tauri.conf.json` với version trong `latest.json` để quyết định có bản mới hay không.
# Sửa thiếu một chỗ thì hậu quả là bản phát hành mới bị chính nó coi là "đang mới nhất",
# và không có thông báo lỗi nào — chỉ là im lặng. Đó là loại lỗi chỉ phát hiện được bằng
# cách cài thử rồi ngồi đợi.
#
# Tag git cũng phải khớp: `v0.1.1` ứng với version `0.1.1`. `tauri-action` dùng tag để
# đặt tên bản phát hành, nhưng dùng `tauri.conf.json` để đặt tên file — lệch nhau thì
# `latest.json` trỏ vào một file không tồn tại.

param(
    [Parameter(Mandatory = $true)][string]$Version
)

$ErrorActionPreference = "Stop"

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version phai dang x.y.z, nhan duoc: $Version"
}

$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $cur = (Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
    Write-Output "$cur  ->  $Version"
    Write-Output ""

    # package.json + package-lock.json — npm sửa cả hai, đúng định dạng của nó.
    & npm version $Version --no-git-tag-version --allow-same-version | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "npm version that bai" }
    Write-Output "  package.json"
    Write-Output "  package-lock.json"

    # Cargo.toml — chỉ có đúng một dòng `version = ` ở đầu dòng, trong [workspace.package].
    # Các crate con đều dùng `version.workspace = true` nên không phải sửa.
    # So khớp trước rồi mới thay. Đừng dùng "nội dung có đổi không" làm bằng chứng đã
    # tìm thấy: bump về đúng số cũ là chuyện hợp lệ (dựng lại một version), và khi đó
    # nội dung không đổi dù regex khớp hoàn hảo.
    function Set-Version {
        param([string]$Path, [string]$Pattern, [string]$Replacement)
        $full = Join-Path $root $Path
        $text = [System.IO.File]::ReadAllText($full)
        if (-not [regex]::IsMatch($text, $Pattern)) {
            throw "Khong tim thay dong version trong $Path"
        }
        [System.IO.File]::WriteAllText($full, [regex]::Replace($text, $Pattern, $Replacement, 1))
        Write-Output "  $Path"
    }

    # Các crate con đều dùng `version.workspace = true` nên chỉ [workspace.package] cần sửa.
    Set-Version "Cargo.toml" '(?m)^version = "[^"]+"' "version = `"$Version`""
    Set-Version "src-tauri\tauri.conf.json" '"version": "[^"]+"' "`"version`": `"$Version`""

    # Cargo.lock ghi version của từng crate trong workspace. Không cập nhật thì lần dựng
    # sau cargo tự sửa, nhưng khi đó file lock lệch với commit — nên làm luôn ở đây.
    & cargo update --workspace --quiet
    if ($LASTEXITCODE -ne 0) { throw "cargo update that bai" }
    Write-Output "  Cargo.lock"

    Write-Output ""
    Write-Output "Kiem tra lai:"
    Select-String -Path "package.json", "Cargo.toml", "src-tauri\tauri.conf.json" `
        -Pattern "^\s*`"?version`"?\s*[:=]" | ForEach-Object {
        "  {0,-28} {1}" -f $_.Filename, $_.Line.Trim()
    }
    Write-Output ""
    Write-Output "Tiep: commit, roi `git tag v$Version` va push tag."
} finally {
    Pop-Location
}
