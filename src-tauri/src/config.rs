//! Thiết lập người dùng, lưu xuống `%APPDATA%\Writa\settings.json`.
//!
//! # Cái gì được lưu và cái gì không
//!
//! File này chứa **thiết lập** và **từ điển cá nhân** — không có một mẩu text nào
//! user từng gõ. Đó là ranh giới cố ý: Writa đọc được mọi ô nhập trên máy, nên thứ
//! duy nhất chạm đĩa phải là thứ user tự tay thêm vào.
//!
//! # Vì sao tự viết thay vì dùng plugin store
//!
//! Cần đúng hai thao tác — đọc lúc khởi động, ghi lúc user đổi — trên một struct đã
//! có sẵn `Serialize`. Thêm một plugin cho việc đó là thêm bề mặt phải bảo trì mà
//! không đổi lấy được gì.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use writa_core::{rules::RuleOptions, CheckOptions, DEFAULT_REAL_WORD_MARGIN};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Tạm ngừng hoàn toàn. Phím tắt vẫn đăng ký nhưng không làm gì — để cái hiện
    /// trong cài đặt luôn là cái thật sự đang giữ chỗ trên hệ thống.
    pub paused: bool,
    pub hotkey_check: String,
    pub hotkey_diacritic: String,
    /// Phím áp dụng gợi ý inline của Tier 2.
    pub hotkey_accept: String,

    /// Kiểm tra ngay lúc gõ (Tier 2).
    ///
    /// Mặc định **tắt**, và không phải vì nó chưa chạy được: bật nó lên là cắm một
    /// hook bàn phím toàn máy. Đó là thứ user phải chủ động đồng ý, không phải thứ
    /// bật sẵn rồi thông báo sau.
    pub realtime: bool,

    /// Tự sửa mà không hỏi, **chỉ** với lỗi chắc chắn.
    ///
    /// "Chắc chắn" ở đây có định nghĩa hẹp và kiểm chứng được: âm tiết không tồn tại
    /// trong tiếng Việt ([`writa_core::Confidence::Certain`]), precision ~100% vì nó
    /// là một phép tra bảng chứ không phải một phán đoán. Lỗi *real-word* thì tuyệt
    /// đối không tự sửa dù bật cờ này — người viết có thể chủ ý.
    ///
    /// Vẫn mặc định tắt: text bị đổi dưới tay mình mà không xin phép là chuyện đáng
    /// giật mình, kể cả khi sửa đúng.
    pub auto_fix: bool,

    pub real_word_margin: f64,
    pub flag_unattested: bool,
    /// Có hiện lỗi dấu câu / khoảng trắng không.
    ///
    /// Lọc ở lớp vỏ chứ không phải trong engine: [`writa_core::rules`] luôn chạy,
    /// và giữ nguyên như vậy để một tuỳ chọn UI không lặng lẽ đổi hành vi của lớp
    /// mà `writa-cli` và eval đang đo.
    pub check_punctuation: bool,
    pub check_capitalization: bool,
    pub typographic_style: bool,

    pub autostart: bool,

    /// Tự kiểm tra bản mới khi khởi động.
    ///
    /// Mặc định **bật**, và đây là ngoại lệ có chủ ý so với các tính năng chạm mạng
    /// khác của dự án (lớp AI mặc định tắt). Lý do: Writa cắm hook bàn phím và ghi text
    /// vào ô nhập của người khác, nên khi tìm ra lỗi làm hỏng text thì bản vá **phải**
    /// tới được tay user. Với công cụ chạy nền, "user tự đi tải bản mới" nghĩa là không
    /// bao giờ.
    ///
    /// Chỉ *kiểm tra* là tự động. Tải và cài thì user phải bấm — xem [`crate::update`].
    pub auto_update: bool,
    /// Từ Writa không bao giờ báo lỗi. Chữ thường, đã chuẩn hoá.
    pub personal_dict: Vec<String>,
    /// App user tự thêm, **cộng thêm** vào danh sách chặn cứng trong code.
    pub blocklist: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            paused: false,
            hotkey_check: "Ctrl+Alt+V".into(),
            hotkey_diacritic: "Ctrl+Alt+D".into(),
            hotkey_accept: "Ctrl+Alt+Space".into(),
            realtime: false,
            auto_fix: false,
            real_word_margin: DEFAULT_REAL_WORD_MARGIN,
            flag_unattested: false,
            check_punctuation: true,
            check_capitalization: false,
            typographic_style: false,
            autostart: false,
            auto_update: true,
            personal_dict: Vec::new(),
            blocklist: Vec::new(),
        }
    }
}

impl Settings {
    pub fn check_options(&self) -> CheckOptions {
        CheckOptions {
            flag_unattested: self.flag_unattested,
            // Luôn bật, và **không** có công tắc trong Settings.
            //
            // `CheckOptions::detect_real_word` tồn tại để `writa-cli scan --no-realword`
            // đo được phần đóng góp riêng của lớp này vào false-positive. Nó từng lọt
            // vào cài đặt của app, và hậu quả có thật: user tắt nó, `chia sẽ` /
            // `sữa lỗi` / `xử dụng` — nhóm lỗi người Việt mắc nhiều nhất — biến mất,
            // và app trông như **hỏng** chứ không như đã bị tắt bớt.
            //
            // Một núm đo lường không phải một tuỳ chọn người dùng. Ai muốn Writa nói ít
            // hơn thì đã có "Độ nhạy", vốn giữ được precision thay vì bỏ hẳn cả lớp.
            detect_real_word: true,
            real_word_margin: self.real_word_margin,
            rules: RuleOptions {
                check_capitalization: self.check_capitalization,
                typographic_style: self.typographic_style,
            },
            // Chi phí cho candidate hai phép sửa không lên UI: nó là hằng số chỉnh
            // theo số đo, không phải sở thích. Núm "Độ nhạy" đã là chỗ user điều
            // khiển độ mạnh tay rồi, thêm núm thứ hai chỉ làm loãng núm thứ nhất.
            ..CheckOptions::default()
        }
    }

    /// App này có bị user chặn không? (Danh sách cứng trong code xét riêng.)
    pub fn blocks(&self, exe: &str) -> bool {
        self.blocklist.iter().any(|b| b == exe)
    }

    pub fn ignores(&self, word: &str) -> bool {
        self.personal_dict.contains(&word.to_lowercase())
    }

    /// Dọn dữ liệu đến từ UI hoặc từ file sửa tay.
    ///
    /// Một ngưỡng âm hoặc một phím tắt rỗng không làm app sập, nhưng làm nó cư xử
    /// theo cách không giải thích được — rẻ hơn nhiều nếu chặn ngay tại cửa.
    pub fn sanitize(&mut self) {
        if !self.real_word_margin.is_finite() || self.real_word_margin <= 0.0 {
            self.real_word_margin = DEFAULT_REAL_WORD_MARGIN;
        }
        self.real_word_margin = self.real_word_margin.clamp(1.0, 30.0);

        let defaults = Settings::default();
        for (slot, fallback) in [
            (&mut self.hotkey_check, defaults.hotkey_check),
            (&mut self.hotkey_diacritic, defaults.hotkey_diacritic),
            (&mut self.hotkey_accept, defaults.hotkey_accept),
        ] {
            // `Ctrl + Alt + V` và `Ctrl+Alt+V` là cùng một phím tắt, nhưng chuỗi khác
            // nhau thì hiện ra khác nhau và so sánh cũng khác nhau — đủ để UI báo
            // "phím tắt bị chiếm" trong khi thật ra nó vừa đăng ký xong.
            *slot = slot.split('+').map(str::trim).collect::<Vec<_>>().join("+");
            if slot.is_empty() {
                *slot = fallback;
            }
        }

        for list in [&mut self.personal_dict, &mut self.blocklist] {
            for item in list.iter_mut() {
                *item = item.trim().to_lowercase();
            }
            list.retain(|w| !w.is_empty());
            list.sort();
            list.dedup();
        }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

/// Đọc thiết lập. Hỏng file thì quay về mặc định — không bao giờ chặn khởi động.
pub fn load(app: &AppHandle) -> Settings {
    let raw = path(app).and_then(|p| fs::read_to_string(p).ok());
    let parsed = raw
        .as_deref()
        // Cắt BOM UTF-8 trước khi parse. `serde_json` coi BOM là ký tự lạ và hỏng
        // toàn bộ file, mà hỏng ở đây là **im lặng** quay về mặc định — user sửa tay
        // file cấu hình bằng Notepad hoặc PowerShell 5.1 (cả hai ghi BOM) sẽ thấy mọi
        // thiết lập của mình bốc hơi mà không có lời giải thích nào.
        .map(|s| s.trim_start_matches('\u{feff}'))
        .and_then(|s| serde_json::from_str::<Settings>(s).ok());

    if raw.is_some() && parsed.is_none() {
        crate::debug::log(format_args!(
            "config: KHONG doc duoc settings.json, dung mac dinh"
        ));
    }

    let mut s = parsed.unwrap_or_default();
    s.sanitize();
    // Ghi lại thiết lập hiệu lực. Một lần "tại sao app không báo lỗi gì" đã tốn cả
    // lượt gỡ rối chỉ vì không ai thấy được app đang chạy với thiết lập nào.
    crate::debug::log(format_args!(
        "config: realtime={} autoFix={} margin={} unattested={} punctuation={} \
         hotkeys=[{} | {} | {}] dict={} blocklist={}",
        s.realtime,
        s.auto_fix,
        s.real_word_margin,
        s.flag_unattested,
        s.check_punctuation,
        s.hotkey_check,
        s.hotkey_diacritic,
        s.hotkey_accept,
        s.personal_dict.len(),
        s.blocklist.len(),
    ));
    s
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let Some(p) = path(app) else {
        return Err("không xác định được thư mục cấu hình".into());
    };
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("không tạo được {}: {e}", dir.display()))?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&p, raw).map_err(|e| format!("không ghi được {}: {e}", p.display()))
}
