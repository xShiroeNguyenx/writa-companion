//! L6 — Gọi Claude API. **Crate duy nhất trong workspace có quyền ra mạng.**
//!
//! # Vì sao tách riêng crate
//!
//! Writa hứa "100% offline mặc định". Lời hứa đó chỉ đáng tin nếu kiểm chứng được,
//! nên ranh giới offline/online cắt ở mức **crate**, không phải mức hàm:
//! `writa-core` không có dependency mạng nào (canh bằng test đọc chính `Cargo.toml`
//! của nó), và toàn bộ HTTP nằm ở đây. Desktop shell nối hai bên lại qua trait
//! [`writa_core::ai::AiProvider`], và chỉ khi user bật.
//!
//! Rust chưa có SDK Anthropic chính thức, nên gọi HTTP thô theo đúng tài liệu.

use std::time::Duration;

use serde::Deserialize;
use writa_core::ai::{build_prompt, AiError, AiProvider, AiRequest, AiSuggestion, RESPONSE_SCHEMA};

const API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Phiên bản API. Hằng số này bắt buộc phải gửi trên mọi request.
const API_VERSION: &str = "2023-06-01";

/// Model mặc định.
///
/// Chất lượng quan trọng hơn giá ở đây: lớp này chỉ chạy khi user chủ động bấm,
/// nên tần suất thấp, còn ngữ pháp tiếng Việt thì tinh tế. Người dùng vẫn đổi được
/// qua [`ClaudeConfig::model`] — đây là mô hình BYO key, tiền là của họ.
pub const DEFAULT_MODEL: &str = "claude-opus-5";

/// Trần token đầu ra.
///
/// Không stream nên phải nằm dưới ngưỡng timeout HTTP của tài liệu (~16k). Gợi ý
/// sửa lỗi thì ngắn, nên đây đã là rộng rãi.
const MAX_TOKENS: u32 = 8192;

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// API key của chính user. Không bao giờ ghi log, không bao giờ gửi đi đâu khác.
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

impl ClaudeConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

pub struct ClaudeProvider {
    config: ClaudeConfig,
}

impl ClaudeProvider {
    pub fn new(config: ClaudeConfig) -> Self {
        Self { config }
    }
}

// ---------------------------------------------------------------------------
// Hình dạng phản hồi
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    stop_details: Option<StopDetails>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct StopDetails {
    #[serde(default)]
    category: Option<String>,
}

#[derive(Deserialize)]
struct Suggestions {
    suggestions: Vec<RawSuggestion>,
}

#[derive(Deserialize)]
struct RawSuggestion {
    original: String,
    replacement: String,
    explanation: String,
}

impl AiProvider for ClaudeProvider {
    fn suggest(&self, request: &AiRequest) -> Result<Vec<AiSuggestion>, AiError> {
        if self.config.api_key.trim().is_empty() {
            return Err(AiError::NoApiKey);
        }

        let prompt = build_prompt(request);
        let schema: serde_json::Value = serde_json::from_str(RESPONSE_SCHEMA)
            .map_err(|e| AiError::BadResponse(format!("schema hỏng: {e}")))?;

        // Cố ý KHÔNG gửi `temperature` / `top_p` / `top_k`: các model hiện tại từ
        // chối chúng với lỗi 400. Muốn điều chỉnh hành vi thì sửa prompt.
        let body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": MAX_TOKENS,
            "system": prompt.system,
            "messages": [{ "role": "user", "content": prompt.user }],
            // Structured output: model bị ràng buộc trả đúng schema, nên không phải
            // bóc tách văn xuôi và không có nhánh "parse thất bại thì đoán".
            "output_config": {
                "format": { "type": "json_schema", "schema": schema }
            }
        });

        let response = ureq::post(API_URL)
            .config()
            .timeout_global(Some(self.config.timeout))
            .build()
            .header("content-type", "application/json")
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", API_VERSION)
            .send_json(&body);

        let mut response = match response {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(status)) => {
                return Err(AiError::Api {
                    status,
                    // Cố tình KHÔNG kèm body lỗi: một số lỗi phản chiếu lại request,
                    // và request có chứa văn bản của user.
                    message: describe_status(status).to_string(),
                });
            }
            Err(e) => return Err(AiError::Network(e.to_string())),
        };

        let parsed: ApiResponse = response
            .body_mut()
            .read_json()
            .map_err(|e| AiError::BadResponse(e.to_string()))?;

        // PHẢI xét stop_reason trước khi đọc content: khi model từ chối, `content`
        // rỗng hoặc dở dang, và code đọc thẳng `content[0]` sẽ vỡ.
        if parsed.stop_reason.as_deref() == Some("refusal") {
            return Err(AiError::Refused {
                category: parsed.stop_details.and_then(|d| d.category),
            });
        }

        let text = parsed
            .content
            .iter()
            .find(|b| b.kind == "text")
            .and_then(|b| b.text.as_deref())
            .ok_or_else(|| AiError::BadResponse("phản hồi không có khối text".into()))?;

        let suggestions: Suggestions = serde_json::from_str(text)
            .map_err(|e| AiError::BadResponse(format!("JSON không khớp schema: {e}")))?;

        Ok(suggestions
            .suggestions
            .into_iter()
            .map(|s| AiSuggestion {
                original: s.original,
                replacement: s.replacement,
                explanation: s.explanation,
            })
            .collect())
    }
}

fn describe_status(status: u16) -> &'static str {
    match status {
        400 => "yêu cầu không hợp lệ",
        401 => "API key sai hoặc thiếu",
        403 => "API key không có quyền",
        404 => "không tìm thấy model — kiểm tra lại tên model",
        413 => "văn bản quá dài",
        429 => "vượt giới hạn tần suất, thử lại sau",
        500..=599 => "lỗi phía máy chủ, thử lại sau",
        _ => "lỗi không xác định",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_api_key_fails_before_touching_the_network() {
        // Không có key thì phải hỏng NGAY, không được thử gọi mạng.
        let provider = ClaudeProvider::new(ClaudeConfig::new("   "));
        let err = provider
            .suggest(&AiRequest {
                text: "test".into(),
                task: writa_core::ai::AiTask::Grammar,
                already_found: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, AiError::NoApiKey));
    }

    #[test]
    fn default_model_is_set() {
        let c = ClaudeConfig::new("k");
        assert_eq!(c.model, DEFAULT_MODEL);
        assert_eq!(c.with_model("claude-sonnet-5").model, "claude-sonnet-5");
    }

    #[test]
    fn schema_parses_as_json() {
        // Schema hỏng thì mọi request đều 400; bắt ở đây chứ không phải lúc chạy thật.
        let v: serde_json::Value = serde_json::from_str(RESPONSE_SCHEMA).unwrap();
        assert_eq!(v["type"], "object");
        assert_eq!(v["properties"]["suggestions"]["type"], "array");
    }

    #[test]
    fn error_messages_never_leak_user_text() {
        // Thông điệp lỗi hiện lên UI và có thể vào log. Chúng phải mô tả mã trạng
        // thái, không được chứa lại nội dung request.
        for status in [400, 401, 403, 404, 413, 429, 500, 503, 999] {
            let msg = describe_status(status);
            assert!(!msg.is_empty());
            assert!(
                !msg.contains("http"),
                "thông điệp lỗi không nên lộ chi tiết request"
            );
        }
    }

    #[test]
    fn refusal_is_detected_before_reading_content() {
        // Khi model từ chối, `content` rỗng — đọc thẳng content[0] sẽ panic.
        let json = r#"{"content":[],"stop_reason":"refusal","stop_details":{"category":"cyber"}}"#;
        let parsed: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.stop_reason.as_deref(), Some("refusal"));
        assert!(parsed.content.is_empty());
        assert_eq!(
            parsed.stop_details.unwrap().category.as_deref(),
            Some("cyber")
        );
    }

    #[test]
    fn parses_a_normal_response() {
        let json = r#"{
            "content": [{"type": "text", "text": "{\"suggestions\":[{\"original\":\"a\",\"replacement\":\"b\",\"explanation\":\"vì c\"}]}"}],
            "stop_reason": "end_turn"
        }"#;
        let parsed: ApiResponse = serde_json::from_str(json).unwrap();
        let text = parsed.content[0].text.as_deref().unwrap();
        let s: Suggestions = serde_json::from_str(text).unwrap();
        assert_eq!(s.suggestions[0].replacement, "b");
        assert_eq!(s.suggestions[0].explanation, "vì c");
    }
}
