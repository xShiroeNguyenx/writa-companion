// Không mở cửa sổ console ở bản release — Writa chạy nền, một khung đen nhấp nháy
// lúc khởi động vừa xấu vừa đúng dấu hiệu của phần mềm đáng ngờ.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    writa_app::run()
}
