//! P3 — Thêm dấu tự động.
//!
//! `toi yeu tieng viet` → `tôi yêu tiếng Việt`
//!
//! # Vì sao chi phí biên gần bằng không
//!
//! PLAN.md xếp việc này vào MVP với lý do "dùng chung ~90% hạ tầng". Giờ thì thấy
//! con số đó còn khiêm tốn: sửa lỗi real-word và thêm dấu là **cùng một bài toán** —
//! sinh lựa chọn cho từng vị trí, rồi giải mã chuỗi tốt nhất bằng mô hình ngôn ngữ.
//!
//! ```text
//! Sửa real-word:  "chia sẽ" → {sẻ, sẽ, sẹ…}          → Viterbi → "chia sẻ"
//! Thêm dấu:       "chia se" → {se, sẻ, sẽ, sè, sé…}  → Viterbi → "chia sẻ"
//! ```
//!
//! Khác biệt duy nhất là **bước sinh lựa chọn**. Bộ giải mã ([`crate::lm::viterbi`]),
//! mô hình ngôn ngữ, và từ điển dùng lại 100%. Toàn bộ module này là bước sinh lựa
//! chọn cộng với việc ghép lại text — không có mô hình mới nào.
//!
//! # Bỏ dấu ≠ bỏ thanh
//!
//! [`crate::phonology::strip_tone`] chỉ bỏ **thanh điệu**, giữ nguyên dấu phụ của
//! chữ cái (`tiếng` → `tiêng`). Ở đây phải bỏ về **ASCII trần** (`tiếng` → `tieng`),
//! vì đó là thứ người ta gõ khi không bật bộ gõ.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{dict, lm, normalize, token};

/// Bỏ toàn bộ dấu tiếng Việt, đưa về ASCII trần.
///
/// `tiếng` → `tieng`, `được` → `duoc`, `Đà Nẵng` → `Da Nang`.
pub fn strip_diacritics(s: &str) -> String {
    s.chars().map(strip_char).collect()
}

fn strip_char(c: char) -> char {
    // Bảng theo chữ cái cơ sở. Mỗi dòng gồm dạng trần, dạng có dấu phụ, và cả sáu
    // dạng mang thanh của từng dạng đó.
    const TABLE: [(char, &str); 12] = [
        ('a', "aàáảãạăằắẳẵặâầấẩẫậ"),
        ('e', "eèéẻẽẹêềếểễệ"),
        ('i', "iìíỉĩị"),
        ('o', "oòóỏõọôồốổỗộơờớởỡợ"),
        ('u', "uùúủũụưừứửữự"),
        ('y', "yỳýỷỹỵ"),
        ('d', "dđ"),
        ('A', "AÀÁẢÃẠĂẰẮẲẴẶÂẦẤẨẪẬ"),
        ('E', "EÈÉẺẼẸÊỀẾỂỄỆ"),
        ('I', "IÌÍỈĨỊ"),
        ('O', "OÒÓỎÕỌÔỒỐỔỖỘƠỜỚỞỠỢ"),
        ('U', "UÙÚỦŨỤƯỪỨỬỮỰ"),
    ];
    for (base, forms) in TABLE {
        if forms.chars().any(|f| f == c) {
            return base;
        }
    }
    // Y và Đ hoa xử lý riêng cho gọn bảng.
    match c {
        'Y' | 'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'Y',
        'Đ' => 'D',
        other => other,
    }
}

/// Chỉ mục: dạng ASCII → mọi âm tiết có dấu đã chứng thực, tần suất cao trước.
///
/// Chỉ lấy âm tiết **đã chứng thực** trong corpus. Lấy cả 18.261 âm tiết hợp lệ về
/// ngữ âm sẽ nhét vào bộ giải mã hàng loạt lựa chọn không ai dùng, vừa chậm vừa dễ
/// chọn sai.
fn index() -> &'static HashMap<String, Vec<&'static str>> {
    static CACHE: OnceLock<HashMap<String, Vec<&'static str>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map: HashMap<String, Vec<&'static str>> = HashMap::new();
        for syllable in dict::attested_syllables() {
            map.entry(strip_diacritics(syllable))
                .or_default()
                .push(syllable);
        }
        for options in map.values_mut() {
            // Tần suất cao trước: nó vừa là thứ tự hợp lý khi phải cắt bớt, vừa là
            // đáp án đúng khi không có ngữ cảnh nào để dựa vào.
            options.sort_by_key(|s| std::cmp::Reverse(dict::frequency(s)));
        }
        map
    })
}

/// Số lựa chọn tối đa cho mỗi vị trí.
///
/// Vài dạng ASCII ánh xạ tới hơn 20 âm tiết (`a` → `a à á ả ã ạ ă ằ ắ…`). Viterbi
/// bậc hai tốn `O(n · k²)`, nên cắt ở đây giữ tốc độ ở mức dùng được mà hầu như
/// không mất chất lượng — đáp án đúng gần như luôn nằm trong nhóm tần suất cao nhất.
const MAX_OPTIONS: usize = 12;

/// Các dạng có dấu khả dĩ cho một âm tiết ASCII, tần suất cao trước.
pub fn options_for(ascii: &str) -> Vec<&'static str> {
    index()
        .get(ascii)
        .map(|v| v.iter().take(MAX_OPTIONS).copied().collect())
        .unwrap_or_default()
}

/// Token này có cần thêm dấu không?
///
/// Chỉ những từ **hoàn toàn không có dấu** mới được xét. Văn bản trộn lẫn — gõ được
/// nửa chừng rồi bộ gõ tắt — vẫn xử lý được vì ta xét từng từ một.
fn needs_diacritics(word: &str) -> bool {
    word.chars().all(|c| c.is_ascii_alphabetic())
}

/// Lý do bảo vệ của L0 này có chặn việc thêm dấu không?
///
/// # Hai lớp bảo vệ của L0 KHÔNG áp dụng ở đây
///
/// L0 chặn `ProperNoun` (viết hoa giữa câu) và `Acronym` (toàn chữ hoa) vì với kiểm
/// tra chính tả, chúng là tín hiệu "đây không phải từ tiếng Việt". Nhưng khi **cả
/// câu không có dấu**, kiểu viết hoa không còn mang thông tin đó nữa: `Viet`, `Nam`,
/// `Ha Noi` viết hoa y hệt `Paris`; `VIET NAM` viết hoa y hệt `USA`.
///
/// Bỏ hai lớp đó ở đây an toàn vì **chỉ mục tự lọc**: chỉ dạng ASCII ứng với âm tiết
/// tiếng Việt thật mới có lựa chọn.
///
/// | Từ | Dạng ASCII | Khớp âm tiết? | Kết quả |
/// |---|---|---|---|
/// | `Paris` | `paris` | không | để yên |
/// | `USA` | `usa` | không | để yên |
/// | `HĐND` | `hdnd` | không | để yên |
/// | `Viet` | `viet` | có (`việt`, `viết`) | thêm dấu |
///
/// **Rủi ro còn lại, nói cho rõ:** viết tắt hai-ba chữ tình cờ trùng dạng một âm tiết
/// (`BA`, `CA`, `MA`) vẫn có thể bị đụng. Dạng gốc luôn nằm trong danh sách lựa chọn
/// nên mô hình ngôn ngữ thường giữ nguyên nó, nhưng không có gì bảo đảm. Đây là đánh
/// đổi có ý thức: chặn hết chữ hoa thì mất `VIET NAM` và `XIN CHAO` — vốn phổ biến
/// hơn nhiều trong tin nhắn.
///
/// Các lý do còn lại **vẫn chặn** — thêm dấu vào URL, email hay code là phá chúng, và
/// ở đó chỉ mục không cứu được vì `com` thật sự khớp `còm`.
fn blocks_restore(reason: token::ProtectReason) -> bool {
    use token::ProtectReason as R;
    matches!(
        reason,
        R::Url | R::Email | R::Path | R::Mention | R::Hashtag | R::Code | R::MixedAlnum
    )
}

/// Áp lại kiểu viết hoa của bản gốc lên bản có dấu.
///
/// `Toi` → `Tôi`, `VIET` → `VIỆT`. Không có bước này thì thêm dấu phá hết tên riêng
/// và chữ đầu câu.
fn match_case(original: &str, restored: &str) -> String {
    let mut orig = original.chars();
    let Some(first) = orig.next() else {
        return restored.to_string();
    };

    // TOÀN chữ hoa (từ hai chữ trở lên) → giữ toàn hoa.
    if original.chars().filter(|c| c.is_alphabetic()).count() >= 2
        && original
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
    {
        return restored.to_uppercase();
    }

    if first.is_uppercase() {
        let mut out: String = restored
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default();
        out.extend(restored.chars().skip(1));
        return out;
    }
    restored.to_string()
}

/// Một vị trí được thêm dấu, kèm các lựa chọn khác cho vị trí đó.
///
/// Tồn tại vì UI cần nhiều hơn chuỗi kết quả: user phải thấy **từng chỗ** đã đổi để
/// bỏ chọn hoặc chọn phương án khác. Với độ chính xác 94,47%, cứ khoảng 18 âm tiết
/// lại có một chỗ sai — đưa thẳng kết quả vào ô nhập mà không cho xem trước là đẩy
/// cái sai đó cho user tự tìm.
#[derive(Debug, Clone)]
pub struct Restoration {
    /// Span theo byte trong text **gốc**.
    pub span: std::ops::Range<usize>,
    /// Dạng trong bản gốc, chưa dấu.
    pub from: String,
    /// Dạng mô hình ngôn ngữ chọn, đã khớp kiểu viết hoa của bản gốc.
    pub to: String,
    /// Các lựa chọn khác cho vị trí này, đã khớp kiểu viết hoa, tần suất cao trước.
    /// Luôn chứa cả [`Restoration::to`] ở đầu.
    pub options: Vec<String>,
}

/// Thêm dấu cho một đoạn text.
///
/// Giữ nguyên dấu câu, khoảng trắng, URL, code và mọi vùng bảo vệ khác của L0 — chỉ
/// những từ ASCII thuần được đụng tới.
pub fn restore(text: &str) -> String {
    apply(text, &restore_changes(text))
}

/// Áp một danh sách thay đổi lên text gốc.
///
/// Tách khỏi [`restore_changes`] để UI dùng lại được sau khi user bỏ chọn vài chỗ
/// hoặc đổi phương án. Quan trọng là **cùng một hàm** dựng bản xem trước và bản ghi
/// ra ô nhập — hai đường riêng thì sớm muộn cũng lệch nhau.
pub fn apply(text: &str, changes: &[Restoration]) -> String {
    // Đi từ cuối về đầu để span của các thay đổi trước không bị dịch.
    let mut order: Vec<usize> = (0..changes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(changes[i].span.start));

    let mut out = text.to_string();
    for i in order {
        out.replace_range(changes[i].span.clone(), &changes[i].to);
    }
    out
}

/// Như [`restore`] nhưng trả về **từng chỗ đã đổi** thay vì chuỗi kết quả.
///
/// Chỉ liệt kê vị trí thật sự khác bản gốc. Vị trí mà mô hình chọn lại đúng dạng
/// ASCII ban đầu (`ma` → `ma`) không phải một thay đổi, dù nó vẫn tham gia làm ngữ
/// cảnh cho các từ bên cạnh.
///
/// Kết quả sắp theo thứ tự xuất hiện trong text.
pub fn restore_changes(text: &str) -> Vec<Restoration> {
    let tokens = token::tokenize(text);

    // Vị trí các token sẽ được thêm dấu, và lựa chọn cho từng vị trí.
    let mut targets: Vec<usize> = Vec::new();
    let mut options: Vec<Vec<&str>> = Vec::new();

    // Chuỗi đưa vào bộ giải mã gồm MỌI từ, kể cả từ không cần sửa — vì từ đã có dấu
    // chính là ngữ cảnh giúp chọn đúng cho từ bên cạnh.
    let mut sequence: Vec<Vec<&str>> = Vec::new();
    let mut seq_owner: Vec<usize> = Vec::new();

    for (i, tok) in tokens.iter().enumerate() {
        if tok.kind != token::TokenKind::Word {
            continue;
        }
        let raw = tok.text(text);

        let blocked = tok.protect.is_some_and(blocks_restore);
        if !blocked && needs_diacritics(raw) {
            let opts = options_for(&tok.normalized);
            if !opts.is_empty() {
                targets.push(i);
                options.push(opts.clone());
                sequence.push(opts);
                seq_owner.push(i);
                continue;
            }
        }
        // Từ đã có dấu, hoặc được bảo vệ: một lựa chọn duy nhất — chính nó.
        sequence.push(vec![tok.normalized.as_str()]);
        seq_owner.push(i);
    }

    if targets.is_empty() {
        return Vec::new();
    }

    let path = lm::viterbi(&sequence);

    let mut out = Vec::new();
    for (slot, &tok_index) in seq_owner.iter().enumerate() {
        if !targets.contains(&tok_index) {
            continue;
        }
        let tok = &tokens[tok_index];
        let original = tok.text(text);
        let chosen = match_case(original, sequence[slot][path[slot]]);
        if chosen == original {
            continue;
        }
        // Đưa phương án đã chọn lên đầu, phần còn lại giữ thứ tự tần suất.
        let mut options = vec![chosen.clone()];
        options.extend(
            sequence[slot]
                .iter()
                .map(|o| match_case(original, o))
                .filter(|o| o != &chosen),
        );
        out.push(Restoration {
            span: tok.span.clone(),
            from: original.to_string(),
            to: chosen,
            options,
        });
    }
    out
}

/// Bỏ dấu cả đoạn text — dùng để sinh bộ test và cho user muốn gõ không dấu.
pub fn remove(text: &str) -> String {
    normalize::to_nfc(text).chars().map(strip_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_every_vietnamese_diacritic() {
        assert_eq!(strip_diacritics("tiếng Việt"), "tieng Viet");
        assert_eq!(strip_diacritics("được"), "duoc");
        assert_eq!(strip_diacritics("Đà Nẵng"), "Da Nang");
        assert_eq!(strip_diacritics("ưở ẫẩỡợ"), "uo aaoo");
        // ASCII đi qua nguyên vẹn
        assert_eq!(strip_diacritics("hello world 123!"), "hello world 123!");
    }

    #[test]
    fn index_maps_ascii_to_toned_forms() {
        let se = options_for("se");
        assert!(se.contains(&"sẻ"), "{se:?}");
        assert!(se.contains(&"sẽ"), "{se:?}");
        assert!(se.contains(&"se"), "{se:?}");
    }

    #[test]
    fn restores_a_plain_sentence() {
        assert_eq!(restore("toi yeu tieng viet"), "tôi yêu tiếng việt");
    }

    #[test]
    fn keeps_capitalisation() {
        let got = restore("Toi yeu tieng Viet");
        assert!(got.starts_with("Tôi"), "{got}");
        assert!(got.contains("Việt"), "{got}");
        assert_eq!(restore("VIET NAM"), "VIỆT NAM");
    }

    #[test]
    fn keeps_punctuation_and_spacing() {
        let got = restore("Xin chao, ban khoe khong?");
        assert!(got.contains(','), "{got}");
        assert!(got.ends_with('?'), "{got}");
        assert!(!got.contains("  "), "{got}");
    }

    #[test]
    fn the_index_itself_filters_foreign_words_and_acronyms() {
        // Đây là lý do bỏ được hai lớp bảo vệ ProperNoun và Acronym: dạng ASCII của
        // chúng không khớp âm tiết tiếng Việt nào, nên chỉ mục trả về rỗng.
        for foreign in ["paris", "london", "usa", "hdnd", "sdd", "wikipedia"] {
            assert!(
                options_for(foreign).is_empty(),
                "{foreign} không nên có lựa chọn thêm dấu"
            );
        }
        let got = restore("Toi den Paris roi qua London");
        assert!(got.contains("Paris") && got.contains("London"), "{got}");
        assert!(got.starts_with("Tôi"), "{got}");

        let got = restore("Nghi quyet cua HDND");
        assert!(got.contains("HDND"), "{got}");
    }

    #[test]
    fn leaves_protected_spans_alone() {
        // URL, email, code không được thêm dấu — `com` thành `còm` thì hỏng link.
        for s in [
            "xem https://example.com/tin-tuc nhe",
            "gui toi khanh@rivercrane.com.vn",
            "chay `cargo build` di",
        ] {
            let got = restore(s);
            assert!(got.contains("com") || got.contains("cargo"), "{s} → {got}");
        }
    }

    #[test]
    fn already_toned_words_are_context_not_targets() {
        // Từ đã có dấu phải giữ NGUYÊN, và đồng thời làm ngữ cảnh cho từ bên cạnh.
        let got = restore("Tôi muốn chia se điều này");
        assert!(got.contains("Tôi muốn chia"), "{got}");
        assert!(got.contains("điều này"), "{got}");
        // `se` giữa `chia` và `điều` phải ra `sẻ`
        assert!(got.contains("chia sẻ"), "{got}");
    }

    #[test]
    fn context_disambiguates_the_same_ascii_form() {
        // Cùng chuỗi `hoc` nhưng ngữ cảnh khác nhau — đây là điều bảng tra không
        // làm được và mô hình ngôn ngữ làm được.
        let a = restore("toi di hoc");
        assert!(a.contains("học"), "{a}");
    }

    #[test]
    fn text_with_no_ascii_words_is_returned_unchanged() {
        let s = "Tôi yêu tiếng Việt.";
        assert_eq!(restore(s), s);
    }

    #[test]
    fn round_trip_through_remove_and_restore() {
        // Bỏ dấu rồi thêm lại phải ra gần bản gốc. Đây cũng là cách sinh bộ test.
        let original = "tôi đi học ở trường";
        let stripped = remove(original);
        assert_eq!(stripped, "toi di hoc o truong");
        let restored = restore(&stripped);
        assert_eq!(restored, original);
    }

    #[test]
    fn changes_pinpoint_each_edit_and_offer_alternatives() {
        // Đây là thứ popup cần: biết đã đổi chỗ nào, thành gì, và còn phương án nào.
        let src = "toi di hoc";
        let ch = restore_changes(src);
        assert_eq!(ch.len(), 3, "{ch:?}");
        assert_eq!(ch[0].from, "toi");
        assert_eq!(ch[0].to, "tôi");
        assert_eq!(&src[ch[0].span.clone()], "toi");
        // Phương án đã chọn phải đứng đầu, và phải còn phương án khác để user đổi.
        assert_eq!(ch[0].options[0], "tôi");
        assert!(ch[0].options.len() > 1, "{:?}", ch[0].options);
    }

    #[test]
    fn changes_are_ascending_and_never_report_a_no_op() {
        let src = "Toi den Paris hom nay";
        let ch = restore_changes(src);
        assert!(ch.windows(2).all(|w| w[0].span.start < w[1].span.start));
        // `Paris` không khớp âm tiết nào nên không có mặt; không chỗ nào được liệt kê
        // mà lại giữ nguyên chữ.
        assert!(ch.iter().all(|c| c.from != c.to), "{ch:?}");
        assert!(!ch.iter().any(|c| c.from == "Paris"));
    }

    #[test]
    fn apply_reproduces_restore_exactly() {
        // Bất biến quan trọng nhất của cặp API này: bản xem trước user duyệt và bản
        // ghi vào ô nhập phải là một. Nếu test này vỡ thì UI đang nói dối.
        for s in [
            "hom nay toi di hoc o truong dai hoc bach khoa",
            "Xin chao, ban khoe khong?",
            "Toi den Paris hom nay, xem https://a.vn/b nhe",
            "khong co gi de sua",
        ] {
            assert_eq!(apply(s, &restore_changes(s)), restore(s), "lệch ở {s:?}");
        }
    }

    #[test]
    fn apply_with_a_subset_touches_only_the_kept_spans() {
        // User bỏ chọn vài chỗ — phần còn lại vẫn phải rơi đúng vị trí, vì span của
        // các thay đổi sau bị dịch khi thay đổi trước đổi độ dài byte.
        let src = "toi di hoc";
        let all = restore_changes(src);
        let subset: Vec<_> = all.iter().filter(|c| c.from != "di").cloned().collect();
        assert_eq!(apply(src, &subset), "tôi di học");
    }
}
