//! L5 — Luật dấu câu, khoảng trắng, viết hoa.
//!
//! Khác mọi lớp trước, lớp này **tất định**: không tần suất, không mô hình, không
//! ngưỡng. Một khoảng trắng đôi là một khoảng trắng đôi.
//!
//! # Vì sao vẫn phải cẩn thận
//!
//! Tất định không có nghĩa là an toàn. `1.000` và `T.P` trông y hệt lỗi "thiếu
//! khoảng trắng sau dấu chấm", còn `x  =  1` trong code trông y hệt "khoảng trắng
//! đôi". Nên mọi luật ở đây đều tôn trọng vùng bảo vệ của [`crate::token`], và luật
//! nào không phân biệt được ngữ cảnh thì để **tuỳ chọn tắt** thay vì đoán.
//!
//! # Ba mức
//!
//! - **Mặc định bật, `Certain`**: thuần cơ học, không cần hiểu nghĩa — khoảng trắng
//!   đôi, khoảng trắng trước dấu câu.
//! - **Mặc định bật, `Likely`**: đúng trong văn viết chuẩn nhưng có ngoại lệ hợp lý.
//! - **Mặc định tắt**: sở thích văn phong, không phải lỗi — `...` so với `…`, chuẩn
//!   hoá vị trí dấu thanh.

use std::ops::Range;

use crate::token::{Token, TokenKind};
use crate::{Confidence, Diagnostic, DiagnosticKind};

/// Tuỳ chọn cho lớp luật.
///
/// Cả hai mặc định `false` — xem lý do ở từng trường. Đó là hướng lệch chung của
/// dự án: lớp nào chưa chắc thì im lặng.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuleOptions {
    /// Báo khi câu mới không viết hoa chữ đầu.
    ///
    /// Mặc định **tắt**. Trong chat và tin nhắn, không viết hoa là chuẩn mực chứ
    /// không phải lỗi — bật mặc định thì Writa biến thành phiền toái ở đúng nơi
    /// người ta gõ nhiều nhất. Per-app profile sẽ bật nó cho Word và email.
    pub check_capitalization: bool,

    /// Đề xuất đổi `...` thành `…` và `--` thành `–`.
    ///
    /// Mặc định **tắt**: đây là sở thích văn phong, không phải lỗi chính tả.
    pub typographic_style: bool,
}

/// Dấu câu mà tiếng Việt **không** đặt khoảng trắng phía trước.
const NO_SPACE_BEFORE: [char; 7] = [',', '.', '!', '?', ';', ':', '%'];

/// Dấu câu cần có khoảng trắng phía sau khi còn chữ theo sau.
const SPACE_AFTER: [char; 5] = [',', ';', ':', '!', '?'];

const OPEN_CLOSE: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('«', '»')];

fn diag(span: Range<usize>, found: &str, fix: &str, conf: Confidence) -> Diagnostic {
    Diagnostic {
        span,
        kind: DiagnosticKind::Punctuation,
        found: found.to_string(),
        candidates: vec![fix.to_string()],
        confidence: conf,
    }
}

/// Vị trí này có nằm trong vùng đã được bảo vệ không?
///
/// URL, đường dẫn, code và số đều chứa dấu câu dày đặc và **không** tuân luật văn
/// bản. Bỏ bước này thì `https://a.b/c` biến thành một chuỗi báo lỗi.
fn protected_at(tokens: &[Token], pos: usize) -> bool {
    tokens
        .iter()
        .any(|t| t.protect.is_some() && t.span.contains(&pos))
}

/// Vị trí này kề với chữ số ở cả hai bên? (`1.000`, `3,14`, `10:30`)
fn between_digits(text: &str, pos: usize) -> bool {
    let before = text[..pos].chars().next_back();
    let after = text[pos..].chars().nth(1);
    before.is_some_and(|c| c.is_ascii_digit()) && after.is_some_and(|c| c.is_ascii_digit())
}

pub fn check(text: &str, tokens: &[Token], opts: RuleOptions) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    out.extend(double_spaces(text, tokens));
    out.extend(space_before_punctuation(text, tokens));
    out.extend(missing_space_after_punctuation(text, tokens));
    out.extend(unbalanced_pairs(text, tokens));
    if opts.check_capitalization {
        out.extend(sentence_capitalisation(text, tokens));
    }
    if opts.typographic_style {
        out.extend(typographic(text, tokens));
    }
    out.sort_by_key(|d| d.span.start);
    out
}

/// Hai khoảng trắng liền nhau trong cùng một dòng.
fn double_spaces(text: &str, tokens: &[Token]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].1 != ' ' {
            i += 1;
            continue;
        }
        let start = chars[i].0;
        let mut j = i;
        while j < chars.len() && chars[j].1 == ' ' {
            j += 1;
        }
        let end = chars.get(j).map_or(text.len(), |(k, _)| *k);
        if j - i >= 2 && !protected_at(tokens, start) {
            out.push(diag(
                start..end,
                &text[start..end],
                " ",
                Confidence::Certain,
            ));
        }
        i = j;
    }
    out
}

/// Khoảng trắng trước `,` `.` `!` `?` `;` `:` `%`.
fn space_before_punctuation(text: &str, tokens: &[Token]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        if !NO_SPACE_BEFORE.contains(&c) || protected_at(tokens, i) {
            continue;
        }
        let ws_len: usize = text[..i]
            .chars()
            .rev()
            .take_while(|c| *c == ' ')
            .map(char::len_utf8)
            .sum();
        if ws_len == 0 {
            continue;
        }
        // Đầu dòng thì không phải lỗi — có thể là danh sách hoặc thụt đầu dòng.
        if text[..i - ws_len]
            .chars()
            .next_back()
            .is_none_or(|p| p == '\n')
        {
            continue;
        }
        out.push(diag(
            i - ws_len..i + c.len_utf8(),
            &text[i - ws_len..i + c.len_utf8()],
            &c.to_string(),
            Confidence::Certain,
        ));
    }
    out
}

/// Thiếu khoảng trắng sau dấu câu: `xin chào,bạn`.
fn missing_space_after_punctuation(text: &str, tokens: &[Token]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        if !SPACE_AFTER.contains(&c) || protected_at(tokens, i) {
            continue;
        }
        // `1,5` `10:30` — dấu nằm giữa hai chữ số là ký hiệu, không phải dấu câu.
        if between_digits(text, i) {
            continue;
        }
        let Some(next) = text[i + c.len_utf8()..].chars().next() else {
            continue;
        };
        // Chỉ báo khi ngay sau là CHỮ. Dấu câu nối tiếp (`?!`, `),`) là bình thường.
        if !next.is_alphabetic() {
            continue;
        }
        let end = i + c.len_utf8() + next.len_utf8();
        out.push(diag(
            i..end,
            &text[i..end],
            &format!("{c} {next}"),
            Confidence::Certain,
        ));
    }
    out
}

/// Ngoặc hoặc nháy kép không cân.
fn unbalanced_pairs(text: &str, tokens: &[Token]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (open, close) in OPEN_CLOSE {
        let n_open = text
            .match_indices(open)
            .filter(|(i, _)| !protected_at(tokens, *i))
            .count();
        let n_close = text
            .match_indices(close)
            .filter(|(i, _)| !protected_at(tokens, *i))
            .count();
        if n_open == n_close {
            continue;
        }
        let (missing, present) = if n_open > n_close {
            (close, open)
        } else {
            (open, close)
        };
        if let Some((i, _)) = text.match_indices(present).next_back() {
            out.push(Diagnostic {
                span: i..i + present.len_utf8(),
                kind: DiagnosticKind::Punctuation,
                found: format!("{present} không có {missing} đi kèm"),
                candidates: vec![missing.to_string()],
                confidence: Confidence::Likely,
            });
        }
    }
    // Nháy kép thẳng: chỉ báo khi lẻ.
    let quotes: Vec<usize> = text
        .match_indices('"')
        .map(|(i, _)| i)
        .filter(|i| !protected_at(tokens, *i))
        .collect();
    if quotes.len() % 2 == 1 {
        if let Some(&i) = quotes.last() {
            out.push(Diagnostic {
                span: i..i + 1,
                kind: DiagnosticKind::Punctuation,
                found: "nháy kép lẻ".to_string(),
                candidates: vec!["\"".to_string()],
                confidence: Confidence::Likely,
            });
        }
    }
    out
}

/// Chữ đầu câu không viết hoa. Chỉ chạy khi bật.
fn sentence_capitalisation(text: &str, tokens: &[Token]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut expect_capital = true;

    for tok in tokens {
        match tok.kind {
            TokenKind::Word => {
                if expect_capital && tok.protect.is_none() {
                    let raw = tok.text(text);
                    if let Some(first) = raw.chars().next() {
                        if first.is_lowercase() {
                            let upper: String = first.to_uppercase().collect();
                            out.push(Diagnostic {
                                span: tok.span.clone(),
                                kind: DiagnosticKind::Capitalization,
                                found: raw.to_string(),
                                candidates: vec![format!("{upper}{}", &raw[first.len_utf8()..])],
                                confidence: Confidence::Likely,
                            });
                        }
                    }
                }
                expect_capital = false;
            }
            TokenKind::Number | TokenKind::Other => expect_capital = false,
            TokenKind::Punct => {
                let c = tok.text(text).chars().next().unwrap_or(' ');
                if matches!(c, '.' | '!' | '?' | '…') {
                    expect_capital = true;
                }
            }
            TokenKind::Space => {
                if tok.text(text).contains('\n') {
                    expect_capital = true;
                }
            }
        }
    }
    out
}

/// Sở thích kiểu chữ. Chỉ chạy khi bật.
fn typographic(text: &str, tokens: &[Token]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (pat, fix) in [("...", "…"), ("--", "–")] {
        for (i, _) in text.match_indices(pat) {
            if protected_at(tokens, i) {
                continue;
            }
            out.push(Diagnostic {
                span: i..i + pat.len(),
                kind: DiagnosticKind::Punctuation,
                found: pat.to_string(),
                candidates: vec![fix.to_string()],
                confidence: Confidence::Likely,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::tokenize;

    fn run(text: &str) -> Vec<(String, Vec<String>)> {
        let toks = tokenize(text);
        check(text, &toks, RuleOptions::default())
            .into_iter()
            .map(|d| (d.found, d.candidates))
            .collect()
    }

    fn run_with(text: &str, opts: RuleOptions) -> Vec<String> {
        let toks = tokenize(text);
        check(text, &toks, opts)
            .into_iter()
            .map(|d| d.found)
            .collect()
    }

    #[test]
    fn catches_double_spaces() {
        assert_eq!(run("xin  chào").len(), 1);
        assert_eq!(run("xin   chào")[0].1, vec![" ".to_string()]);
        assert!(run("xin chào").is_empty());
    }

    #[test]
    fn catches_space_before_punctuation() {
        assert_eq!(run("xin chào , bạn").len(), 1);
        assert_eq!(run("thật vậy !").len(), 1);
        assert!(run("xin chào, bạn").is_empty());
    }

    #[test]
    fn catches_missing_space_after_punctuation() {
        let got = run("xin chào,bạn");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, vec![", b".to_string()]);
    }

    #[test]
    fn does_not_touch_numbers() {
        // `1.000`, `3,14`, `10:30` là ký hiệu chứ không phải dấu câu.
        for s in [
            "giá 1.000 đồng",
            "số pi là 3,14",
            "lúc 10:30 sáng",
            "tăng 5% một năm",
        ] {
            assert!(run(s).is_empty(), "báo oan trong {s:?} → {:?}", run(s));
        }
    }

    #[test]
    fn does_not_touch_protected_spans() {
        // URL và đường dẫn đầy dấu câu và không theo luật văn bản.
        for s in [
            "xem https://vi.wikipedia.org/wiki/Tiếng_Việt nhé",
            "mở D:\\NGUYENKHANH\\file.txt rồi",
            "gửi khanh.nguyen@rivercrane.com.vn nhé",
            "chạy `a  =  1` trong terminal",
        ] {
            assert!(run(s).is_empty(), "báo oan trong {s:?} → {:?}", run(s));
        }
    }

    #[test]
    fn catches_unbalanced_brackets() {
        assert_eq!(run("đây là (ví dụ").len(), 1);
        assert!(run("đây là (ví dụ)").is_empty());
        assert_eq!(run("anh ấy nói \"xin chào").len(), 1);
        assert!(run("anh ấy nói \"xin chào\"").is_empty());
    }

    #[test]
    fn capitalisation_is_off_by_default() {
        // Trong chat, không viết hoa là chuẩn mực chứ không phải lỗi.
        assert!(run("xin chào. bạn khoẻ không?").is_empty());

        let opts = RuleOptions {
            check_capitalization: true,
            ..Default::default()
        };
        let got = run_with("xin chào. bạn khoẻ không?", opts);
        assert_eq!(got, vec!["xin".to_string(), "bạn".to_string()]);
    }

    #[test]
    fn typographic_style_is_off_by_default() {
        // Sở thích văn phong, không phải lỗi.
        assert!(run("đợi đã... rồi tính").is_empty());

        let opts = RuleOptions {
            typographic_style: true,
            ..Default::default()
        };
        assert_eq!(run_with("đợi đã... rồi tính", opts).len(), 1);
    }

    #[test]
    fn spans_are_usable_for_replacement() {
        let src = "xin chào , bạn";
        let toks = tokenize(src);
        let ds = check(src, &toks, RuleOptions::default());
        let mut fixed = src.to_string();
        for d in ds.iter().rev() {
            fixed.replace_range(d.span.clone(), &d.candidates[0]);
        }
        assert_eq!(fixed, "xin chào, bạn");
    }

    #[test]
    fn stays_silent_on_clean_text() {
        for s in [
            "Tôi yêu tiếng Việt.",
            "Xin chào, rất vui được gặp bạn!",
            "Anh ấy hỏi: \"Bạn khoẻ không?\"",
            "Giá là 1.000.000 đồng (đã gồm thuế).",
            "Danh sách: một, hai, ba.",
        ] {
            assert!(run(s).is_empty(), "báo oan trong {s:?} → {:?}", run(s));
        }
    }
}
