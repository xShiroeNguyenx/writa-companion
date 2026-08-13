//! Hợp đồng dữ liệu với UI.
//!
//! Mọi kiểu ở đây phải khớp `ui/src/api.ts`. Dùng `camelCase` để phía TypeScript
//! viết theo quy ước của nó mà không phải đổi tên ở từng chỗ dùng.

use serde::{Deserialize, Serialize};

/// Hotkey nào đã kích hoạt lượt xem lại này.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Kiểm tra chính tả đoạn đang bôi đen.
    Check,
    /// Thêm dấu cho đoạn đang bôi đen.
    Diacritic,
}

/// Loại thay đổi — quyết định màu sắc và mức tin cậy hiển thị.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Invalid,
    Unattested,
    Confused,
    Punctuation,
    Capitalization,
    Diacritic,
}

/// Một chỗ Writa đề nghị sửa.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Định danh trong lượt này. UI gửi lại kèm quyết định của user.
    pub id: usize,
    /// Vị trí **byte** trong [`ReviewPayload::original`].
    pub start: usize,
    pub end: usize,
    pub kind: ChangeKind,
    pub from: String,
    /// Phương án thay thế, tốt nhất trước.
    ///
    /// **Có thể rỗng.** Engine phát hiện được lỗi mà không nghĩ ra cách sửa là
    /// chuyện thường (`nghiep` chẳng hạn). Giấu những chỗ đó đi thì user tưởng câu
    /// mình đúng — nên vẫn báo, chỉ là không sửa được.
    pub options: Vec<String>,
    pub certain: bool,
    /// Ngữ cảnh hai bên đã cắt ngắn, để user định vị lỗi trong câu.
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPayload {
    pub mode: Mode,
    /// Tên exe app đích, hoặc `"clipboard"`.
    pub app: String,
    pub original: String,
    pub changes: Vec<Change>,
    /// Lời nhắn thay cho danh sách lỗi — "chưa bôi đen đoạn nào", "app này không cho
    /// đọc". Có nó thì popup vẫn hiện: im lặng sau khi user bấm phím tắt là để họ
    /// tưởng app hỏng.
    pub notice: Option<String>,
}

/// Một thay đổi user đồng ý áp dụng.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: usize,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub ok: bool,
    /// Text sau khi áp dụng — luôn có, kể cả khi ghi thất bại, để user chép tay.
    pub text: String,
    /// Lý do thất bại, viết sẵn bằng tiếng Việt để hiện thẳng cho user.
    pub error: Option<String>,
}

impl ApplyOutcome {
    pub fn ok(text: String) -> Self {
        Self {
            ok: true,
            text,
            error: None,
        }
    }

    pub fn failed(text: String, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            text,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub version: String,
    pub syllables: usize,
    pub attested: usize,
    pub accepted_foreign: usize,
    pub compounds: usize,
    pub trigrams: usize,
    pub default_blocklist: Vec<String>,
}
