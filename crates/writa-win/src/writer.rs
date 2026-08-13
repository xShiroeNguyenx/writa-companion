//! Ghi text vào app đang focus.
//!
//! # Vì sao `KEYEVENTF_UNICODE` chứ không phải `WM_CHAR`
//!
//! `SendInput` với cờ `KEYEVENTF_UNICODE` đưa ký tự vào **hàng đợi input của hệ
//! thống**, nên app đích xử lý nó y hệt như user gõ: đúng thứ tự với các phím khác,
//! đúng undo stack, chạy được cả với control tự vẽ (Chrome, Electron, game).
//! `WM_CHAR` gửi thẳng vào một cửa sổ cụ thể nên phải biết đúng HWND, và nhiều
//! framework hiện đại bỏ qua nó.
//!
//! Với `KEYEVENTF_UNICODE`, `wScan` mang **UTF-16 code unit** và `wVk` phải bằng 0.
//! Tiếng Việt nằm trọn trong BMP nên mỗi chữ là một code unit — trừ emoji, vốn cần
//! cặp surrogate và ta gửi từng nửa một, đúng như Windows mong đợi.
//!
//! # Vì sao có cả đường clipboard
//!
//! `SendInput` gửi từng ký tự một. Với một câu dài, đó là hàng trăm sự kiện input,
//! và nếu user gõ chen vào giữa thì text đan xen lẫn nhau. Đường clipboard thay cả
//! đoạn trong một thao tác dán duy nhất — nhanh hơn và nguyên tử hơn. Cái giá là
//! phải mượn clipboard của user, nên ta **luôn trả lại nguyên trạng**.

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};

use crate::{WinError, WinResult};

/// Trên ngưỡng này thì dùng clipboard thay vì gõ từng ký tự.
///
/// Dưới ngưỡng, `SendInput` đơn giản hơn và không đụng vào clipboard của user.
const CLIPBOARD_THRESHOLD: usize = 24;

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                // wVk PHẢI bằng 0 với KEYEVENTF_UNICODE; ký tự đi trong wScan.
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> WinResult<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(WinError::Api {
            call: "SendInput",
            code: sent as i32,
        });
    }
    Ok(())
}

/// Mọi phím phụ có thể đang bị giữ khi ta bắt đầu bơm.
const MODIFIERS: [VIRTUAL_KEY; 11] = [
    VK_CONTROL,
    VK_LCONTROL,
    VK_RCONTROL,
    VK_MENU,
    VK_LMENU,
    VK_RMENU,
    VK_SHIFT,
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LWIN,
    VK_RWIN,
];

/// Phím này có đang được giữ không.
pub fn is_key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Nhấn giữ vài phím — **chỉ để probe dựng lại tình huống lỗi**.
///
/// Không dùng ở đường chạy thật: Writa không bao giờ cần giữ phím phụ hộ user. Nó tồn
/// tại để `hook-probe` kiểm được [`release_modifiers`] mà không phải nhờ người ngồi
/// giữ `Ctrl+Alt` bằng tay.
pub fn hold_for_test(keys: &[VIRTUAL_KEY]) {
    let inputs: Vec<INPUT> = keys.iter().map(|k| key_input(*k, false)).collect();
    let _ = send(&inputs);
}

/// Nhả các phím phụ user **đang giữ** trước khi ta bơm phím của mình.
///
/// # Vì sao bắt buộc
///
/// Phím tắt bắn ngay lúc user **nhấn** `Ctrl+Alt+Space`, và lúc đó Ctrl với Alt vẫn
/// còn đang giữ. `SendInput(VK_BACK)` gửi vào giữa trạng thái đó thì app đích nhận
/// được **`Ctrl+Alt+Backspace`**, không phải Backspace — hầu hết app bỏ qua tổ hợp
/// đó. Kết quả: phần xoá không xảy ra, chỉ còn phần gõ vào, và bản sửa **mọc thành
/// một từ mới** thay vì thay từ cũ.
///
/// Trường hợp tệ hơn: `paste_text` tự gửi `Ctrl+V`, mà Alt còn giữ thì thành
/// `Ctrl+Alt+V` — đúng phím tắt của chính Writa, tức là tự gọi lại mình.
///
/// Chỉ nhả những phím thật sự đang xuống. Không dựng lại sau: user sắp nhả tay ra,
/// và một keyup thừa thì vô hại, còn keydown dựng lại thì có thể kẹt phím nếu ta
/// chết giữa đường.
pub fn release_modifiers() {
    let mut inputs = Vec::new();
    for vk in MODIFIERS {
        if is_key_down(vk.0) {
            inputs.push(key_input(vk, true));
        }
    }
    if !inputs.is_empty() {
        let _ = send(&inputs);
        // Cho app đích kịp xử lý keyup trước khi phím thật của ta tới.
        sleep(Duration::from_millis(10));
    }
}

/// Gõ một chuỗi vào app đang focus, ký tự một.
pub fn type_text(text: &str) -> WinResult<()> {
    release_modifiers();
    let mut inputs = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        inputs.push(unicode_input(unit, false));
        inputs.push(unicode_input(unit, true));
    }
    send(&inputs)
}

/// Gửi `count` lần phím Backspace.
pub fn backspace(count: usize) -> WinResult<()> {
    release_modifiers();
    let mut inputs = Vec::with_capacity(count * 2);
    for _ in 0..count {
        inputs.push(key_input(VK_BACK, false));
        inputs.push(key_input(VK_BACK, true));
    }
    send(&inputs)
}

/// Thay `char_count` ký tự cuối bằng `replacement`.
///
/// Đây là thao tác mà việc chấp nhận một gợi ý quy về. Số đếm theo **ký tự
/// Unicode**, không phải byte hay code unit UTF-16 — vì Backspace xoá theo đơn vị
/// người dùng nhìn thấy.
pub fn replace_last(char_count: usize, replacement: &str) -> WinResult<()> {
    backspace(char_count)?;
    // Nhịp nghỉ ngắn: một số app xử lý Backspace bất đồng bộ, và gửi text quá sát
    // sau đó có thể bị chèn trước khi xoá xong.
    sleep(Duration::from_millis(8));
    type_text(replacement)
}

// ---------------------------------------------------------------------------
// Đường clipboard
// ---------------------------------------------------------------------------

/// Dán một đoạn text bằng clipboard, rồi **trả clipboard về nguyên trạng**.
///
/// Dùng cho đoạn dài, nơi gõ từng ký tự vừa chậm vừa dễ bị user gõ chen vào.
pub fn paste_text(text: &str) -> WinResult<()> {
    // Trước mọi thứ: Alt còn giữ thì `Ctrl+V` dưới đây thành `Ctrl+Alt+V` — chính
    // phím tắt của Writa.
    release_modifiers();

    let saved = clipboard_get_text();
    clipboard_set_text(text)?;

    // Ctrl+V
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_V, false),
        key_input(VK_V, true),
        key_input(VK_CONTROL, true),
    ];
    let result = send(&inputs);

    // App đích đọc clipboard bất đồng bộ; trả lại quá sớm thì nó dán nhầm nội dung cũ.
    sleep(Duration::from_millis(120));
    if let Some(old) = saved {
        let _ = clipboard_set_text(&old);
    }
    result
}

/// Chọn đường ghi phù hợp với độ dài.
pub fn write_text(text: &str) -> WinResult<()> {
    if text.chars().count() > CLIPBOARD_THRESHOLD {
        paste_text(text)
    } else {
        type_text(text)
    }
}

/// Số lần thử mở clipboard trước khi bỏ cuộc.
///
/// Clipboard là tài nguyên **toàn máy chỉ một chủ tại một thời điểm**: hễ app khác
/// đang giữ nó thì `OpenClipboard` trả `E_ACCESSDENIED` ngay. Chuyện này xảy ra
/// thường xuyên hơn tưởng — trình quản lý clipboard, Office, trình duyệt đều mở
/// clipboard chớp nhoáng khi có thay đổi, và Writa thì luôn chạm clipboard đúng ngay
/// sau khi vừa gửi `Ctrl+C`, tức đúng lúc đông đúc nhất.
///
/// Một lần hỏng ở đây làm hỏng cả đường lùi đọc vùng chọn *và* đường ghi ngược, mà
/// triệu chứng bên ngoài chỉ là "bấm phím tắt không thấy gì". Đợi vài chục mili giây
/// rồi thử lại giải quyết gần như toàn bộ, vì các app kia chỉ giữ trong chốc lát.
const CLIPBOARD_TRIES: u32 = 8;
const CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(12);

/// Mở clipboard, thử lại khi có app khác đang giữ.
fn open_clipboard() -> WinResult<()> {
    let mut last = 0;
    for attempt in 0..CLIPBOARD_TRIES {
        match unsafe { OpenClipboard(None) } {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e.code().0;
                if attempt + 1 < CLIPBOARD_TRIES {
                    sleep(CLIPBOARD_RETRY_DELAY);
                }
            }
        }
    }
    Err(WinError::Api {
        call: "OpenClipboard",
        code: last,
    })
}

/// Đọc text đang có trong clipboard.
pub fn clipboard_get_text() -> Option<String> {
    open_clipboard().ok()?;
    unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok();
        let text = handle.and_then(|h| {
            let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(h.0)) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(h.0));
            Some(s)
        });
        let _ = CloseClipboard();
        text
    }
}

/// Đặt text vào clipboard.
pub fn clipboard_set_text(text: &str) -> WinResult<()> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    let bytes = units.len() * 2;

    open_clipboard()?;
    unsafe {
        let result = (|| {
            EmptyClipboard().map_err(|e| WinError::Api {
                call: "EmptyClipboard",
                code: e.code().0,
            })?;
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| WinError::Api {
                call: "GlobalAlloc",
                code: e.code().0,
            })?;
            let dst = GlobalLock(mem) as *mut u16;
            if dst.is_null() {
                return Err(WinError::Api {
                    call: "GlobalLock",
                    code: 0,
                });
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
            let _ = GlobalUnlock(mem);
            // Clipboard nhận quyền sở hữu khối nhớ — KHÔNG được giải phóng ở đây.
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0))).map_err(|e| {
                WinError::Api {
                    call: "SetClipboardData",
                    code: e.code().0,
                }
            })?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Gửi Ctrl+C tới cửa sổ đang focus. Dùng làm đường lùi khi UIA đọc selection thất bại.
pub fn send_copy(_target: HWND) -> WinResult<()> {
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VIRTUAL_KEY(b'C' as u16), false),
        key_input(VIRTUAL_KEY(b'C' as u16), true),
        key_input(VK_CONTROL, true),
    ];
    send(&inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vietnamese_chars_are_single_utf16_units() {
        // Toàn bộ chữ tiếng Việt nằm trong BMP, nên mỗi chữ là một sự kiện input.
        // Nếu bất biến này vỡ thì việc đếm Backspace theo ký tự sẽ sai.
        for c in "tiếng Việt ưở đẹp ẫẩỡợ".chars() {
            assert_eq!(c.len_utf16(), 1, "{c:?} cần nhiều hơn một code unit");
        }
    }

    #[test]
    fn clipboard_threshold_picks_the_cheaper_path() {
        assert!("sửa".chars().count() <= CLIPBOARD_THRESHOLD);
        let long = "Tôi muốn chia sẻ điều này với tất cả mọi người ở đây.";
        assert!(long.chars().count() > CLIPBOARD_THRESHOLD);
    }
}
