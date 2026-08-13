//! L0b — Tách token và mặt nạ vùng bảo vệ.
//!
//! # Quyết định thiết kế then chốt: tokenize trên text GỐC
//!
//! Chuẩn hoá NFD → NFC **làm dịch byte offset** (`e`+U+0302+U+0301 dài 5 byte,
//! `ế` dài 3 byte). Nếu tokenize trên text đã chuẩn hoá thì offset trả về không
//! còn trỏ đúng vào text gốc, và `SendInput` sẽ thay sai đoạn trong app đích —
//! lỗi này phá text của user, không phải chỉ báo sai.
//!
//! Nên: **tokenize text gốc, chuẩn hoá từng token**. Mỗi [`Token`] giữ span theo
//! byte của text gốc, cộng thêm dạng đã chuẩn hoá để tra cứu. Ranh giới token bền
//! với NFC/NFD vì dấu tổ hợp được tính là ký tự nối từ.
//!
//! # Vùng bảo vệ — vì sao đây là hạng mục sống còn
//!
//! Tool chạy nền mà gạch đỏ vào URL, email, hay `Đắk Lắk` sẽ bị tắt trong 5 phút.
//! Với Writa, **precision quan trọng hơn recall rất nhiều**, nên lớp này lệch hẳn
//! về phía im lặng: thà bỏ sót lỗi hơn là báo oan.
//!
//! Danh sách dưới đây không phải suy đoán — nó đến từ vòng verify đối chiếu corpus
//! viwiki, nơi các token bị loại nhiều nhất đều thuộc đúng những nhóm này:
//! viết tắt (`HĐND`, `SĐD`), tên riêng ngoại (`México`, `Napoléon`), địa danh dân
//! tộc (`Đắk`, `Lắk`, `Krông`), từ phiên âm khoa học (`vectơ`, `nitơ`, `hiđrô`).

use std::ops::Range;

use crate::normalize::{is_combining_mark, is_invisible, is_latin_letter, normalize_for_lookup};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Ứng viên âm tiết tiếng Việt — thứ duy nhất engine kiểm tra.
    Word,
    Number,
    Punct,
    Space,
    Other,
}

/// Lý do một token không được kiểm tra. Giữ lý do (chứ không chỉ cờ boolean) để
/// UI giải thích được cho user vì sao chỗ đó bị bỏ qua, và để debug false-negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectReason {
    Url,
    Email,
    Path,
    Mention,
    Hashtag,
    /// Trong dấu backtick hoặc dòng thụt đầu ≥4 space — coi là code.
    Code,
    /// Lẫn chữ và số: `A4`, `3D`, `COVID19`, mã sản phẩm.
    MixedAlnum,
    /// Viết tắt toàn chữ hoa từ 2 ký tự: `HĐND`, `VN`, `SĐD`.
    Acronym,
    Number,
    /// Viết hoa giữa câu — gần như luôn là tên riêng: `México`, `Đắk`, `Krông`.
    ProperNoun,
    /// Chứa chữ ngoài hệ Latin: `λ`, `α`, `Кириллица`, `漢字`.
    ///
    /// Không thể là lỗi chính tả tiếng Việt, nên không bao giờ báo.
    NonLatinScript,
}

#[derive(Debug, Clone)]
pub struct Token {
    /// Span theo **byte trong text gốc**. Dùng trực tiếp để thay thế.
    pub span: Range<usize>,
    pub kind: TokenKind,
    /// NFC + chữ thường + đã bỏ ký tự vô hình. Chỉ điền cho [`TokenKind::Word`].
    pub normalized: String,
    pub protect: Option<ProtectReason>,
}

impl Token {
    /// Đoạn text gốc của token này.
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.span.clone()]
    }

    /// Token này có được đưa xuống các lớp kiểm tra không?
    pub fn checkable(&self) -> bool {
        self.kind == TokenKind::Word && self.protect.is_none()
    }
}

fn is_word_char(c: char) -> bool {
    // Dấu tổ hợp phải tính là ký tự nối từ, nếu không thì ở dạng NFD một chữ
    // bị xé thành nhiều token. Ký tự vô hình cũng vậy: nó lọt vào giữa từ khi
    // copy từ web, và ta muốn cả từ nằm trong MỘT token để thay được cả cụm.
    c.is_alphabetic() || is_combining_mark(c) || is_invisible(c)
}

fn is_sentence_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…' | ':' | ';')
}

// ---------------------------------------------------------------------------
// Vùng bảo vệ
// ---------------------------------------------------------------------------

/// Ký tự được phép xuất hiện trong URL / email / đường dẫn sau khi đã bắt được
/// điểm neo. Cắt ở khoảng trắng là đủ; dấu câu cuối được gọt riêng bên dưới.
fn is_uri_char(c: char) -> bool {
    !c.is_whitespace()
}

/// Gọt dấu câu ở cuối một URL/đường dẫn bắt được.
///
/// `Xem tại https://vi.wikipedia.org/wiki/Việt_Nam.` — dấu chấm cuối là dấu câu
/// của câu, không phải phần của URL.
fn trim_trailing_punct(src: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let tail = &src[start..end];
        let last = match tail.chars().next_back() {
            Some(c) => c,
            None => break,
        };
        if matches!(
            last,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '"' | '\'' | '»' | '…'
        ) {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn char_before(src: &str, i: usize) -> Option<char> {
    src[..i].chars().next_back()
}

/// Quét ra mọi vùng phải bỏ qua. Kết quả đã sắp theo `start` và gộp vùng chồng nhau.
fn protected_ranges(src: &str) -> Vec<(Range<usize>, ProtectReason)> {
    let mut out: Vec<(Range<usize>, ProtectReason)> = Vec::new();

    // --- code fence ``` ... ``` ---
    // Làm trước tiên: nội dung bên trong không được các bộ nhận diện khác chạm vào.
    let mut fences: Vec<usize> = src.match_indices("```").map(|(i, _)| i).collect();
    fences.dedup();
    let mut fenced: Vec<Range<usize>> = Vec::new();
    for pair in fences.chunks_exact(2) {
        let (a, b) = (pair[0], pair[1] + 3);
        fenced.push(a..b);
        out.push((a..b, ProtectReason::Code));
    }
    let in_fence = |i: usize| fenced.iter().any(|r| r.contains(&i));

    // --- backtick đơn, trong cùng một dòng ---
    let ticks: Vec<usize> = src
        .match_indices('`')
        .map(|(i, _)| i)
        .filter(|i| !in_fence(*i))
        .collect();
    let mut k = 0;
    while k + 1 < ticks.len() {
        let (a, b) = (ticks[k], ticks[k + 1]);
        if src[a..b].contains('\n') {
            k += 1; // backtick lẻ cuối dòng — không ghép
            continue;
        }
        out.push((a..b + 1, ProtectReason::Code));
        k += 2;
    }

    // --- dòng thụt đầu >= 4 space hoặc tab ---
    // Trong chat/email người ta không thụt đầu dòng kiểu này; gần như luôn là code.
    let mut line_start = 0usize;
    for line in src.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        if !body.trim().is_empty() && (body.starts_with("    ") || body.starts_with('\t')) {
            out.push((line_start..line_start + body.len(), ProtectReason::Code));
        }
        line_start += line.len();
    }

    // --- URL ---
    for pat in ["https://", "http://", "ftp://", "www."] {
        for (i, _) in src.match_indices(pat) {
            if in_fence(i) {
                continue;
            }
            let end = i + src[i..]
                .find(|c: char| !is_uri_char(c))
                .unwrap_or(src.len() - i);
            out.push((i..trim_trailing_punct(src, i, end), ProtectReason::Url));
        }
    }

    // --- email ---
    // Phải chạy TRƯỚC @mention: `a@b.com` là email, không phải mention của `b.com`.
    let mut emails: Vec<Range<usize>> = Vec::new();
    for (at, _) in src.match_indices('@') {
        let local_start = src[..at]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
            .last()
            .map(|(i, _)| i);
        let Some(local_start) = local_start else {
            continue;
        };
        let after = at + 1;
        let domain_len = src[after..]
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-')))
            .unwrap_or(src.len() - after);
        let domain = &src[after..after + domain_len];
        // Bắt buộc có dấu chấm trong domain, và có ký tự sau dấu chấm cuối.
        if let Some(dot) = domain.rfind('.') {
            if dot + 1 < domain.len() {
                let end = trim_trailing_punct(src, local_start, after + domain_len);
                emails.push(local_start..end);
                out.push((local_start..end, ProtectReason::Email));
            }
        }
    }
    let in_email = |i: usize| emails.iter().any(|r| r.contains(&i));

    // --- @mention và #hashtag ---
    for (marker, reason) in [('@', ProtectReason::Mention), ('#', ProtectReason::Hashtag)] {
        for (i, _) in src.match_indices(marker) {
            if in_fence(i) || in_email(i) {
                continue;
            }
            // Chỉ tính khi đứng đầu từ, tránh bắt `e@f` giữa chuỗi.
            if char_before(src, i).is_some_and(|c| !c.is_whitespace() && c != '(') {
                continue;
            }
            let after = i + marker.len_utf8();
            let len = src[after..]
                .find(|c: char| !(is_word_char(c) || c.is_ascii_digit() || c == '_'))
                .unwrap_or(src.len() - after);
            if len > 0 {
                out.push((i..after + len, reason));
            }
        }
    }

    // --- đường dẫn ---
    // Cố ý bảo thủ: chỉ bắt ổ đĩa `D:\`, UNC `\\srv`, và đường dẫn tương đối
    // `./` `../`. KHÔNG bắt mọi chuỗi có `/`, vì `và/hoặc`, `nam/nữ` là văn bản
    // bình thường và phải được kiểm tra.
    let push_path = |i: usize, out: &mut Vec<(Range<usize>, ProtectReason)>| {
        let end = i + src[i..].find(char::is_whitespace).unwrap_or(src.len() - i);
        out.push((i..trim_trailing_punct(src, i, end), ProtectReason::Path));
    };
    for (i, c) in src.char_indices() {
        // Đường dẫn luôn bắt đầu ở đầu từ; giữa từ thì không tính.
        if in_fence(i) || !char_before(src, i).is_none_or(char::is_whitespace) {
            continue;
        }
        let rest = &src[i..];

        // Ổ đĩa Windows: `D:\…` hoặc `D:/…`
        if c.is_ascii_alphabetic() {
            let mut after = rest.chars().skip(1);
            if after.next() == Some(':') && matches!(after.next(), Some('\\' | '/')) {
                push_path(i, &mut out);
                continue;
            }
        }

        // UNC `\\server\…` hoặc đường dẫn tương đối `./` `../`
        if rest.starts_with("\\\\") || rest.starts_with("./") || rest.starts_with("../") {
            push_path(i, &mut out);
        }
    }

    out.sort_by_key(|(r, _)| (r.start, r.end));
    out
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// Tách `src` thành token, kèm mặt nạ vùng bảo vệ.
///
/// Span của mọi token trỏ vào **text gốc**, dùng trực tiếp để thay thế được.
pub fn tokenize(src: &str) -> Vec<Token> {
    let ranges = protected_ranges(src);
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut tokens: Vec<Token> = Vec::new();

    // Bám theo vị trí đầu câu để nhận ra viết hoa giữa câu = tên riêng.
    let mut at_sentence_start = true;
    let mut i = 0usize;

    while i < chars.len() {
        let (start, c) = chars[i];

        if c.is_whitespace() {
            let mut j = i;
            let mut saw_newline = false;
            while j < chars.len() && chars[j].1.is_whitespace() {
                saw_newline |= chars[j].1 == '\n';
                j += 1;
            }
            let end = chars.get(j).map_or(src.len(), |(k, _)| *k);
            tokens.push(Token {
                span: start..end,
                kind: TokenKind::Space,
                normalized: String::new(),
                protect: None,
            });
            if saw_newline {
                at_sentence_start = true;
            }
            i = j;
            continue;
        }

        if is_word_char(c) || c.is_ascii_digit() {
            let mut j = i;
            let mut has_alpha = false;
            let mut has_digit = false;
            while j < chars.len() {
                let ch = chars[j].1;
                if is_word_char(ch) {
                    has_alpha |= ch.is_alphabetic();
                    j += 1;
                } else if ch.is_ascii_digit() {
                    has_digit = true;
                    j += 1;
                } else {
                    break;
                }
            }
            let end = chars.get(j).map_or(src.len(), |(k, _)| *k);
            let raw = &src[start..end];

            let (kind, mut protect) = match (has_alpha, has_digit) {
                (true, true) => (TokenKind::Word, Some(ProtectReason::MixedAlnum)),
                (false, true) => (TokenKind::Number, Some(ProtectReason::Number)),
                (true, false) => (TokenKind::Word, None),
                // Chỉ gồm dấu tổ hợp / ký tự vô hình đứng trơ trọi.
                (false, false) => (TokenKind::Other, None),
            };

            if protect.is_none() && kind == TokenKind::Word {
                let letters: Vec<char> = raw.chars().filter(|c| c.is_alphabetic()).collect();
                if letters.iter().any(|c| !is_latin_letter(*c)) {
                    // Xét trước hai luật dưới: một token Hy Lạp/Kirin toàn chữ hoa
                    // không phải "viết tắt", nó chỉ là chữ viết khác.
                    protect = Some(ProtectReason::NonLatinScript);
                } else if letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase()) {
                    protect = Some(ProtectReason::Acronym);
                } else if !at_sentence_start && letters.first().is_some_and(|c| c.is_uppercase()) {
                    protect = Some(ProtectReason::ProperNoun);
                }
            }

            tokens.push(Token {
                span: start..end,
                kind,
                normalized: if kind == TokenKind::Word {
                    normalize_for_lookup(raw)
                } else {
                    String::new()
                },
                protect,
            });
            if matches!(kind, TokenKind::Word | TokenKind::Number) {
                at_sentence_start = false;
            }
            i = j;
            continue;
        }

        // Ký tự đơn: dấu câu hoặc thứ khác.
        let end = chars.get(i + 1).map_or(src.len(), |(k, _)| *k);
        let kind = if c.is_alphanumeric() {
            TokenKind::Other
        } else {
            TokenKind::Punct
        };
        tokens.push(Token {
            span: start..end,
            kind,
            normalized: String::new(),
            protect: None,
        });
        if is_sentence_terminator(c) {
            at_sentence_start = true;
        }
        i += 1;
    }

    apply_protected_ranges(&mut tokens, &ranges);
    tokens
}

/// Gắn lý do bảo vệ cho token nào chồng lấn vùng bảo vệ.
///
/// Token và vùng đều đã sắp tăng dần nên chạy một lượt bằng con trỏ trượt,
/// không cần so từng cặp.
fn apply_protected_ranges(tokens: &mut [Token], ranges: &[(Range<usize>, ProtectReason)]) {
    if ranges.is_empty() {
        return;
    }
    let mut cursor = 0usize;
    for tok in tokens.iter_mut() {
        // Bỏ qua các vùng đã nằm hoàn toàn phía trước token hiện tại.
        while cursor < ranges.len() && ranges[cursor].0.end <= tok.span.start {
            cursor += 1;
        }
        // Vùng có thể chồng nhau, nên phải nhìn xa hơn con trỏ.
        for (range, reason) in &ranges[cursor..] {
            if range.start >= tok.span.end {
                break;
            }
            if range.start < tok.span.end && tok.span.start < range.end {
                if tok.protect.is_none() {
                    tok.protect = Some(*reason);
                }
                break;
            }
        }
    }
}

/// Chỉ các token cần kiểm tra chính tả — đây là thứ L1 tiêu thụ.
pub fn checkable_words(src: &str) -> Vec<Token> {
    tokenize(src).into_iter().filter(Token::checkable).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Lấy danh sách dạng chuẩn hoá của các token sẽ được kiểm tra.
    fn checked(src: &str) -> Vec<String> {
        checkable_words(src)
            .into_iter()
            .map(|t| t.normalized)
            .collect()
    }

    /// Lấy (text gốc, lý do bảo vệ) của các token bị bỏ qua.
    fn skipped(src: &str) -> Vec<(String, ProtectReason)> {
        tokenize(src)
            .into_iter()
            .filter(|t| matches!(t.kind, TokenKind::Word | TokenKind::Number))
            .filter_map(|t| t.protect.map(|r| (t.text(src).to_string(), r)))
            .collect()
    }

    #[test]
    fn spans_point_into_the_original_text() {
        // Đây là bất biến quan trọng nhất của lớp này: nếu span sai thì
        // SendInput thay sai đoạn và phá text của user.
        let src = "Tôi viết sai chinhs tả.";
        for tok in tokenize(src) {
            assert!(
                src.get(tok.span.clone()).is_some(),
                "span {:?} không hợp lệ",
                tok.span
            );
        }
        let words = checkable_words(src);
        let sai = words.iter().find(|t| t.normalized == "chinhs").unwrap();
        assert_eq!(sai.text(src), "chinhs");
    }

    #[test]
    fn spans_stay_correct_for_nfd_input() {
        // "tiếng" dạng tổ hợp: 7 char / 9 byte (mỗi dấu tổ hợp 2 byte).
        // Dạng dựng sẵn chỉ 5 char / 7 byte — chênh 2 byte là lý do KHÔNG được
        // tokenize trên text đã chuẩn hoá.
        let src = "tie\u{0302}\u{0301}ng Việt";
        let words = checkable_words(src);
        assert_eq!(words[0].normalized, "tiếng");
        assert_eq!(words[0].text(src), "tie\u{0302}\u{0301}ng");
        assert_eq!(words[0].span, 0..9);
        assert_eq!("tiếng".len(), 7, "dạng NFC ngắn hơn dạng NFD");
    }

    #[test]
    fn combining_marks_do_not_split_words() {
        let src = "tie\u{0302}\u{0301}ng";
        assert_eq!(tokenize(src).len(), 1, "dấu tổ hợp bị coi là ranh giới từ");
    }

    #[test]
    fn zero_width_space_inside_word_stays_one_token() {
        let src = "tiế\u{200B}ng";
        let words = checkable_words(src);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].normalized, "tiếng");
        // Cả cụm nằm trong một span nên thay thế được trọn vẹn
        assert_eq!(words[0].text(src), "tiế\u{200B}ng");
    }

    #[test]
    fn protects_urls() {
        let src = "Xem tại https://vi.wikipedia.org/wiki/Tiếng_Việt.";
        assert_eq!(checked(src), vec!["xem", "tại"]);
        let s = skipped(src);
        assert!(s
            .iter()
            .any(|(t, r)| t.starts_with("https") && *r == ProtectReason::Url));
        // Dấu chấm cuối câu không được tính vào URL
        assert!(src.ends_with('.'));
    }

    #[test]
    fn protects_www_without_scheme() {
        assert_eq!(
            checked("truy cập www.example.com nhé"),
            vec!["truy", "cập", "nhé"]
        );
    }

    #[test]
    fn protects_email_and_prefers_it_over_mention() {
        let src = "gửi cho khanh.nguyen@rivercrane.com.vn nha";
        assert_eq!(checked(src), vec!["gửi", "cho", "nha"]);
        assert!(skipped(src).iter().any(|(_, r)| *r == ProtectReason::Email));
    }

    #[test]
    fn protects_mention_and_hashtag() {
        let src = "chào @khanh xem #tiengviet đi";
        assert_eq!(checked(src), vec!["chào", "xem", "đi"]);
        let reasons: Vec<_> = skipped(src).into_iter().map(|(_, r)| r).collect();
        assert!(reasons.contains(&ProtectReason::Mention));
        assert!(reasons.contains(&ProtectReason::Hashtag));
    }

    #[test]
    fn protects_code_in_backticks_and_fences() {
        assert_eq!(checked("dùng `cargo buildd` nhé"), vec!["dùng", "nhé"]);
        assert_eq!(
            checked("xem\n```\nlet x = sai;\n```\nxong"),
            vec!["xem", "xong"]
        );
    }

    #[test]
    fn protects_indented_lines() {
        assert_eq!(checked("mã:\n    let saii = 1;\nhết"), vec!["mã", "hết"]);
    }

    #[test]
    fn protects_windows_and_relative_paths() {
        let src = "mở D:\\NGUYENKHANH\\filee.txt rồi ../src/mainn.rs";
        assert_eq!(checked(src), vec!["mở", "rồi"]);
    }

    #[test]
    fn does_not_treat_ordinary_slashes_as_paths() {
        // `và/hoặc`, `nam/nữ` là văn bản bình thường — PHẢI được kiểm tra.
        assert_eq!(checked("nam/nữ và/hoặc"), vec!["nam", "nữ", "và", "hoặc"]);
    }

    #[test]
    fn protects_acronyms_and_mixed_alphanumerics() {
        let src = "HĐND ban hành QĐ số 12 khổ A4 về COVID19";
        let reasons: Vec<_> = skipped(src).into_iter().collect();
        assert!(reasons
            .iter()
            .any(|(t, r)| t == "HĐND" && *r == ProtectReason::Acronym));
        assert!(reasons
            .iter()
            .any(|(t, r)| t == "QĐ" && *r == ProtectReason::Acronym));
        assert!(reasons
            .iter()
            .any(|(t, r)| t == "A4" && *r == ProtectReason::MixedAlnum));
        assert!(reasons
            .iter()
            .any(|(t, r)| t == "COVID19" && *r == ProtectReason::MixedAlnum));
        assert!(reasons
            .iter()
            .any(|(t, r)| t == "12" && *r == ProtectReason::Number));
        assert_eq!(checked(src), vec!["ban", "hành", "số", "khổ", "về"]);
    }

    #[test]
    fn protects_proper_nouns_mid_sentence() {
        // Đúng nhóm token mà vòng verify corpus thấy bị loại nhiều nhất.
        let src = "Tôi đến México rồi qua Đắk Lắk và Krông Nô.";
        let names: Vec<String> = skipped(src)
            .into_iter()
            .filter(|(_, r)| *r == ProtectReason::ProperNoun)
            .map(|(t, _)| t)
            .collect();
        for n in ["México", "Đắk", "Lắk", "Krông", "Nô"] {
            assert!(names.contains(&n.to_string()), "chưa bảo vệ tên riêng {n}");
        }
    }

    #[test]
    fn protects_non_latin_scripts() {
        // Ký hiệu Hy Lạp trong bài khoa học là nhóm bị báo oan nhiều nhất trước
        // khi có luật này.
        let src = "Bước sóng λ và hệ số α ảnh hưởng tới ω trong công thức.";
        let reasons: Vec<ProtectReason> = skipped(src).into_iter().map(|(_, r)| r).collect();
        assert!(reasons.iter().all(|r| *r == ProtectReason::NonLatinScript));
        assert_eq!(
            checked(src),
            vec![
                "bước", "sóng", "và", "hệ", "số", "ảnh", "hưởng", "tới", "trong", "công", "thức"
            ]
        );

        // Kirin, Hán, Nhật cũng vậy
        for s in ["Кириллица", "漢字", "日本語", "καὶ"] {
            let line = format!("từ {s} ở giữa câu");
            assert!(check_free(&line), "chưa bảo vệ chữ {s}");
        }
    }

    /// Không có token nào ngoài chữ Latin bị đưa xuống lớp kiểm tra.
    fn check_free(src: &str) -> bool {
        tokenize(src).into_iter().filter(Token::checkable).all(|t| {
            t.normalized
                .chars()
                .all(|c| !c.is_alphabetic() || is_latin_letter(c))
        })
    }

    #[test]
    fn first_word_of_sentence_is_still_checked() {
        // Viết hoa đầu câu là chuyện thường, không phải tín hiệu tên riêng —
        // nếu bỏ qua thì mất khả năng bắt lỗi ở từ đầu mỗi câu.
        assert_eq!(
            checked("Tôi sai. Nhưng sửa được."),
            vec!["tôi", "sai", "nhưng", "sửa", "được"]
        );
        assert_eq!(checked("Xin chào"), vec!["xin", "chào"]);
    }

    #[test]
    fn newline_starts_a_new_sentence() {
        assert_eq!(
            checked("dòng một\nHai dòng"),
            vec!["dòng", "một", "hai", "dòng"]
        );
    }

    #[test]
    fn tokens_cover_the_whole_input_without_gaps() {
        // Bất biến: ghép mọi span lại phải ra đúng text gốc.
        for src in [
            "Tôi yêu tiếng Việt.",
            "a@b.com  #tag\n\tcode\n`x` D:\\p ../q",
            "",
            "   ",
            "…!?",
            "tie\u{0302}\u{0301}ng",
        ] {
            let toks = tokenize(src);
            let mut pos = 0;
            for t in &toks {
                assert_eq!(
                    t.span.start, pos,
                    "có khoảng hở trước span {:?} trong {src:?}",
                    t.span
                );
                pos = t.span.end;
            }
            assert_eq!(pos, src.len(), "token không phủ hết {src:?}");
        }
    }
}
