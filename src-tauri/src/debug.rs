//! Nhật ký chẩn đoán, mặc định TẮT.
//!
//! Bật bằng biến môi trường `WRITA_DEBUG=1`, ghi vào `%TEMP%\writa-debug.log`.
//!
//! # Vì sao cần
//!
//! Luồng Tier 1 đi qua bốn tầng chạy ở bốn nơi khác nhau — thread phím tắt, Win32,
//! lệnh Tauri, và JavaScript trong WebView. Bản release không có console, và
//! WebView không có devtools. Khi user báo "không có gì xảy ra" thì đó là bốn khả
//! năng không phân biệt được, và đoán từng cái một là cách tốn thời gian nhất.
//!
//! # Vì sao KHÔNG bật mặc định
//!
//! File này ghi tên app đang focus và độ dài đoạn text. Đó vẫn là thông tin về thứ
//! user đang gõ, dù không phải nội dung. Một công cụ đọc được mọi ô nhập trên máy
//! thì không được để lại dấu vết nào trên đĩa nếu user không chủ động bật.

use std::fmt::Arguments;
use std::io::Write;
use std::sync::OnceLock;

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("WRITA_DEBUG").is_some())
}

fn path() -> std::path::PathBuf {
    std::env::temp_dir().join("writa-debug.log")
}

pub fn log(args: Arguments<'_>) {
    if !enabled() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path())
    {
        let _ = writeln!(f, "{args}");
    }
}

/// Ghi một dòng chẩn đoán. Không làm gì khi `WRITA_DEBUG` chưa đặt.
macro_rules! dbg_log {
    ($($arg:tt)*) => { $crate::debug::log(format_args!($($arg)*)) };
}

pub(crate) use dbg_log;
