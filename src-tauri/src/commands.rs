//! Các lệnh UI gọi xuống.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use writa_win::{context, selection, writer};

use crate::config::{self, Settings};
use crate::debug::dbg_log;
use crate::flow;
use crate::hotkey;
use crate::model::{ApplyOutcome, Decision, EngineInfo, ReviewPayload};
use crate::realtime;
use crate::review;
use crate::state::AppState;
use crate::tray;

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

/// Lưu thiết lập và trả về thứ **thật sự** đang chạy.
///
/// Trả về bản đã áp dụng thay vì bản user gửi lên, vì hai thứ có thể khác nhau: phím
/// tắt bị app khác chiếm, autostart bị chính sách hệ thống chặn. UI vẽ lại theo giá
/// trị trả về nên nó không bao giờ hiển thị một thiết lập không có thật.
#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Settings {
    let state = app.state::<AppState>();
    let previous = state.settings.lock().unwrap().clone();

    let mut next = settings;
    next.sanitize();

    let (check, diacritic, accept) = hotkey::rebind(&app, &next, &previous);
    next.hotkey_check = check;
    next.hotkey_diacritic = diacritic;
    next.hotkey_accept = accept;

    if next.autostart != previous.autostart {
        let launcher = app.autolaunch();
        let result = if next.autostart {
            launcher.enable()
        } else {
            launcher.disable()
        };
        if result.is_err() {
            // Không đổi được thì báo cáo sự thật, đừng báo cáo mong muốn.
            next.autostart = launcher.is_enabled().unwrap_or(previous.autostart);
        }
    }

    if next.paused != previous.paused {
        if next.paused {
            flow::hide_popup(&app);
        }
        tray::refresh(&app, next.paused);
    }

    // Lưu trước khi bật/tắt hook: `realtime::set_enabled` đọc thiết lập từ state.
    *state.settings.lock().unwrap() = next.clone();

    // Tạm dừng phải tháo hook thật, không chỉ ngừng xử lý sự kiện.
    let want_realtime = next.realtime && !next.paused;
    if want_realtime != realtime::is_enabled() {
        realtime::set_enabled(&app, want_realtime);
    }

    let _ = config::save(&app, &next);
    next
}

#[tauri::command]
pub fn engine_info() -> EngineInfo {
    let s = writa_core::dict::stats();
    EngineInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        syllables: writa_core::phonology::syllable_set().len(),
        attested: s.attested,
        accepted_foreign: s.accepted_foreign,
        compounds: s.compounds,
        trigrams: s.trigrams,
        default_blocklist: context::DEFAULT_BLOCKLIST
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

#[tauri::command]
pub fn get_review(app: AppHandle) -> Option<ReviewPayload> {
    let out = app
        .state::<AppState>()
        .review
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.payload.clone());
    dbg_log!(
        "get_review -> {}",
        out.as_ref().map_or("None".to_string(), |p| format!(
            "{} thay doi",
            p.changes.len()
        ))
    );
    out
}

/// Popup báo chiều cao nó cần; Rust đặt kích thước, định vị, rồi mới hiện.
#[tauri::command]
pub fn fit_popup(app: AppHandle, height: f64) -> Result<(), String> {
    flow::size_and_place(&app, height)
}

/// Overlay inline báo kích thước nội dung; Rust đặt kích thước, neo cạnh caret, hiện
/// mà không lấy focus.
#[tauri::command]
pub fn fit_inline(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    realtime::fit_and_show(&app, width, height)
}

#[tauri::command]
pub fn dismiss_review(app: AppHandle) {
    flow::hide_popup(&app);
}

/// Thêm một từ vào từ điển cá nhân.
#[tauri::command]
pub fn ignore_word(app: AppHandle, word: String) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut s = state.settings.lock().unwrap();
        let word = word.trim().to_lowercase();
        if word.is_empty() || s.personal_dict.contains(&word) {
            return;
        }
        s.personal_dict.push(word);
        s.personal_dict.sort();
        s.clone()
    };
    let _ = config::save(&app, &snapshot);
    let _ = app.emit_to(flow::SETTINGS, "writa://settings", ());
}

#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    writer::clipboard_set_text(&text).map_err(|e| e.to_string())
}

/// Hỏi xem có bản mới không. Không tải, không cài.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<crate::update::UpdateInfo>, String> {
    crate::update::check(&app).await
}

/// Tải, cài, rồi khởi động lại. Chỉ chạy khi user chủ động bấm.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::update::install(app).await
}

/// Ghi bản đã sửa về chỗ nó đến.
///
/// `async` để Tauri chạy lệnh này ngoài main thread: nó chờ focus chuyển xong rồi
/// gọi UIA, tổng cộng cả trăm mili-giây. Chạy trên main thread thì cả giao diện đứng
/// hình đúng lúc user vừa bấm nút.
#[tauri::command]
pub async fn apply_review(app: AppHandle, decisions: Vec<Decision>) -> ApplyOutcome {
    let Some((payload, target)) = app
        .state::<AppState>()
        .review
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| (r.payload.clone(), r.target))
    else {
        return ApplyOutcome::failed(String::new(), "Lượt xem lại đã đóng rồi.");
    };

    let text = review::apply(&payload.original, &payload.changes, &decisions);

    let Some(target) = target else {
        // Chế độ clipboard: kết quả quay về đúng chỗ nó đến.
        return match writer::clipboard_set_text(&text) {
            Ok(()) => {
                flow::hide_popup(&app);
                ApplyOutcome::ok(text)
            }
            Err(e) => ApplyOutcome::failed(text, format!("Không ghi được vào clipboard ({e}).")),
        };
    };

    // Ẩn trước khi trả focus: popup luôn-trên-cùng nằm ngay trên chỗ sắp ghi.
    let popup = app.get_webview_window(flow::POPUP);
    if let Some(win) = &popup {
        let _ = win.hide();
    }
    let reshow = || {
        if let Some(win) = &popup {
            let _ = win.show();
            let _ = win.set_focus();
        }
    };

    if let Err(e) = context::focus(target) {
        reshow();
        return ApplyOutcome::failed(text, format!("Không quay lại được app đích ({e})."));
    }
    // App đích cần một nhịp để thật sự nhận focus trước khi ta bơm phím vào nó.
    std::thread::sleep(Duration::from_millis(120));

    // Chốt an toàn quan trọng nhất của cả luồng này.
    //
    // Ta ghi đè bằng cách dựa vào việc **vùng chọn cũ vẫn còn** ở app đích. Hầu hết
    // app giữ vùng chọn khi mất focus (chỉ vẽ nhạt đi), nhưng không phải tất cả — và
    // nếu nó đã mất thì ta không ghi đè mà chèn thêm, tức là nhân đôi đoạn text của
    // user. Đọc lại qua UIA để kiểm chứng trước khi ghi.
    //
    // App nào UIA không đọc được thì ta không có cách nào biết, và vẫn ghi như cũ:
    // bước này chỉ chặn được trường hợp quan sát được, nhưng không bao giờ làm mọi
    // thứ tệ hơn hiện trạng.
    if let Ok(now) = selection::read_selection_uia() {
        if now.trim() != payload.original.trim() {
            reshow();
            return ApplyOutcome::failed(
                text,
                "Vùng chọn ở app đích không còn như lúc kiểm tra — không ghi đè để tránh làm hỏng text.",
            );
        }
    }

    match writer::write_text(&text) {
        Ok(()) => {
            flow::hide_popup(&app);
            ApplyOutcome::ok(text)
        }
        Err(e) => {
            reshow();
            ApplyOutcome::failed(text, format!("Không ghi được vào app đích ({e})."))
        }
    }
}
