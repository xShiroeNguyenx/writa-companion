//! Cửa sổ nổi **không bao giờ lấy focus**.
//!
//! # Vì sao Tier 2 bắt buộc cần cái này
//!
//! Popup của Tier 1 lấy focus được, và điều đó tốt: user đã bôi đen xong, việc tiếp
//! theo của họ là đọc và chọn. Tier 2 thì ngược hẳn — user **đang gõ dở**. Một cửa
//! sổ nhảy lên cướp focus giữa lúc gõ sẽ làm mất caret ở app đích, và tính năng chết
//! ngay tại đó: gõ tiếp thì chữ rơi vào cửa sổ của Writa.
//!
//! Hai mảnh, cả hai đều cần, thiếu một là hỏng:
//!
//! - `WS_EX_NOACTIVATE` — cửa sổ không nhận activation khi được hiện hay khi bị bấm.
//! - `SW_SHOWNOACTIVATE` — riêng lệnh hiện cũng không được activate. `ShowWindow(SW_SHOW)`
//!   thông thường vẫn activate dù có cờ trên, nên phải đi đường riêng.
//!
//! Hệ quả: overlay **không nhận được phím**. Mọi thao tác của Tier 2 vì thế đi qua
//! [`crate::hook`] chứ không qua bàn phím của cửa sổ này.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use crate::context::WindowId;

fn hwnd(window: WindowId) -> HWND {
    HWND(window as *mut std::ffi::c_void)
}

/// Biến một cửa sổ thành overlay: không lấy focus, luôn trên cùng, không vào taskbar
/// và không vào Alt+Tab.
///
/// Gọi **một lần** sau khi cửa sổ được tạo, trước lần hiện đầu tiên.
pub fn make_non_activating(window: WindowId) {
    let h = hwnd(window);
    unsafe {
        let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
        // `WS_EX_TOOLWINDOW` là thứ giữ overlay khỏi Alt+Tab. Không có nó, một bong
        // bóng gợi ý rộng 200px sẽ nằm chình ình giữa danh sách chuyển cửa sổ.
        let wanted = (WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(h, GWL_EXSTYLE, style | wanted);

        // Đổi ex-style xong phải gọi SetWindowPos để Windows áp dụng lại frame.
        let _ = SetWindowPos(
            h,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Hiện overlay mà **không** activate.
///
/// Không dùng `Window::show()` của Tauri ở đây: nó gọi `ShowWindow(SW_SHOW)`, và
/// `SW_SHOW` activate cửa sổ bất kể `WS_EX_NOACTIVATE`.
pub fn show_no_activate(window: WindowId) {
    unsafe {
        let _ = ShowWindow(hwnd(window), SW_SHOWNOACTIVATE);
    }
}

pub fn hide(window: WindowId) {
    unsafe {
        let _ = ShowWindow(hwnd(window), SW_HIDE);
    }
}

/// Đặt overlay ở toạ độ màn hình, không activate, không đổi kích thước.
pub fn move_to(window: WindowId, x: i32, y: i32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(window),
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
