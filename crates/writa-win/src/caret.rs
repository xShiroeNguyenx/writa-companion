//! Định vị caret — chuỗi bốn phương án.
//!
//! Không API Windows nào lấy được vị trí con trỏ text ở mọi app, nên đây là chuỗi
//! fallback có thứ tự. Mỗi bậc phủ một họ app khác nhau, và **bậc cuối luôn thành
//! công** — overlay hiện lệch chỗ vẫn dùng được, overlay không hiện thì không.
//!
//! | Bậc | Cách | Phủ |
//! |---|---|---|
//! | 1 | `GetGUIThreadInfo` → `rcCaret` | Control Edit chuẩn Win32: Notepad, WordPad, hộp thoại |
//! | 2 | UIA `TextPattern2::GetCaretRange` | Chrome, Edge, Electron, app modern |
//! | 3 | UIA `TextPattern::GetSelection` | App có UIA nhưng thiếu TextPattern2 |
//! | 4 | Vị trí chuột | Mọi thứ còn lại |
//!
//! Bậc 1 đứng trước vì nó rẻ nhất (không COM, không cross-process marshalling) và
//! chính xác nhất khi có. Bậc 2–3 nằm ở [`crate::selection`] vì dùng chung hạ tầng
//! COM/UIA.

use windows::Win32::Foundation::POINT;
// ClientToScreen sống ở Graphics::Gdi chứ không phải WindowsAndMessaging — nó là
// phép đổi hệ toạ độ, không phải API cửa sổ.
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

use crate::context::AppContext;
use crate::selection;

/// Vị trí caret theo toạ độ màn hình, kèm nguồn tìm ra nó.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretPos {
    /// Mép trái của caret.
    pub x: i32,
    /// Mép **trên** của caret. Overlay thường muốn neo dưới caret, nên cộng thêm
    /// [`CaretPos::height`].
    pub y: i32,
    /// Chiều cao caret — dùng để đặt overlay ngay dưới dòng đang gõ.
    pub height: i32,
    pub source: CaretSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretSource {
    /// Bậc 1 — chính xác nhất, rẻ nhất.
    GuiThreadInfo,
    /// Bậc 2 — UIA `TextPattern2`.
    UiaCaretRange,
    /// Bậc 3 — UIA vùng chọn.
    UiaSelection,
    /// Bậc 4 — không lấy được caret, neo theo chuột.
    MousePosition,
}

impl CaretSource {
    /// Vị trí này có thật sự là caret không, hay chỉ là phỏng đoán?
    ///
    /// UI dùng để quyết định: caret thật thì neo overlay sát dòng đang gõ; phỏng
    /// đoán thì nên lùi ra một chút để không che mất text.
    pub fn is_exact(self) -> bool {
        !matches!(self, CaretSource::MousePosition)
    }
}

/// Định vị caret của app đang focus. **Không bao giờ thất bại** — bậc cuối là vị
/// trí chuột.
pub fn locate(ctx: &AppContext) -> CaretPos {
    if let Some(p) = from_gui_thread_info(ctx) {
        return p;
    }
    if let Some(p) = selection::caret_via_uia() {
        return p;
    }
    from_mouse()
}

/// Bậc 1 — `GetGUIThreadInfo`.
fn from_gui_thread_info(ctx: &AppContext) -> Option<CaretPos> {
    let tid = unsafe { GetWindowThreadProcessId(ctx.window, None) };
    if tid == 0 {
        return None;
    }

    let mut gui = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(tid, &mut gui) }.ok()?;

    // Cửa sổ không có caret, hoặc caret bị thu về rỗng (nhiều app làm vậy khi mất
    // focus). Cả hai trường hợp đều là "không dùng được", không phải lỗi.
    if gui.hwndCaret.0.is_null() {
        return None;
    }
    let r = gui.rcCaret;
    if r.right <= r.left && r.bottom <= r.top {
        return None;
    }

    // rcCaret theo toạ độ CLIENT của cửa sổ có caret, phải đổi sang toạ độ màn hình.
    let mut pt = POINT {
        x: r.left,
        y: r.top,
    };
    if !unsafe { ClientToScreen(gui.hwndCaret, &mut pt) }.as_bool() {
        return None;
    }

    Some(CaretPos {
        x: pt.x,
        y: pt.y,
        height: (r.bottom - r.top).max(1),
        source: CaretSource::GuiThreadInfo,
    })
}

/// Bậc 4 — vị trí chuột. Luôn có.
fn from_mouse() -> CaretPos {
    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    CaretPos {
        x: pt.x,
        // Lùi xuống một chút để overlay không nằm ngay dưới mũi chuột.
        y: pt.y + 20,
        height: 20,
        source: CaretSource::MousePosition,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mouse_fallback_is_inexact() {
        assert!(CaretSource::GuiThreadInfo.is_exact());
        assert!(CaretSource::UiaCaretRange.is_exact());
        assert!(CaretSource::UiaSelection.is_exact());
        assert!(!CaretSource::MousePosition.is_exact());
    }

    #[test]
    fn mouse_fallback_always_returns_something() {
        // Bất biến của cả module: định vị caret không bao giờ thất bại. Overlay
        // lệch chỗ vẫn dùng được; overlay không hiện thì không.
        let p = from_mouse();
        assert_eq!(p.source, CaretSource::MousePosition);
        assert!(p.height > 0);
    }
}
