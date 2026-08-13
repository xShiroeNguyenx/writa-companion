//! P0 / P4 — Tích hợp Windows.
//!
//! # Vì sao đây là code sản xuất chứ không phải spike vứt đi
//!
//! PLAN.md xếp P0 là "spike throwaway". Nhưng bốn trong sáu spike (đọc selection,
//! ghi text, định vị caret, ngữ cảnh app) chính là **những hàm P2 và P4 sẽ gọi**.
//! Viết chúng thành thư viện ngay từ đầu tốn không hơn viết spike, mà tránh được
//! việc viết lại lần hai — và quan trọng hơn: bảng compatibility matrix của P0 đo
//! trên đúng code sẽ chạy thật, không phải trên một bản nháp gần giống.
//!
//! Binary `win-probe` là phần "spike": nó gọi thư viện này và in ra bảng kết quả
//! cho SPIKE_RESULTS.md.
//!
//! # Nguyên tắc xuyên suốt
//!
//! Không API nào ở đây chạy được ở mọi app. Mỗi module vì thế là một **chuỗi
//! fallback** có thứ tự, và luôn kết thúc bằng một phương án chấp nhận được thay vì
//! báo lỗi. Cái giá của việc thất bại phải là "kém chính xác hơn", không bao giờ là
//! "hỏng".

pub mod buffer;
pub mod caret;
pub mod context;
pub mod hook;
pub mod overlay;
pub mod selection;
pub mod writer;

use std::fmt;

/// Lỗi tích hợp Windows.
///
/// Mọi biến thể đều là *thông tin*, không phải thảm hoạ: lớp gọi luôn có đường lùi.
#[derive(Debug, Clone)]
pub enum WinError {
    /// Không có cửa sổ nào đang focus.
    NoForegroundWindow,
    /// API có tồn tại nhưng app đích không hỗ trợ (thường gặp với UIA).
    NotSupported(&'static str),
    /// Gọi Win32 thất bại.
    Api { call: &'static str, code: i32 },
    /// Không có gì để đọc — user chưa bôi đen.
    NothingSelected,
}

impl fmt::Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WinError::NoForegroundWindow => write!(f, "không có cửa sổ đang focus"),
            WinError::NotSupported(what) => write!(f, "app không hỗ trợ {what}"),
            WinError::Api { call, code } => write!(f, "{call} lỗi (mã {code})"),
            WinError::NothingSelected => write!(f, "chưa bôi đen đoạn nào"),
        }
    }
}

impl std::error::Error for WinError {}

pub type WinResult<T> = Result<T, WinError>;

/// Chuyển buffer UTF-16 kết thúc bằng NUL thành `String`.
pub(crate) fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
