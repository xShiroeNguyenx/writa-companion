//! Khay hệ thống — mặt duy nhất của Writa khi nó chạy nền.
//!
//! Icon đổi màu khi tạm dừng, và đó là chủ ý: một app đọc được mọi ô nhập trên máy
//! phải cho biết nó đang bật hay tắt **mà không cần mở menu ra xem**.
//!
//! # Vì sao menu có mục "clipboard"
//!
//! Phím tắt chỉ chạy được khi app đích đang focus. Nhưng bấm vào menu khay hệ thống
//! thì foreground đã là taskbar rồi, nên "kiểm tra đoạn đang bôi đen" ở đây sẽ đọc
//! nhầm cửa sổ. Đường clipboard không phụ thuộc cửa sổ nào nên luôn đúng — và tiện
//! cho việc thử nhanh xem engine còn sống không.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::flow;
use crate::model::Mode;
use crate::state::AppState;

pub const TRAY_ID: &str = "writa";

const ICON_ACTIVE: &[u8] = include_bytes!("../icons/tray.png");
const ICON_PAUSED: &[u8] = include_bytes!("../icons/tray-paused.png");

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_ACTIVE)?)
        .tooltip("Writa")
        .menu(&menu(app, false)?)
        .on_menu_event(on_menu)
        .build(app)?;
    Ok(())
}

/// Vẽ lại icon và menu theo trạng thái bật/tắt.
pub fn refresh(app: &AppHandle, paused: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let bytes = if paused { ICON_PAUSED } else { ICON_ACTIVE };
    if let Ok(icon) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(if paused {
        "Writa — đã tạm dừng"
    } else {
        "Writa — đang hoạt động"
    }));
    if let Ok(menu) = menu(app, paused) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn menu(app: &AppHandle, paused: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let status = MenuItem::with_id(
        app,
        "status",
        if paused {
            "Đã tạm dừng"
        } else {
            "Đang hoạt động"
        },
        false, // vô hiệu hoá: đây là nhãn trạng thái, không phải nút
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        if paused {
            "Bật lại"
        } else {
            "Tạm dừng"
        },
        true,
        None::<&str>,
    )?;
    let check_clip = MenuItem::with_id(
        app,
        "clipboard-check",
        "Kiểm tra nội dung trong clipboard",
        true,
        None::<&str>,
    )?;
    let restore_clip = MenuItem::with_id(
        app,
        "clipboard-restore",
        "Thêm dấu cho nội dung trong clipboard",
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "Cài đặt…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Thoát Writa", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &status,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &check_clip,
            &restore_clip,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &quit,
        ],
    )
}

fn on_menu(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "pause" => {
            let state = app.state::<AppState>();
            let paused = {
                let mut s = state.settings.lock().unwrap();
                s.paused = !s.paused;
                s.paused
            };
            let snapshot = state.settings.lock().unwrap().clone();
            let _ = crate::config::save(app, &snapshot);
            if paused {
                flow::hide_popup(app);
            }
            refresh(app, paused);
            // Cửa sổ cài đặt có thể đang mở và đang hiển thị trạng thái cũ.
            let _ = app.emit_to(flow::SETTINGS, "writa://settings", ());
        }
        "clipboard-check" => flow::trigger_clipboard(app, Mode::Check),
        "clipboard-restore" => flow::trigger_clipboard(app, Mode::Diacritic),
        "settings" => flow::show_settings(app),
        "quit" => app.exit(0),
        _ => {}
    }
}
