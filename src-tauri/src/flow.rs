//! Luồng Tier 1: phím tắt → đọc vùng chọn → kiểm tra → hiện popup.
//!
//! # Thứ tự ở đây không tuỳ tiện
//!
//! Đọc vùng chọn **phải** xong trước khi hiện popup, vì popup lấy focus và đường lùi
//! của việc đọc là gửi `Ctrl+C` tới cửa sổ đang focus. Hiện popup trước thì ta sẽ
//! copy chính popup của mình.
//!
//! # Vì sao có `notice`
//!
//! Mọi nhánh thoát sớm — chưa bôi đen, app bị chặn, ô mật khẩu — đều dẫn tới một
//! popup có lời nhắn, **trừ** trường hợp bị chặn vì lý do riêng tư. User bấm phím
//! tắt mà không thấy gì sẽ nghĩ app hỏng và bấm lại; nhưng ở ô mật khẩu thì im lặng
//! mới đúng: một popup hiện lên đúng lúc đó cũng là một cách xác nhận Writa đang đọc
//! ô đó.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize};
use writa_win::{caret, context, selection};

use crate::debug::dbg_log;
use crate::model::{Mode, ReviewPayload};
use crate::review;
use crate::state::{ActiveReview, AppState};

pub const POPUP: &str = "popup";
pub const SETTINGS: &str = "settings";

/// Chiều rộng popup, đơn vị logic. Cố định để danh sách lỗi không nhảy ngang mỗi
/// lần nội dung đổi.
pub const POPUP_WIDTH: f64 = 520.0;

/// Neo popup vào đâu trên màn hình.
#[derive(Debug, Clone, Copy)]
pub enum Anchor {
    /// Ngay dưới caret của app đích. Toạ độ vật lý.
    At { x: i32, y: i32, line_height: i32 },
    /// Không biết caret ở đâu (chế độ clipboard) — neo giữa màn hình chính.
    Center,
}

/// Đang có một lượt chụp dở dang.
///
/// Giữ phím tắt sẽ bắn liên tiếp; mỗi lượt lại gửi `Ctrl+C` và đụng clipboard, nên
/// chồng lượt vừa chậm vừa làm hỏng việc khôi phục clipboard.
static BUSY: AtomicBool = AtomicBool::new(false);

/// Bấm phím tắt. Trả ngay, việc nặng chạy ở thread riêng.
///
/// Handler của plugin phím tắt chạy trên thread dịch vụ của nó; giữ thread đó cả
/// trăm mili-giây trong lúc gọi UIA sẽ làm trễ mọi phím tắt sau đó.
pub fn trigger(app: &AppHandle, mode: Mode) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        capture(&app, mode);
        BUSY.store(false, Ordering::SeqCst);
    });
}

fn capture(app: &AppHandle, mode: Mode) {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if settings.paused {
        return;
    }

    let Ok(ctx) = context::current() else {
        return;
    };

    // Cửa sổ của chính Writa: không có gì để kiểm tra, và đọc nó sẽ đá vào popup.
    if own_exe().is_some_and(|own| own == ctx.exe) {
        return;
    }

    // Ba lớp chặn vì riêng tư. Im lặng tuyệt đối — xem ghi chú đầu file.
    if !ctx.is_safe_to_assist() || settings.blocks(&ctx.exe) {
        return;
    }
    if selection::is_password_element() {
        return;
    }

    // Lấy caret TRƯỚC khi đọc vùng chọn: đường lùi đọc bằng clipboard gửi `Ctrl+C`,
    // và ở vài app điều đó làm con trỏ nhảy.
    let caret = caret::locate(&ctx);
    let anchor = Anchor::At {
        x: caret.x,
        y: caret.y,
        line_height: if caret.source.is_exact() {
            caret.height
        } else {
            0
        },
    };

    let payload = match selection::read_selection() {
        Ok(text) if !text.trim().is_empty() => ReviewPayload {
            mode,
            app: ctx.exe.clone(),
            changes: review::build(mode, &text, &settings),
            original: text,
            notice: None,
        },
        Ok(_) | Err(writa_win::WinError::NothingSelected) => empty(
            mode,
            &ctx.exe,
            "Chưa bôi đen đoạn nào. Chọn text rồi bấm phím tắt lại.",
        ),
        Err(e) => empty(
            mode,
            &ctx.exe,
            format!("Không đọc được vùng chọn ở app này ({e})."),
        ),
    };

    present(
        app,
        ActiveReview {
            payload,
            target: Some(ctx.window_id()),
            anchor,
        },
    );
}

/// Kiểm tra nội dung đang có trong clipboard.
///
/// Có mặt vì phím tắt phụ thuộc vào việc app đích đang focus, mà khay hệ thống thì
/// không: bấm vào menu tray là taskbar chiếm foreground rồi. Đường này không phụ
/// thuộc cửa sổ nào nên luôn dùng được — kể cả để thử nhanh xem engine chạy chưa.
pub fn trigger_clipboard(app: &AppHandle, mode: Mode) {
    match writa_win::writer::clipboard_get_text() {
        Some(text) if !text.trim().is_empty() => review_text(app, mode, &text, "clipboard"),
        _ => present(
            app,
            ActiveReview {
                payload: empty(mode, "clipboard", "Clipboard đang trống."),
                target: None,
                anchor: Anchor::Center,
            },
        ),
    }
}

/// Kiểm tra một đoạn text cho sẵn, không đọc từ đâu cả.
///
/// Đường vào của `--selftest`. Cố tình **không** đi qua clipboard: self-test tồn tại
/// để trả lời "popup có hiện được không", nên nó không được hỏng vì một lý do khác —
/// và clipboard thì hỏng được (app khác giữ, hoặc phiên làm việc không có window
/// station tương tác).
pub fn review_text(app: &AppHandle, mode: Mode, text: &str, source: &str) {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    present(
        app,
        ActiveReview {
            payload: ReviewPayload {
                mode,
                app: source.to_string(),
                changes: review::build(mode, text, &settings),
                original: text.to_string(),
                notice: None,
            },
            target: None,
            anchor: Anchor::Center,
        },
    );
}

fn empty(mode: Mode, app: &str, notice: impl Into<String>) -> ReviewPayload {
    ReviewPayload {
        mode,
        app: app.to_string(),
        original: String::new(),
        changes: Vec::new(),
        notice: Some(notice.into()),
    }
}

/// Đưa lượt xem lại cho popup.
///
/// Chỉ **báo** cho popup chứ không tự hiện nó. Popup nạp nội dung, tự đo chiều cao,
/// rồi gọi ngược `fit_popup` — nơi cửa sổ mới thật sự hiện ra. Hiện trước khi đo thì
/// user thấy một khung sai kích thước nhấp nháy, mà popup thì mở ra đóng vào liên
/// tục nên cái nhấp nháy đó rất lộ.
fn present(app: &AppHandle, review: ActiveReview) {
    dbg_log!(
        "present: mode={:?} app={} changes={} notice={:?}",
        review.payload.mode,
        review.payload.app,
        review.payload.changes.len(),
        review.payload.notice
    );
    *app.state::<AppState>().review.lock().unwrap() = Some(review);
    let sent = app.emit_to(POPUP, "writa://review", ());
    dbg_log!("present: emit_to(popup) -> {sent:?}");
}

pub fn hide_popup(app: &AppHandle) {
    *app.state::<AppState>().review.lock().unwrap() = None;
    if let Some(win) = app.get_webview_window(POPUP) {
        let _ = win.hide();
    }
}

/// Mở cửa sổ cài đặt, **tạo nó nếu chưa có**.
///
/// Cửa sổ này cố tình không nằm trong `tauri.conf.json`: mỗi cửa sổ WebView giữ sẵn là
/// vài chục MB RAM cho một thứ user mở vài lần mỗi tháng. Writa chạy nền cả ngày, nên
/// trả cái giá đó suốt phiên làm việc để tiết kiệm một giây lúc mở là đánh đổi sai
/// chiều.
pub fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        return;
    }

    let built = tauri::WebviewWindowBuilder::new(
        app,
        SETTINGS,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Writa — Cài đặt")
    .inner_size(760.0, 900.0)
    .min_inner_size(600.0, 460.0)
    .center()
    .resizable(true)
    .build();

    match built {
        Ok(win) => {
            // Đóng là **ẩn**, không phải huỷ: dựng lại WebView mỗi lần đóng mở thì user
            // thấy một khoảng trắng chờ, còn RAM thì chỉ tốn khi họ thật sự dùng tới.
            let w = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = w.hide();
                }
            });
        }
        Err(e) => dbg_log!("settings: khong tao duoc cua so ({e})"),
    }
}

/// Đặt kích thước rồi định vị popup, và hiện nó nếu đang có việc để xem.
pub fn size_and_place(app: &AppHandle, height: f64) -> Result<(), String> {
    dbg_log!("fit_popup: height={height}");
    let win = app
        .get_webview_window(POPUP)
        .ok_or_else(|| "không tìm thấy cửa sổ popup".to_string())?;
    win.set_size(LogicalSize::new(POPUP_WIDTH, height))
        .map_err(|e| e.to_string())?;

    let anchor = app
        .state::<AppState>()
        .review
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.anchor);

    // Không có lượt xem lại nào thì đây là lần nạp lúc khởi động — đo xong rồi thôi,
    // tuyệt đối không hiện.
    let Some(anchor) = anchor else {
        dbg_log!("fit_popup: chua co luot xem lai, khong hien");
        return Ok(());
    };

    let size = win.outer_size().unwrap_or(PhysicalSize::new(520, 240));
    place(app, anchor, size);
    let shown = win.show();
    let focused = win.set_focus();
    dbg_log!("fit_popup: show -> {shown:?}, focus -> {focused:?}");
    Ok(())
}

/// Đặt popup cạnh caret, kéo về trong màn hình chứa caret.
///
/// Kẹp theo màn hình là bắt buộc chứ không phải trau chuốt: caret ở cuối dòng cuối
/// màn hình là chuyện rất thường, và một popup rơi ra ngoài viền thì coi như không
/// hiện.
fn place(app: &AppHandle, anchor: Anchor, size: PhysicalSize<u32>) {
    let Some(win) = app.get_webview_window(POPUP) else {
        return;
    };
    let (w, h) = (size.width as i32, size.height as i32);

    let monitor = match anchor {
        Anchor::At { x, y, .. } => app.monitor_from_point(x as f64, y as f64).ok().flatten(),
        Anchor::Center => app.primary_monitor().ok().flatten(),
    };

    let (mut x, mut y) = match anchor {
        Anchor::At { x, y, line_height } => (x, y + line_height + 8),
        Anchor::Center => match &monitor {
            Some(m) => (
                m.position().x + (m.size().width as i32 - w) / 2,
                m.position().y + (m.size().height as i32 - h) / 3,
            ),
            None => (240, 240),
        },
    };

    if let Some(m) = monitor {
        let (p, s) = (m.position(), m.size());
        let left = p.x + 8;
        let top = p.y + 8;
        let right = (p.x + s.width as i32 - w - 8).max(left);
        let bottom = (p.y + s.height as i32 - h - 8).max(top);

        x = x.clamp(left, right);
        if y > bottom {
            // Không đủ chỗ bên dưới: lật lên phía trên dòng đang gõ.
            if let Anchor::At { y: caret_y, .. } = anchor {
                y = caret_y - h - 8;
            }
        }
        y = y.clamp(top, bottom);
    }

    let _ = win.set_position(PhysicalPosition::new(x, y));
}

/// Tên file thực thi của chính Writa, chữ thường.
fn own_exe() -> Option<&'static str> {
    static OWN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    OWN.get_or_init(|| {
        std::env::current_exe()
            .ok()?
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
    })
    .as_deref()
}
