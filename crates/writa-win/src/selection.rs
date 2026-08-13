//! Đọc vùng chọn qua UI Automation, và đường lùi bằng clipboard.
//!
//! # Hai đường, và vì sao cần cả hai
//!
//! **UIA** là đường sạch: đọc trực tiếp, không đụng clipboard của user, không sinh
//! ra sự kiện bàn phím nào. Nhưng nó phụ thuộc app đích có cài đặt `TextPattern`
//! hay không — nhiều app không.
//!
//! **Clipboard** là đường lùi hoạt động ở gần như mọi nơi (bất cứ đâu Ctrl+C chạy),
//! nhưng nó mượn clipboard của user, nên phải trả lại nguyên trạng, và nó sinh ra
//! sự kiện bàn phím thật.
//!
//! Thứ tự bắt buộc là UIA trước. Đảo lại thì mỗi lần kiểm tra chính tả đều giẫm lên
//! clipboard của user, kể cả khi hoàn toàn không cần.
//!
//! # `IsPassword` là chốt chặn cuối
//!
//! [`crate::context`] bắt được ô mật khẩu kiểu Win32, nhưng Chrome và Electron tự
//! vẽ ô nhập nên không lộ `ES_PASSWORD`. UIA `IsPassword` là thứ duy nhất thấy được
//! chúng, nên nó phải được hỏi **trước mọi lần đọc** — kể cả khi lớp Win32 đã bảo
//! là an toàn.

use std::cell::Cell;

use windows::core::{Interface, BOOL};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
// Các hàm SafeArray* nằm ở System::Ole, không phải System::Com — dù kiểu SAFEARRAY
// thì khai báo ở cả hai.
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayGetUBound, SafeArrayUnaccessData};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextPattern2, UIA_TextPattern2Id, UIA_TextPatternId,
};

use crate::caret::{CaretPos, CaretSource};
use crate::{writer, WinError, WinResult};

thread_local! {
    /// COM chỉ được khởi tạo một lần cho mỗi thread.
    static COM_READY: Cell<bool> = const { Cell::new(false) };
}

/// Khởi tạo COM cho thread hiện tại, một lần.
///
/// Dùng apartment-threaded vì UIA là mô hình STA và ta gọi từ thread UI.
/// `RPC_E_CHANGED_MODE` nghĩa là thread đã khởi tạo ở chế độ khác — không phải lỗi
/// của ta, cứ dùng tiếp chế độ đang có.
fn ensure_com() {
    COM_READY.with(|ready| {
        if !ready.get() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            ready.set(true);
        }
    });
}

fn automation() -> WinResult<IUIAutomation> {
    ensure_com();
    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.map_err(|e| {
        WinError::Api {
            call: "CoCreateInstance(CUIAutomation)",
            code: e.code().0,
        }
    })
}

fn focused_element() -> WinResult<IUIAutomationElement> {
    let a = automation()?;
    unsafe { a.GetFocusedElement() }.map_err(|e| WinError::Api {
        call: "GetFocusedElement",
        code: e.code().0,
    })
}

/// Phần tử đang focus có phải ô mật khẩu không, theo UIA.
///
/// Đây là lớp nhận diện duy nhất thấy được ô mật khẩu của Chrome, Edge và Electron.
/// Khi không xác định được thì trả `true` — nghi ngờ thì coi là mật khẩu.
pub fn is_password_element() -> bool {
    match focused_element() {
        Ok(el) => unsafe { el.CurrentIsPassword() }
            .map(|b| b.as_bool())
            .unwrap_or(true),
        // Không hỏi được UIA thì không kết luận được là an toàn.
        Err(_) => true,
    }
}

/// Đọc vùng chọn qua UIA. Không đụng vào clipboard.
pub fn read_selection_uia() -> WinResult<String> {
    let el = focused_element()?;

    let pattern: IUIAutomationTextPattern = unsafe { el.GetCurrentPattern(UIA_TextPatternId) }
        .ok()
        .and_then(|p| p.cast().ok())
        .ok_or(WinError::NotSupported("UIA TextPattern"))?;

    let ranges = unsafe { pattern.GetSelection() }.map_err(|e| WinError::Api {
        call: "TextPattern::GetSelection",
        code: e.code().0,
    })?;

    let count = unsafe { ranges.Length() }.unwrap_or(0);
    if count == 0 {
        return Err(WinError::NothingSelected);
    }

    // Vùng chọn rời rạc (bảng, cột) trả về nhiều range. Nối lại bằng dấu cách —
    // đúng hơn là chỉ lấy range đầu và im lặng bỏ phần còn lại.
    let mut out = String::new();
    for i in 0..count {
        let Ok(range) = (unsafe { ranges.GetElement(i) }) else {
            continue;
        };
        // -1 = lấy toàn bộ range, không giới hạn độ dài.
        if let Ok(text) = unsafe { range.GetText(-1) } {
            let s = text.to_string();
            if !s.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&s);
            }
        }
    }

    if out.trim().is_empty() {
        Err(WinError::NothingSelected)
    } else {
        Ok(out)
    }
}

/// Đọc vùng chọn qua clipboard: Ctrl+C, đọc, rồi **trả clipboard về nguyên trạng**.
pub fn read_selection_clipboard() -> WinResult<String> {
    let saved = writer::clipboard_get_text();

    // Xoá clipboard trước để phân biệt "user không chọn gì" với "user chọn đúng
    // đoạn đang có sẵn trong clipboard".
    let _ = writer::clipboard_set_text("");
    writer::send_copy(Default::default())?;

    // App đích xử lý Ctrl+C bất đồng bộ. Poll ngắn thay vì ngủ một phát dài —
    // app nhanh trả lời sau vài mili giây, chờ cứng làm chậm mọi trường hợp.
    let mut copied = None;
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(15));
        if let Some(t) = writer::clipboard_get_text() {
            if !t.is_empty() {
                copied = Some(t);
                break;
            }
        }
    }

    if let Some(old) = saved {
        let _ = writer::clipboard_set_text(&old);
    }

    copied.ok_or(WinError::NothingSelected)
}

/// Đọc vùng chọn: UIA trước, clipboard sau.
///
/// Kiểm tra `IsPassword` trước khi đọc bất cứ thứ gì — kể cả khi lớp Win32 ở
/// [`crate::context`] đã bảo là an toàn.
pub fn read_selection() -> WinResult<String> {
    if is_password_element() {
        return Err(WinError::NotSupported("ô mật khẩu"));
    }
    match read_selection_uia() {
        Ok(text) => Ok(text),
        Err(_) => read_selection_clipboard(),
    }
}

/// Bậc 2 và 3 của chuỗi định vị caret — xem [`crate::caret`].
pub fn caret_via_uia() -> Option<CaretPos> {
    let el = focused_element().ok()?;

    // Bậc 2 — TextPattern2::GetCaretRange, chính xác nhất trong hai bậc UIA.
    if let Some(p2) = unsafe { el.GetCurrentPattern(UIA_TextPattern2Id) }
        .ok()
        .and_then(|p| p.cast::<IUIAutomationTextPattern2>().ok())
    {
        let mut active = BOOL::default();
        if let Ok(range) = unsafe { p2.GetCaretRange(&mut active) } {
            if let Some(pos) = first_rect(unsafe { range.GetBoundingRectangles() }.ok()) {
                return Some(CaretPos {
                    source: CaretSource::UiaCaretRange,
                    ..pos
                });
            }
        }
    }

    // Bậc 3 — hình chữ nhật của vùng chọn. Với caret rỗng (không bôi đen), nhiều
    // provider vẫn trả về một range suy biến ngay tại vị trí con trỏ.
    let pattern: IUIAutomationTextPattern = unsafe { el.GetCurrentPattern(UIA_TextPatternId) }
        .ok()
        .and_then(|p| p.cast().ok())?;
    let ranges = unsafe { pattern.GetSelection() }.ok()?;
    if unsafe { ranges.Length() }.unwrap_or(0) == 0 {
        return None;
    }
    let range = unsafe { ranges.GetElement(0) }.ok()?;
    first_rect(unsafe { range.GetBoundingRectangles() }.ok()).map(|pos| CaretPos {
        source: CaretSource::UiaSelection,
        ..pos
    })
}

/// Lấy hình chữ nhật đầu tiên từ SAFEARRAY mà UIA trả về.
///
/// Bố cục là `[left, top, width, height]` lặp lại — mỗi dòng text một bộ bốn. Ta
/// lấy bộ đầu vì đó là nơi caret đang đứng.
fn first_rect(array: Option<*mut windows::Win32::System::Com::SAFEARRAY>) -> Option<CaretPos> {
    let array = array?;
    if array.is_null() {
        return None;
    }
    unsafe {
        let upper = SafeArrayGetUBound(array, 1).ok()?;
        if upper < 3 {
            return None; // cần đủ bốn phần tử cho một hình chữ nhật
        }
        let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
        SafeArrayAccessData(array, &mut data).ok()?;
        let values = std::slice::from_raw_parts(data as *const f64, 4);
        let pos = CaretPos {
            x: values[0] as i32,
            y: values[1] as i32,
            height: (values[3] as i32).max(1),
            source: CaretSource::UiaCaretRange,
        };
        let _ = SafeArrayUnaccessData(array);
        Some(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com_init_is_idempotent() {
        // Gọi nhiều lần trên cùng thread phải an toàn — mỗi lần đọc selection đều
        // đi qua đây.
        ensure_com();
        ensure_com();
        ensure_com();
    }

    #[test]
    fn null_safearray_yields_no_position() {
        assert!(first_rect(None).is_none());
        assert!(first_rect(Some(std::ptr::null_mut())).is_none());
    }
}
