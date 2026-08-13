//! Tự cập nhật.
//!
//! # Vì sao một app như thế này *phải* có tự cập nhật
//!
//! Writa cắm hook bàn phím toàn máy và ghi text vào ô nhập của người khác. Khi tìm ra
//! một lỗi làm hỏng text — như lỗi "phím phụ còn bị giữ" từng biến bản sửa thành từ mới
//! — thì bản vá phải tới được tay user, chứ không nằm chờ họ tình cờ ghé trang tải về.
//! Với công cụ chạy nền, "user tự đi tải bản mới" nghĩa là **không bao giờ**.
//!
//! # Ba ràng buộc tự đặt ra
//!
//! 1. **Không bao giờ tự cài.** Kiểm tra và tải thì tự động, nhưng thay thế file thực
//!    thi rồi khởi động lại là việc user phải bấm. Một app đọc được mọi ô nhập mà tự
//!    thay chính nó trong lúc user đang gõ là chuyện không nên xảy ra.
//! 2. **Chữ ký bắt buộc.** Gói cập nhật phải ký bằng khoá riêng ở `.keys/`; Tauri từ
//!    chối bản không khớp khoá công khai nhúng trong app. Đây là thứ ngăn một endpoint
//!    bị chiếm quyền đẩy mã tuỳ ý vào máy user.
//! 3. **Tắt được, và mặc định là hỏi.** Xem [`crate::config::Settings::auto_update`].
//!
//! # Vì sao kiểm tra ở đây chứ không dùng hộp thoại sẵn của plugin
//!
//! `dialog: false` trong `tauri.conf.json`: hộp thoại mặc định bật lên giữa màn hình và
//! cướp focus — đúng thứ Writa dành cả P4 để tránh. Thay vào đó ta báo bằng popup của
//! chính mình, vốn đã biết cách hiện mà không phá việc user đang làm.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::debug::dbg_log;
use crate::state::AppState;

/// Đợi bao lâu sau khi khởi động rồi mới kiểm tra.
///
/// Không kiểm ngay: lúc đăng nhập Windows còn đang nạp hàng chục thứ khác, và một lượt
/// gọi mạng ở đó chỉ làm máy chậm thêm mà chẳng ai được lợi.
const STARTUP_DELAY_SECS: u64 = 90;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Kiểm tra ngầm sau khi khởi động, nếu user đã bật.
pub fn check_on_startup(app: &AppHandle) {
    let enabled = app.state::<AppState>().settings.lock().unwrap().auto_update;
    if !enabled {
        dbg_log!("update: user da tat kiem tra tu dong");
        return;
    }
    let app = app.clone();
    // Ngủ trên thread riêng rồi mới nhảy vào async runtime: `tokio` không phải
    // dependency trực tiếp của crate này, và kéo nó vào chỉ để `sleep` là thêm một
    // dependency cho một việc mà `std` làm được.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(STARTUP_DELAY_SECS));
        tauri::async_runtime::spawn(async move {
            match check(&app).await {
                Ok(Some(info)) => {
                    dbg_log!("update: co ban moi {}", info.version);
                    let _ = app.emit("writa://update", info);
                }
                Ok(None) => dbg_log!("update: dang la ban moi nhat"),
                Err(e) => dbg_log!("update: khong kiem tra duoc ({e})"),
            }
        });
    });
}

/// Hỏi endpoint xem có bản mới không. Không tải, không cài.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

/// Tải và cài bản mới, rồi khởi động lại.
///
/// Chỉ chạy khi user chủ động bấm — xem ghi chú đầu file. Tauri kiểm chữ ký của gói
/// **trước khi** ghi đè bất cứ thứ gì; chữ ký sai thì hàm này trả lỗi và file cũ còn
/// nguyên.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("Không còn bản mới nào để cài.".into());
    };

    dbg_log!("update: dang tai {}", update.version);
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| format!("Không cài được bản mới ({e})."))?;

    dbg_log!("update: da cai, khoi dong lai");
    // Tháo hook trước khi tiến trình chết, để không để lại hook mồ côi.
    crate::realtime::set_enabled(&app, false);
    app.restart();
}
