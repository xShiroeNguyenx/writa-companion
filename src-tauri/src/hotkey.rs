//! Đăng ký phím tắt toàn cục.
//!
//! # Vì sao có chuỗi lùi ba bậc
//!
//! Phím tắt toàn cục là tài nguyên **độc quyền toàn máy**: app nào đăng ký trước thì
//! giữ. User gõ `Ctrl+Alt+V` vào ô cài đặt trong khi một app khác đang giữ nó thì
//! đăng ký hỏng — và nếu ta chỉ báo lỗi rồi thôi, Writa sẽ ngồi đó **không có phím
//! tắt nào** dù cài đặt hiển thị là có. Nên: thử cái user muốn, không được thì quay
//! về cái đang chạy, không được nữa thì về mặc định.
//!
//! Lệnh `save_settings` trả về thứ **thật sự đăng ký được**, và UI vẽ lại theo đó —
//! nhờ vậy ô nhập không bao giờ hiển thị một phím tắt không tồn tại.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::config::Settings;
use crate::flow;
use crate::model::Mode;

/// Việc mà một phím tắt kích hoạt.
#[derive(Debug, Clone, Copy)]
enum Action {
    /// Kiểm tra / thêm dấu cho vùng chọn (Tier 1).
    Selection(Mode),
    /// Áp dụng gợi ý inline đang hiện (Tier 2).
    ///
    /// Đi qua phím tắt toàn cục chứ không qua hook, dù Tier 2 vốn đã có hook: overlay
    /// mang `WS_EX_NOACTIVATE` nên không nhận được phím, và tự dò tổ hợp Ctrl+Alt
    /// trong hook thì phải tự theo dõi trạng thái phím phụ — việc mà plugin phím tắt
    /// đã làm đúng rồi.
    AcceptInline,
}

/// Ba phím tắt, đăng ký lại toàn bộ. Trả về những gì **thật sự** giữ chỗ được.
pub fn rebind(app: &AppHandle, wanted: &Settings, previous: &Settings) -> (String, String, String) {
    let _ = app.global_shortcut().unregister_all();
    let defaults = Settings::default();

    let check = first_that_binds(
        app,
        Action::Selection(Mode::Check),
        [
            &wanted.hotkey_check,
            &previous.hotkey_check,
            &defaults.hotkey_check,
        ],
    );
    let diacritic = first_that_binds(
        app,
        Action::Selection(Mode::Diacritic),
        [
            &wanted.hotkey_diacritic,
            &previous.hotkey_diacritic,
            &defaults.hotkey_diacritic,
        ],
    );
    let accept = first_that_binds(
        app,
        Action::AcceptInline,
        [
            &wanted.hotkey_accept,
            &previous.hotkey_accept,
            &defaults.hotkey_accept,
        ],
    );
    (check, diacritic, accept)
}

fn first_that_binds(app: &AppHandle, action: Action, candidates: [&String; 3]) -> String {
    let mut tried: Vec<&str> = Vec::new();
    for spec in candidates {
        if tried.contains(&spec.as_str()) {
            continue;
        }
        tried.push(spec);
        if bind(app, action, spec) {
            return spec.clone();
        }
    }
    // Cả ba đều hỏng: trả chuỗi rỗng thay vì nói dối. UI hiện ô trống, user thấy ngay
    // là chưa có phím tắt nào.
    String::new()
}

fn bind(app: &AppHandle, action: Action, spec: &str) -> bool {
    let Ok(shortcut) = spec.parse::<Shortcut>() else {
        return false;
    };
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            // Nhả phím cũng bắn sự kiện; bỏ qua, nếu không mỗi lần bấm là hai lượt.
            if event.state() != ShortcutState::Pressed {
                return;
            }
            match action {
                Action::Selection(mode) => flow::trigger(app, mode),
                Action::AcceptInline => {
                    let app = app.clone();
                    // Việc này gõ phím và gọi UIA — không giữ thread phím tắt lại.
                    std::thread::spawn(move || crate::realtime::accept(&app));
                }
            }
        })
        .is_ok()
}
