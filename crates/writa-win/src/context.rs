//! Ngữ cảnh app đang focus — **và khi nào phải im lặng**.
//!
//! Đây là module quyết định Writa có được phép hoạt động ở đâu. Nó quan trọng hơn
//! độ chính xác của engine: một tool chạy nền mà đọc ô mật khẩu thì không có con số
//! chính tả nào cứu được.
//!
//! Nhận diện ô mật khẩu làm **nhiều lớp** vì không lớp nào phủ hết:
//! - Style `ES_PASSWORD` bắt được ô Edit chuẩn của Win32.
//! - Tên lớp cửa sổ bắt được vài control tự vẽ.
//! - UIA `IsPassword` (ở [`crate::selection`]) bắt được Chrome, Electron, app modern.
//!
//! Khi nghi ngờ thì coi là mật khẩu. Bỏ sót một ô text thường chẳng mất gì; bỏ sót
//! một ô mật khẩu thì mất tất cả.

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
// `AttachThreadInput` xếp ở System::Threading chứ không phải UI::* — nó thao tác
// hàng đợi input của *thread*, không phải của cửa sổ.
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow, GUITHREADINFO, GWL_STYLE,
    SW_RESTORE,
};

use crate::{wide_to_string, WinError, WinResult};

/// `ES_PASSWORD` — ô Edit hiển thị dấu sao thay vì chữ.
const ES_PASSWORD: i32 = 0x0020;

/// Tên lớp cửa sổ của các control mật khẩu không dùng `ES_PASSWORD`.
const PASSWORD_CLASS_HINTS: [&str; 4] = ["password", "passwd", "pinbox", "credential"];

/// App mặc định không bao giờ can thiệp.
///
/// Danh sách này là mặc định an toàn, không phải danh sách đầy đủ — user thêm được
/// qua per-app profile. Terminal nằm trong đây vì người ta dán mật khẩu và token
/// vào terminal suốt, và vì nội dung terminal là lệnh chứ không phải văn xuôi.
pub const DEFAULT_BLOCKLIST: [&str; 12] = [
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "lastpass.exe",
    "dashlane.exe",
    "mstsc.exe", // Remote Desktop
    "windowsterminal.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "conhost.exe",
];

/// Ảnh chụp ngữ cảnh app đang focus.
#[derive(Debug, Clone)]
pub struct AppContext {
    /// Cửa sổ đang focus (top-level).
    pub window: HWND,
    /// Control đang nhận bàn phím bên trong cửa sổ đó, nếu xác định được.
    pub focused_control: Option<HWND>,
    /// Tên file thực thi, chữ thường. `"chrome.exe"`, `"zalo.exe"`.
    pub exe: String,
    /// Tiêu đề cửa sổ — chỉ để hiển thị/gỡ lỗi, không dùng để phán quyết.
    pub title: String,
    /// Có phải ô mật khẩu không (theo các lớp nhận diện Win32).
    pub is_password_field: bool,
}

impl AppContext {
    /// Writa có được phép hoạt động ở đây không?
    ///
    /// Lưu ý điều này chỉ trả lời phần Win32. Lớp gọi **vẫn phải** hỏi thêm UIA
    /// `IsPassword` ([`crate::selection::is_password_element`]) trước khi đọc gì,
    /// vì Chrome và Electron không lộ `ES_PASSWORD`.
    pub fn is_safe_to_assist(&self) -> bool {
        !self.is_password_field && !self.is_blocklisted()
    }

    pub fn is_blocklisted(&self) -> bool {
        DEFAULT_BLOCKLIST.contains(&self.exe.as_str())
    }

    /// Định danh cửa sổ ở dạng mang đi được giữa các thread. Xem [`WindowId`].
    pub fn window_id(&self) -> WindowId {
        self.window.0 as WindowId
    }
}

/// Định danh cửa sổ dạng số nguyên.
///
/// `HWND` bọc một con trỏ thô nên không `Send`. Lớp shell phải nhớ cửa sổ đích từ
/// lúc user bấm hotkey đến lúc user bấm "Áp dụng" — hai thời điểm nằm ở hai thread
/// khác nhau — nên nó cần một dạng đi qua được ranh giới đó.
pub type WindowId = isize;

/// Đưa một cửa sổ trở lại foreground.
///
/// Cần cho luồng Tier 1: popup gợi ý **có** lấy focus (để user bấm chuột và gõ phím
/// bình thường), nên trước khi ghi text phải trả focus về app đích.
///
/// Windows chỉ cho phép đổi foreground khi process gọi *đang* là foreground — và ở
/// đây đúng như vậy, vì popup của chính ta vừa nhận focus. Nghịch lý nhỏ: việc lấy
/// focus, thứ thường bị coi là vấn đề, lại chính là điều khiến bước trả focus này
/// hợp lệ. Overlay không-lấy-focus của Tier 2 sẽ phải đi đường khác.
pub fn focus(window: WindowId) -> WinResult<()> {
    let hwnd = HWND(window as *mut std::ffi::c_void);

    // Cửa sổ có thể đã bị thu nhỏ trong lúc user xem popup.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }

    if unsafe { SetForegroundWindow(hwnd) }.as_bool() {
        return Ok(());
    }

    // Đường lùi: gắn input queue của ta vào thread cửa sổ đích, khi đó
    // `SetForegroundWindow` được xử lý như thể đến từ chính thread đó. Cần khi một
    // process thứ ba vừa chen vào foreground, hoặc app đích chạy ở integrity level
    // khác.
    let target_tid = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if target_tid == 0 {
        return Err(WinError::Api {
            call: "GetWindowThreadProcessId",
            code: 0,
        });
    }
    let our_tid = unsafe { GetCurrentThreadId() };
    let ok = unsafe {
        let _ = AttachThreadInput(our_tid, target_tid, true);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(our_tid, target_tid, false);
        ok
    };
    if ok {
        Ok(())
    } else {
        Err(WinError::Api {
            call: "SetForegroundWindow",
            code: 0,
        })
    }
}

/// Đọc ngữ cảnh của cửa sổ đang focus.
pub fn current() -> WinResult<AppContext> {
    let window = unsafe { GetForegroundWindow() };
    if window.0.is_null() {
        return Err(WinError::NoForegroundWindow);
    }

    let mut pid = 0u32;
    let tid = unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    if tid == 0 {
        return Err(WinError::Api {
            call: "GetWindowThreadProcessId",
            code: 0,
        });
    }

    // Control đang nhận bàn phím. GetFocus() chỉ trả về cho thread của CHÍNH ta,
    // nên phải đi qua GetGUIThreadInfo của thread cửa sổ kia.
    let mut gui = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let focused_control = unsafe { GetGUIThreadInfo(tid, &mut gui) }
        .ok()
        .and_then(|()| (!gui.hwndFocus.0.is_null()).then_some(gui.hwndFocus));

    let exe = process_exe_name(pid).unwrap_or_default();
    let title = window_text(window);
    let is_password_field = focused_control.is_some_and(looks_like_password_field);

    Ok(AppContext {
        window,
        focused_control,
        exe,
        title,
        is_password_field,
    })
}

/// Tên file thực thi của một process, chữ thường.
fn process_exe_name(pid: u32) -> Option<String> {
    // PROCESS_QUERY_LIMITED_INFORMATION là quyền hẹp nhất đủ dùng, và là quyền duy
    // nhất mở được process chạy ở integrity level cao hơn.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buf = [0u16; MAX_PATH as usize];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    ok.ok()?;

    let full = wide_to_string(&buf[..len as usize]);
    Some(
        full.rsplit(['\\', '/'])
            .next()
            .unwrap_or(&full)
            .to_lowercase(),
    )
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    wide_to_string(&buf[..len as usize])
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    wide_to_string(&buf[..len as usize]).to_lowercase()
}

/// Control này có phải ô mật khẩu không, xét theo tín hiệu Win32.
///
/// Không phủ hết — Chrome và Electron tự vẽ ô nhập nên không có style này. Đó là lý
/// do phải kiểm tra thêm UIA `IsPassword` trước khi đọc bất cứ thứ gì.
fn looks_like_password_field(hwnd: HWND) -> bool {
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };
    if style & ES_PASSWORD != 0 {
        return true;
    }
    let class = class_name(hwnd);
    PASSWORD_CLASS_HINTS.iter().any(|h| class.contains(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocklist_covers_password_managers_and_terminals() {
        for exe in [
            "keepassxc.exe",
            "1password.exe",
            "powershell.exe",
            "mstsc.exe",
        ] {
            let ctx = AppContext {
                window: HWND::default(),
                focused_control: None,
                exe: exe.to_string(),
                title: String::new(),
                is_password_field: false,
            };
            assert!(ctx.is_blocklisted(), "{exe} phải nằm trong blocklist");
            assert!(!ctx.is_safe_to_assist());
        }
    }

    #[test]
    fn ordinary_apps_are_allowed() {
        for exe in ["zalo.exe", "chrome.exe", "winword.exe", "notepad.exe"] {
            let ctx = AppContext {
                window: HWND::default(),
                focused_control: None,
                exe: exe.to_string(),
                title: String::new(),
                is_password_field: false,
            };
            assert!(ctx.is_safe_to_assist(), "{exe} phải được phép");
        }
    }

    #[test]
    fn password_field_blocks_even_in_an_allowed_app() {
        // Ô mật khẩu trong Chrome vẫn phải bị chặn — app được phép không có nghĩa
        // là mọi ô nhập trong nó đều được phép.
        let ctx = AppContext {
            window: HWND::default(),
            focused_control: None,
            exe: "chrome.exe".to_string(),
            title: String::new(),
            is_password_field: true,
        };
        assert!(!ctx.is_safe_to_assist());
    }
}
