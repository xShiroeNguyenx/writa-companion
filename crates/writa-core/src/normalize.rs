//! L0a — Chuẩn hoá văn bản.
//!
//! # Vì sao lớp này phải chạy trước mọi thứ
//!
//! Tiếng Việt tồn tại **hai dạng Unicode cho cùng một chữ**: dựng sẵn
//! (`ế` = U+1EBF) và tổ hợp (`e` + U+0302 + U+0301). Nhìn trên màn hình giống
//! hệt nhau. Nếu không chuẩn hoá, mọi lần tra FST đều **fail âm thầm** — engine
//! sẽ báo `tiếng` là sai chính tả mà không ai hiểu vì sao.
//!
//! Text tổ hợp (NFD) đến từ macOS, một số web form, và vài IME. Đây không phải
//! ca hiếm.
//!
//! # Điều lớp này KHÔNG làm
//!
//! **Không chuẩn hoá vị trí dấu thanh.** `hòa` và `hoà` đều đúng theo hai quy
//! chuẩn khác nhau, tương tự `khỏe`/`khoẻ`, `thúy`/`thuý`. `phonology.rs` sinh
//! cả hai dạng nên cả hai đều tra được. Việc chuyển về một kiểu là rule tuỳ chọn
//! ở L5, do user bật, không phải việc của lớp chuẩn hoá.
//!
//! # Về mã hoá cũ (VNI / TCVN3 / VIQR)
//!
//! PLAN.md xếp việc này vào L0. Sau khi cân nhắc, tôi **hoãn có chủ đích** và ghi
//! rõ lý do ở đây thay vì dựng máy móc suy đoán:
//!
//! - Clipboard Windows và mọi ô nhập text hiện đại đều truyền Unicode. Hai đường
//!   vào của Writa (đọc selection qua UIA, hook bàn phím) **không bao giờ** trả về
//!   byte 8-bit thô, nên VNI/TCVN3 không đến được engine qua đường thường.
//!   Chúng chỉ xuất hiện khi đọc trực tiếp file cũ — tức là tính năng "batch
//!   document check" ở roadmap, không phải MVP.
//! - Nhận diện TCVN3/VNI phải dựa vào heuristic thống kê byte. Đoán sai thì
//!   **chuyển đổi sai văn bản đúng** — thiệt hại lớn hơn nhiều so với việc không
//!   hỗ trợ.
//! - VIQR (`Vie^.t`) trên thực tế đã tuyệt chủng.
//!
//! Khi làm batch document check thì quay lại đây, và làm kèm bộ test có văn bản
//! TCVN3/VNI thật — chứ không đoán.

use std::borrow::Cow;

use unicode_normalization::{is_nfc, UnicodeNormalization};

/// Ký tự vô hình: không hiện trên màn hình nhưng phá vỡ mọi phép so chuỗi.
///
/// Chúng lọt vào khi copy từ web (đặc biệt là văn bản có soft-hyphen để ngắt
/// dòng) hoặc từ Word. Một ZWSP nằm giữa `tiế` và `ng` khiến `tiếng` không bao
/// giờ khớp từ điển, và user không thấy gì bất thường để mà sửa.
const INVISIBLE: [char; 7] = [
    '\u{200B}', // ZERO WIDTH SPACE
    '\u{200C}', // ZERO WIDTH NON-JOINER
    '\u{200D}', // ZERO WIDTH JOINER
    '\u{2060}', // WORD JOINER
    '\u{00AD}', // SOFT HYPHEN
    '\u{FEFF}', // ZERO WIDTH NO-BREAK SPACE / BOM
    '\u{180E}', // MONGOLIAN VOWEL SEPARATOR
];

pub fn is_invisible(c: char) -> bool {
    INVISIBLE.contains(&c)
}

/// Dấu thanh và dấu phụ tổ hợp mà tiếng Việt dùng ở dạng NFD.
///
/// Cần biết tập này để tokenizer **không cắt từ giữa chữ và dấu của nó**: ở dạng
/// NFD, `ế` là ba code point, và nếu tokenizer coi dấu là ranh giới từ thì một
/// chữ bị xé thành nhiều token.
pub fn is_combining_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}')
}

/// Chữ cái thuộc hệ Latin — bao trọn repertoire tiếng Việt.
///
/// Cần phân biệt vì `char::is_alphabetic()` nhận cả Hy Lạp, Kirin, Hán, Ả Rập.
/// Token chứa chữ ngoài hệ Latin thì **về định nghĩa** không thể là lỗi chính tả
/// tiếng Việt, nên L0 phải bỏ qua nó thay vì để L1 báo oan. Đo thực tế: `λ`, `α`,
/// `β`, `ω`, `γ`, `μ`, `ε` nằm trong nhóm bị báo nhiều nhất.
///
/// Ba dải đủ phủ tiếng Việt:
/// - ASCII `a-z`
/// - `U+00C0..=U+024F` — Latin-1 Supplement + Extended-A/B: `â ê ô ă ơ ư đ`
/// - `U+1E00..=U+1EFF` — Latin Extended Additional: `ế ạ ề ỹ` và toàn bộ tổ hợp
///   dấu-thanh dựng sẵn của tiếng Việt
///
/// Hy Lạp bắt đầu ở `U+0370`, Kirin ở `U+0400`, nên chúng nằm ngoài mọi dải trên.
pub fn is_latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}')
}

/// Chuẩn hoá về NFC, không cấp phát nếu chuỗi đã đúng dạng.
///
/// Đường nóng của Writa (mỗi từ, mỗi keystroke) gần như luôn nhận text đã là NFC,
/// nên nhánh `Borrowed` là nhánh thường gặp.
pub fn to_nfc(s: &str) -> Cow<'_, str> {
    if is_nfc(s) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.nfc().collect())
    }
}

/// Dạng dùng để tra cứu trong tập âm tiết / từ điển: NFC + chữ thường + bỏ ký tự
/// vô hình.
///
/// Thứ tự có chủ ý: NFC trước, rồi mới hạ chữ thường. Với tiếng Việt, hạ chữ
/// thường một ký tự NFC dựng sẵn vẫn cho ra NFC dựng sẵn (`Ế` → `ế`), nên không
/// cần chuẩn hoá lần hai.
pub fn normalize_for_lookup(s: &str) -> String {
    s.nfc()
        .filter(|c| !is_invisible(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfd_and_nfc_collapse_to_the_same_lookup_key() {
        // "tiếng" dạng dựng sẵn
        let nfc = "tiếng";
        // "tiếng" dạng tổ hợp: t i e + U+0302 (mũ) + U+0301 (sắc) n g
        let nfd = "tie\u{0302}\u{0301}ng";
        assert_ne!(nfc, nfd, "hai chuỗi phải khác nhau ở mức byte");
        assert_eq!(normalize_for_lookup(nfc), normalize_for_lookup(nfd));
        assert_eq!(normalize_for_lookup(nfd), "tiếng");
    }

    #[test]
    fn lowercases_vietnamese_correctly() {
        assert_eq!(normalize_for_lookup("TIẾNG VIỆT"), "tiếng việt");
        assert_eq!(normalize_for_lookup("Đường"), "đường");
        assert_eq!(normalize_for_lookup("ỄỄ"), "ễễ");
    }

    #[test]
    fn strips_invisible_characters() {
        // ZWSP giữa chữ — thủ phạm kinh điển khi copy từ web
        assert_eq!(normalize_for_lookup("tiế\u{200B}ng"), "tiếng");
        assert_eq!(normalize_for_lookup("\u{FEFF}Việt\u{00AD}"), "việt");
    }

    #[test]
    fn to_nfc_does_not_allocate_when_already_nfc() {
        assert!(matches!(to_nfc("tiếng Việt"), Cow::Borrowed(_)));
        assert!(matches!(to_nfc("tie\u{0302}\u{0301}ng"), Cow::Owned(_)));
    }

    #[test]
    fn tone_placement_variants_are_left_alone() {
        // L0 tuyệt đối không được biến hoà thành hòa hay ngược lại
        assert_eq!(normalize_for_lookup("hoà"), "hoà");
        assert_eq!(normalize_for_lookup("hòa"), "hòa");
        assert_ne!(normalize_for_lookup("hoà"), normalize_for_lookup("hòa"));
    }

    #[test]
    fn latin_covers_all_vietnamese_letters() {
        for c in "aăâbcdđeêghiklmnoôơpqrstuưvxy".chars() {
            assert!(is_latin_letter(c), "{c:?} phải là chữ Latin");
        }
        // Toàn bộ nguyên âm mang thanh, gồm cả dải Latin Extended Additional
        for c in "àáảãạằắẳẵặầấẩẫậèéẻẽẹềếểễệìíỉĩịòóỏõọồốổỗộờớởỡợùúủũụừứửữựỳýỷỹỵ".chars()
        {
            assert!(is_latin_letter(c), "{c:?} phải là chữ Latin");
        }
    }

    #[test]
    fn rejects_non_latin_scripts() {
        // Hy Lạp — nhóm bị báo oan nhiều nhất trước khi có luật này
        for c in "λαβωγμε".chars() {
            assert!(!is_latin_letter(c), "{c:?} là Hy Lạp, không phải Latin");
        }
        for c in "Кириллица漢字العربية日本".chars() {
            assert!(!is_latin_letter(c), "{c:?} không phải Latin");
        }
    }

    #[test]
    fn recognises_vietnamese_combining_marks() {
        for c in [
            '\u{0300}', '\u{0301}', '\u{0303}', '\u{0309}', '\u{0323}', '\u{0302}', '\u{0306}',
            '\u{031B}',
        ] {
            assert!(
                is_combining_mark(c),
                "U+{:04X} phải là dấu tổ hợp",
                c as u32
            );
        }
        assert!(!is_combining_mark('a'));
        assert!(!is_combining_mark('ế'));
    }
}
