//! P2 — vỏ ứng dụng.
//!
//! Đây là lớp biến engine thành thứ dùng được hàng ngày: khay hệ thống, phím tắt
//! toàn cục, popup gợi ý, cài đặt, từ điển cá nhân, khởi động cùng Windows.
//!
//! # Ranh giới với các crate khác
//!
//! Crate này **không** chứa logic chính tả và **không** gọi Win32 trực tiếp. Nó nối
//! `writa-core` (biết tiếng Việt, không biết OS) với `writa-win` (biết Windows,
//! không biết tiếng Việt), và quyết định *khi nào* thì làm gì.
//!
//! # Đang chạy tới đâu
//!
//! Mới là **Tier 1**: user chủ động bôi đen rồi bấm phím tắt. Tier 2 (bắt phím thời
//! gian thực, gạch đỏ ngay lúc gõ) là P4, và phụ thuộc spike 5 — xem SPIKE_RESULTS.md.

mod commands;
mod config;
mod debug;
mod flow;
mod hotkey;
mod model;
mod realtime;
mod review;
mod state;
mod tray;
mod update;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::engine_info,
            commands::get_review,
            commands::fit_popup,
            commands::dismiss_review,
            commands::ignore_word,
            commands::copy_to_clipboard,
            commands::apply_review,
            commands::fit_inline,
            commands::check_update,
            commands::install_update,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            let mut settings = config::load(&handle);

            // Autostart: lấy trạng thái THẬT từ hệ thống chứ không tin file cấu hình.
            // User có thể đã tắt nó bằng Task Manager, và ô tick phải nói sự thật.
            if let Ok(enabled) = handle.autolaunch().is_enabled() {
                settings.autostart = enabled;
            }

            let (check, diacritic, accept) = hotkey::rebind(&handle, &settings, &settings);
            settings.hotkey_check = check;
            settings.hotkey_diacritic = diacritic;
            settings.hotkey_accept = accept;

            let paused = settings.paused;
            let realtime_on = settings.realtime && !paused;
            *app.state::<state::AppState>().settings.lock().unwrap() = settings;

            tray::build(&handle)?;
            if paused {
                tray::refresh(&handle, true);
            }

            // Overlay inline phải được gắn cờ không-lấy-focus **trước** lần hiện đầu
            // tiên, nếu không nó cướp caret đúng một lần — và một lần là đủ để user
            // mất chỗ đang gõ.
            realtime::prepare_overlay(&handle);
            if realtime_on {
                realtime::set_enabled(&handle, true);
            }
            update::check_on_startup(&handle);

            // Popup mất focus = user đã chuyển sự chú ý đi chỗ khác. Đóng luôn, đừng
            // để một cửa sổ luôn-trên-cùng lảng vảng che mất việc của họ.
            if let Some(popup) = app.get_webview_window(flow::POPUP) {
                let h = handle.clone();
                let w = popup.clone();
                popup.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        if w.is_visible().unwrap_or(false) {
                            flow::hide_popup(&h);
                        }
                    }
                });
            }

            // Cửa sổ cài đặt được tạo **theo nhu cầu** ở `flow::show_settings`, kể cả
            // phần xử lý sự kiện đóng: giữ sẵn một WebView cho thứ user mở vài lần mỗi
            // tháng là trả RAM suốt phiên làm việc để tiết kiệm một giây lúc mở.

            // `--selftest`: kiểm một câu có lỗi sẵn rồi mở popup ngay khi khởi động.
            //
            // Tồn tại vì luồng thật đi qua bốn tầng hỏng được độc lập (phím tắt →
            // ngữ cảnh app → đọc vùng chọn → hiện popup), và khi user báo "không có
            // gì xảy ra" thì không cách nào biết tầng nào chết. Đường này bỏ qua ba
            // tầng đầu, nên nó trả lời đúng một câu: **popup có hiện được không.**
            if std::env::args().any(|a| a == "--selftest") {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    flow::review_text(
                        &h,
                        model::Mode::Check,
                        "Tôi làm trong nghành công nghiệp này và muốn chia sẽ điều đó",
                        "self-test",
                    );
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("không khởi tạo được Writa")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { code, api, .. } = event {
                // `code` rỗng nghĩa là "cửa sổ cuối cùng đã đóng". Với app khay hệ
                // thống thì đó không phải lý do để thoát; chỉ menu "Thoát" (gọi
                // `app.exit`, có mã) mới được thoát thật.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
