//! L6 — Lớp AI, dạng hợp đồng thuần.
//!
//! # Ranh giới quan trọng nhất của cả dự án
//!
//! Module này **không có một dòng mạng nào**, và đó là chủ đích. Writa quảng cáo
//! "100% offline mặc định" — lời hứa đó chỉ đáng tin nếu nó *kiểm chứng được*, chứ
//! không phải vì tác giả nói vậy.
//!
//! Nên L6 chỉ định nghĩa **hợp đồng**: kiểu dữ liệu, cách dựng prompt, và trait
//! [`AiProvider`]. Phần gọi HTTP thật nằm ở crate riêng `writa-ai` mà desktop shell
//! nối vào. Hệ quả:
//!
//! - `writa-core` không có dependency mạng nào — kiểm chứng bằng
//!   [`tests::core_has_no_network_dependency`], test đọc chính `Cargo.toml` của crate.
//! - `writa-core` vẫn build được sang WASM (dùng lại cho VSCode extension, web demo).
//! - Cách dựng prompt test được mà không cần gọi API, không cần API key.
//!
//! # Chỉ chạy khi user chủ động bấm
//!
//! Không có đường nào để lớp này tự chạy. Nó không nằm trong [`crate::check`], không
//! có timer, không có prefetch. Desktop shell phải gọi tường minh sau một hành động
//! của user. Mọi thứ khác là vi phạm lời hứa privacy.

use std::fmt;

/// Việc muốn AI làm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiTask {
    /// Kiểm tra ngữ pháp — thứ mà L1–L5 không với tới được: sai cấu trúc câu,
    /// thiếu chủ ngữ, dùng từ nối sai, câu cụt.
    Grammar,
    /// Viết lại theo văn phong.
    Rewrite(Style),
    /// Giải thích vì sao một chỗ bị coi là sai. Dùng cho "Learning mode".
    Explain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Văn phong hành chính, công văn.
    Formal,
    /// Thân mật, tin nhắn.
    Casual,
    /// Ngắn gọn hơn, bỏ chữ thừa.
    Concise,
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Style::Formal => "trang trọng, phù hợp văn bản hành chính",
            Style::Casual => "thân mật, tự nhiên như tin nhắn",
            Style::Concise => "ngắn gọn hơn, bỏ chữ thừa mà giữ nguyên ý",
        })
    }
}

#[derive(Debug, Clone)]
pub struct AiRequest {
    pub text: String,
    pub task: AiTask,
    /// Những lỗi engine offline **đã** tìm ra.
    ///
    /// Gửi kèm để AI khỏi lặp lại chúng — vừa đỡ tốn token, vừa tránh cho user
    /// thấy cùng một lỗi hai lần với hai cách diễn đạt khác nhau.
    pub already_found: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSuggestion {
    /// Đoạn text gốc cần thay.
    pub original: String,
    pub replacement: String,
    /// Giải thích ngắn bằng tiếng Việt, để user học chứ không chỉ bấm chấp nhận.
    pub explanation: String,
}

#[derive(Debug)]
pub enum AiError {
    /// Chưa cấu hình API key.
    NoApiKey,
    /// Mạng lỗi.
    Network(String),
    /// API trả về lỗi. Kèm mã HTTP.
    Api { status: u16, message: String },
    /// Model từ chối xử lý yêu cầu (`stop_reason: "refusal"`).
    Refused { category: Option<String> },
    /// Không đọc được phản hồi.
    BadResponse(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::NoApiKey => write!(f, "chưa cấu hình API key"),
            AiError::Network(e) => write!(f, "lỗi mạng: {e}"),
            AiError::Api { status, message } => write!(f, "API lỗi {status}: {message}"),
            AiError::Refused { category } => match category {
                Some(c) => write!(f, "model từ chối xử lý ({c})"),
                None => write!(f, "model từ chối xử lý"),
            },
            AiError::BadResponse(e) => write!(f, "phản hồi không đọc được: {e}"),
        }
    }
}

impl std::error::Error for AiError {}

/// Nguồn cung cấp gợi ý AI.
///
/// `writa-core` chỉ định nghĩa trait; phần gọi mạng thật nằm ở `writa-ai`. Đây là
/// chỗ ranh giới offline/online cắt qua — và cắt ở mức crate chứ không phải mức
/// hàm, để "core không có mạng" là chuyện trình biên dịch bảo đảm, không phải
/// chuyện kỷ luật của người viết.
pub trait AiProvider {
    fn suggest(&self, request: &AiRequest) -> Result<Vec<AiSuggestion>, AiError>;
}

// ---------------------------------------------------------------------------
// Dựng prompt — thuần, test được, không cần mạng
// ---------------------------------------------------------------------------

/// Cặp prompt gửi lên model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// JSON Schema cho phản hồi. Dùng structured output nên không phải bóc tách văn
/// xuôi — model bị ràng buộc trả đúng hình dạng này.
pub const RESPONSE_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "suggestions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "original": { "type": "string" },
          "replacement": { "type": "string" },
          "explanation": { "type": "string" }
        },
        "required": ["original", "replacement", "explanation"],
        "additionalProperties": false
      }
    }
  },
  "required": ["suggestions"],
  "additionalProperties": false
}"#;

/// Dựng prompt cho một yêu cầu.
///
/// Điểm mấu chốt: prompt **nói rõ engine offline đã lo những gì**. Không nói thì
/// model sẽ báo lại chính những lỗi chính tả mà L1–L3 vừa bắt, và user thấy cùng
/// một lỗi hai lần với hai cách diễn đạt khác nhau — trông như engine tự mâu thuẫn.
pub fn build_prompt(req: &AiRequest) -> Prompt {
    let mut system = String::from(
        "Bạn là trợ lý viết tiếng Việt. Người dùng đang gõ văn bản tiếng Việt và \
         muốn cải thiện nó.\n\n\
         Một engine offline đã kiểm tra xong các lớp sau, ĐỪNG lặp lại:\n\
         - Chính tả cấp âm tiết (âm tiết không tồn tại trong tiếng Việt)\n\
         - Nhầm lẫn hỏi/ngã, s/x, ch/tr, r/d/gi, n/ng, t/c\n\
         - Dấu câu, khoảng trắng thừa, ngoặc không cân\n\n\
         Việc của bạn là những gì engine đó KHÔNG làm được: ngữ pháp, cấu trúc câu, \
         cách dùng từ, và văn phong.\n\n",
    );

    system.push_str(match &req.task {
        AiTask::Grammar => {
            "Nhiệm vụ: tìm lỗi NGỮ PHÁP và cách dùng từ. Ví dụ: thiếu chủ ngữ, \
             từ nối sai, câu cụt, lặp từ thừa, dùng sai cặp từ Hán-Việt.\n\
             Nếu văn bản đã ổn, trả về danh sách rỗng. Đừng bịa ra lỗi để có cái mà báo."
        }
        AiTask::Rewrite(_) => {
            "Nhiệm vụ: viết lại văn bản theo văn phong được yêu cầu, giữ nguyên ý.\n\
             Trả về MỘT gợi ý duy nhất, với `original` là toàn bộ văn bản gốc."
        }
        AiTask::Explain => {
            "Nhiệm vụ: giải thích vì sao mỗi chỗ được đánh dấu là sai, ngắn gọn và \
             dễ hiểu, để người dùng học được quy tắc chứ không chỉ bấm chấp nhận.\n\
             `replacement` là dạng đúng, `explanation` là lý do."
        }
    });

    if let AiTask::Rewrite(style) = &req.task {
        system.push_str(&format!("\n\nVăn phong cần đạt: {style}."));
    }

    system.push_str(
        "\n\nMọi `explanation` phải viết bằng tiếng Việt. `original` phải là đoạn text \
         XUẤT HIỆN NGUYÊN VĂN trong văn bản người dùng, để chương trình định vị được \
         chỗ cần thay.",
    );

    let mut user = String::new();
    if !req.already_found.is_empty() {
        user.push_str("Engine offline đã tìm ra các lỗi sau, bỏ qua chúng:\n");
        for f in &req.already_found {
            user.push_str("- ");
            user.push_str(f);
            user.push('\n');
        }
        user.push('\n');
    }
    user.push_str("Văn bản cần xử lý:\n\n");
    user.push_str(&req.text);

    Prompt { system, user }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `writa-core` KHÔNG được phụ thuộc bất kỳ crate mạng nào.
    ///
    /// Đây là lời hứa privacy chính của sản phẩm, nên nó phải do trình biên dịch
    /// canh chứ không phải do người viết nhớ. Test đọc thẳng `Cargo.toml` của crate,
    /// nên thêm nhầm một HTTP client là đỏ ngay.
    #[test]
    fn core_has_no_network_dependency() {
        const MANIFEST: &str = include_str!("../Cargo.toml");
        for crate_name in [
            "reqwest",
            "ureq",
            "hyper",
            "curl",
            "isahc",
            "attohttpc",
            "surf",
            "tokio",
            "async-std",
            "socket2",
            "rustls",
            "native-tls",
            "openssl",
        ] {
            assert!(
                !MANIFEST.contains(crate_name),
                "writa-core không được phụ thuộc `{crate_name}` — lớp AI phải nằm ở \
                 crate riêng để lời hứa 'zero network' kiểm chứng được"
            );
        }
    }

    fn req(task: AiTask) -> AiRequest {
        AiRequest {
            text: "Tôi đi học ở trường.".to_string(),
            task,
            already_found: Vec::new(),
        }
    }

    #[test]
    fn prompt_tells_the_model_what_the_offline_engine_already_did() {
        // Thiếu phần này thì AI báo lại đúng những lỗi L1-L3 vừa bắt, và user thấy
        // cùng một lỗi hai lần.
        let p = build_prompt(&req(AiTask::Grammar));
        assert!(p.system.contains("ĐỪNG lặp lại"));
        assert!(p.system.contains("hỏi/ngã"));
        assert!(p.system.contains("Chính tả cấp âm tiết"));
    }

    #[test]
    fn prompt_carries_already_found_errors() {
        let r = AiRequest {
            text: "Tôi muốn chia sẽ điều này".to_string(),
            task: AiTask::Grammar,
            already_found: vec!["sẽ → sẻ".to_string()],
        };
        let p = build_prompt(&r);
        assert!(p.user.contains("sẽ → sẻ"));
        assert!(p.user.contains("bỏ qua chúng"));
    }

    #[test]
    fn rewrite_prompt_names_the_style() {
        let p = build_prompt(&req(AiTask::Rewrite(Style::Formal)));
        assert!(p.system.contains("hành chính"), "{}", p.system);
        let p = build_prompt(&req(AiTask::Rewrite(Style::Casual)));
        assert!(p.system.contains("tin nhắn"));
    }

    #[test]
    fn prompt_requires_verbatim_original() {
        // Chương trình phải định vị được đoạn cần thay trong text gốc; nếu model
        // diễn giải lại `original` thì không tìm ra chỗ nào để thay.
        for task in [
            AiTask::Grammar,
            AiTask::Explain,
            AiTask::Rewrite(Style::Concise),
        ] {
            let p = build_prompt(&req(task));
            assert!(p.system.contains("NGUYÊN VĂN"));
        }
    }

    #[test]
    fn prompt_discourages_inventing_errors() {
        let p = build_prompt(&req(AiTask::Grammar));
        assert!(p.system.contains("Đừng bịa"));
    }

    #[test]
    fn schema_is_valid_json_shape() {
        // Kiểm tra thô mà đủ: schema phải khai báo đúng ba trường và khoá chặt
        // additionalProperties, nếu không structured output mất tác dụng ràng buộc.
        for needle in [
            "\"original\"",
            "\"replacement\"",
            "\"explanation\"",
            "\"additionalProperties\": false",
            "\"suggestions\"",
        ] {
            assert!(RESPONSE_SCHEMA.contains(needle), "schema thiếu {needle}");
        }
    }
}
