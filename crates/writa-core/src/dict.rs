//! L2 — Từ vựng suy ra từ corpus.
//!
//! # Ba câu hỏi L1 không trả lời được
//!
//! L1 chỉ biết một âm tiết có **hợp lệ về ngữ âm** hay không. Nó mù với ba việc:
//!
//! 1. **`electron` có phải lỗi không?** Không phải âm tiết tiếng Việt, nhưng là từ
//!    hoàn toàn bình thường trong văn bản Việt. Đo thực tế: L1 một mình báo oan
//!    **20,52 lần / 1000 từ** và gần như toàn bộ là nhóm này.
//! 2. **`khoẻn` có phải lỗi không?** Hợp lệ ngữ âm nhưng không xuất hiện trong
//!    corpus — 11.501 / 18.261 âm tiết sinh ra thuộc loại này. Tín hiệu nghi vấn,
//!    không phải kết luận.
//! 3. **`sử dụng` hay `xử dụng`?** Cần biết tổ hợp nào thật sự tồn tại.
//!
//! # Vì sao tần suất corpus là câu trả lời
//!
//! Từ vay mượn và lỗi gõ tay **giống nhau** dưới mắt L1: cả hai đều "không phải
//! âm tiết tiếng Việt". Thứ tách chúng ra là **độ lan toả** — từ vay mượn rải khắp
//! nhiều câu, lỗi gõ tay thì lẻ tẻ.
//!
//! Bằng chứng nằm sẵn trong dữ liệu: `vectơ` xuất hiện 929 lần, còn `thuớc` — lỗi
//! chính tả **thật** của Wikipedia (đúng là `thước`) — chỉ 11 lần.
//!
//! Cách này còn giữ được license: mọi thứ ở đây suy ra từ corpus, không phái sinh
//! từ `hunspell-vi` hay từ điển GPL nào.
//!
//! # Ghi chú hiệu năng
//!
//! Dữ liệu nhúng bằng `include_str!` và khoá của các bảng **tham chiếu thẳng vào
//! chuỗi `'static`** đó — dựng bảng không cấp phát chuỗi nào, tra cứu cũng vậy.
//! Khi L4 lên và biết yêu cầu kích thước thật, chuyển sang FST + mmap như PLAN.md.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const SYLLABLES_TSV: &str = include_str!("../../../data/lexicon/syllables.tsv");
const ACCEPTED_TSV: &str = include_str!("../../../data/lexicon/accepted.tsv");
const COMPOUNDS_TSV: &str = include_str!("../../../data/lexicon/compounds.tsv");
const TRIGRAMS_TSV: &str = include_str!("../../../data/lexicon/trigrams.tsv");

/// Bỏ comment và dòng trống, trả về các cột đã cắt.
fn rows(tsv: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    tsv.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect())
}

fn parse_count(cols: &[&str], idx: usize) -> u64 {
    cols.get(idx)
        .and_then(|c| c.trim().parse().ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Chỉ mục dòng — thay cho HashMap ở hai bảng lớn
// ---------------------------------------------------------------------------

/// Tra cứu khoá → số đếm bằng **tìm kiếm nhị phân trên chính chuỗi đã nhúng**.
///
/// # Vì sao không dùng HashMap
///
/// Bảng từ ghép và trigram chiếm hầu hết dữ liệu, và dựng `HashMap` cho chúng tốn bộ
/// nhớ gấp nhiều lần bản thân dữ liệu — mỗi ô phải giữ khoá, giá trị, mã băm và chỗ
/// trống của bảng. Đo trên máy thật: nới ngưỡng cắt tỉa để trigram từ 250 nghìn lên
/// 1,5 triệu dòng làm RAM tiến trình chính nhảy từ **80 MB lên 194 MB**, trong khi dữ
/// liệu thô chỉ tăng 16 MB. Chỉ tiêu MVP là dưới 80 MB, nên hai mục tiêu — thêm dấu
/// chính xác hơn và RAM thấp — trở thành xung đột giả tạo do cách lưu.
///
/// Ở đây ta không dựng bảng nào. Dữ liệu đã nằm sẵn trong file thực thi dưới dạng văn
/// bản; thứ duy nhất phải cấp phát là **một `u32` cho mỗi dòng** để biết dòng bắt đầu ở
/// đâu — 4 byte/dòng thay vì hàng chục. Tra cứu là tìm nhị phân trên mảng offset đó, so
/// khoá đọc thẳng từ chuỗi gốc.
///
/// Đây chính là bước mà ghi chú hiệu năng ở đầu file dự liệu ("chuyển sang FST + mmap
/// khi biết yêu cầu kích thước thật"), làm bằng cách rẻ nhất đủ dùng: không thêm
/// dependency, không đổi định dạng dữ liệu.
struct LineIndex {
    tsv: &'static str,
    /// Offset byte đầu mỗi dòng dữ liệu, **đã sắp theo khoá**.
    starts: Vec<u32>,
}

impl LineIndex {
    fn build(tsv: &'static str) -> Self {
        let mut starts = Vec::new();
        let mut at = 0usize;
        for line in tsv.split_inclusive('\n') {
            let trimmed = line.trim_end();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                starts.push(at as u32);
            }
            at += line.len();
        }
        let mut idx = LineIndex { tsv, starts };
        // Sắp ở đây thay vì tin file đầu vào: một lần sắp 1,5 triệu `u32` mất vài trăm
        // mili giây và chỉ chạy một lần, còn một file chưa sắp thì làm mọi tra cứu sai
        // âm thầm.
        idx.starts
            .sort_unstable_by(|a, b| key_of(tsv, *a as usize).cmp(key_of(tsv, *b as usize)));
        idx
    }

    fn get(&self, key: &str) -> Option<u64> {
        let pos = self
            .starts
            .binary_search_by(|off| key_of(self.tsv, *off as usize).cmp(key))
            .ok()?;
        let line = line_at(self.tsv, self.starts[pos] as usize);
        line.split('\t').nth(1)?.trim().parse().ok()
    }

    fn len(&self) -> usize {
        self.starts.len()
    }
}

fn line_at(tsv: &'static str, start: usize) -> &'static str {
    let rest = &tsv[start..];
    match rest.find('\n') {
        Some(n) => rest[..n].trim_end(),
        None => rest.trim_end(),
    }
}

fn key_of(tsv: &'static str, start: usize) -> &'static str {
    let line = line_at(tsv, start);
    line.split('\t').next().unwrap_or(line)
}

// ---------------------------------------------------------------------------
// Âm tiết đã chứng thực
// ---------------------------------------------------------------------------

/// Giá trị: (tần suất trên trọn dump, tần suất trên tập câu dựng từ ghép).
///
/// Phải giữ cả hai vì chúng dùng cho hai việc khác nhau và **không thay thế được
/// cho nhau**: cột dump nhiều dữ liệu hơn nên xếp hạng candidate tốt hơn, còn cột
/// tập câu là cột duy nhất cùng mẫu với `compounds.tsv` nên là cột duy nhất tính
/// được xác suất có điều kiện `P(b|a) = freq(a b) / freq(a)`. Trộn hai mẫu vào một
/// tỉ số thì tỉ số vô nghĩa.
fn attested_map() -> &'static HashMap<&'static str, (u64, u64)> {
    static CACHE: OnceLock<HashMap<&'static str, (u64, u64)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        rows(SYLLABLES_TSV)
            .filter_map(|c| {
                c.first()
                    .map(|s| (*s, (parse_count(&c, 1), parse_count(&c, 2))))
            })
            .collect()
    })
}

/// Âm tiết này có thật sự xuất hiện trong corpus không?
///
/// Khác [`crate::phonology::is_valid_syllable`]: hàm kia hỏi "có hợp lệ về ngữ âm",
/// hàm này hỏi "có ai dùng thật". `khoẻn` hợp lệ nhưng không ai dùng.
pub fn is_attested(syllable: &str) -> bool {
    attested_map().contains_key(syllable)
}

/// Mọi âm tiết đã chứng thực. Dùng để dựng chỉ mục bỏ-dấu của P3.
pub fn attested_syllables() -> impl Iterator<Item = &'static str> {
    attested_map().keys().copied()
}

/// Tần suất trên trọn dump (231 triệu token) — dùng xếp hạng candidate ở L3.
pub fn frequency(syllable: &str) -> u64 {
    attested_map().get(syllable).map_or(0, |(dump, _)| *dump)
}

/// Tần suất trên **cùng tập câu** đã dựng [`compound_frequency`].
///
/// Đây là mẫu số duy nhất hợp lệ để tính `P(b|a) = freq(a b) / freq(a)`. Xem
/// [`collocation_strength`].
pub fn sentence_frequency(syllable: &str) -> u64 {
    attested_map().get(syllable).map_or(0, |(_, sent)| *sent)
}

/// Độ mạnh kết hợp: `P(second | first)` — xác suất `second` theo sau `first`.
///
/// # Vì sao cần con số này
///
/// Tần suất bigram thô **không phân biệt** được từ ghép cố định với kết hợp tự do,
/// và đó là lỗ hổng đã đo thấy: engine báo `cát → các`, `dùng → vùng`, `hộ → họ`
/// vì tổ hợp thay thế *có tần suất cao*, mà không biết rằng cao chỉ vì cả hai từ
/// đều phổ biến.
///
/// `chia sẻ` là **một từ** — gần như mỗi lần thấy `chia` là có `sẻ` theo sau, nên
/// `P(sẻ|chia)` lớn. `các trắng` thì `các` đi trước hàng nghìn danh từ khác nhau,
/// nên `P(trắng|các)` bé tí dù đếm thô có thể không nhỏ.
///
/// Trả về 0.0 nếu `first` không có trong corpus.
pub fn collocation_strength(first: &str, second: &str) -> f64 {
    let base = sentence_frequency(first);
    if base == 0 {
        return 0.0;
    }
    compound_frequency(first, second) as f64 / base as f64
}

/// Độ chặt của một tổ hợp, đo **cả hai chiều**: `min(P(b|a), P(a|b))`.
///
/// # Vì sao phải hai chiều
///
/// Một chiều không đủ, và đây là lỗ hổng đã đo thấy. Tiếng Việt có nhiều **từ chức
/// năng** — `của`, `có`, `là`, `và`, `được` — đi sau rất nhiều từ khác nhau. Với
/// chúng `P(của|X)` lớn với hầu hết `X`, nên cổng một chiều để lọt `cửa → của`
/// (71 lần) và `cố → có` (50 lần) trên 50 nghìn câu.
///
/// Chiều ngược lại thì phơi bày ngay: `P(X|của)` bé tí vì `của` đứng cạnh hàng
/// nghìn từ. Còn từ ghép cố định thật thì chặt cả hai chiều — `chia sẻ` vừa có
/// `sẻ` gần như luôn theo sau `chia`, vừa có `chia` là ngữ cảnh chính của `sẻ`.
pub fn compound_tightness(first: &str, second: &str) -> f64 {
    let forward = collocation_strength(first, second);
    let backward = {
        let base = sentence_frequency(second);
        if base == 0 {
            0.0
        } else {
            compound_frequency(first, second) as f64 / base as f64
        }
    };
    forward.min(backward)
}

// ---------------------------------------------------------------------------
// Từ vay mượn / tên riêng / viết tắt được chấp nhận
// ---------------------------------------------------------------------------

fn accepted_set() -> &'static HashSet<&'static str> {
    static CACHE: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CACHE.get_or_init(|| {
        rows(ACCEPTED_TSV)
            .filter_map(|c| c.first().copied())
            .collect()
    })
}

/// Token này không phải âm tiết tiếng Việt, nhưng có được chấp nhận trong văn bản
/// Việt không?
///
/// Bao gồm từ vay mượn khoa học (`electron`, `protein`), tên riêng ngoại (`paris`,
/// `canada`), và viết tắt thường (`km`, `dna`).
pub fn is_accepted_foreign(token: &str) -> bool {
    accepted_set().contains(token)
}

// ---------------------------------------------------------------------------
// Từ ghép
// ---------------------------------------------------------------------------

fn compound_index() -> &'static LineIndex {
    static CACHE: OnceLock<LineIndex> = OnceLock::new();
    CACHE.get_or_init(|| LineIndex::build(COMPOUNDS_TSV))
}

/// Tần suất của từ ghép hai âm tiết, `0` nếu không có trong corpus.
///
/// Đây là thứ phân biệt `sử dụng` (có) với `xử dụng` (không) — cả hai đều gồm âm
/// tiết hợp lệ nên L1 không giúp được gì.
pub fn compound_frequency(first: &str, second: &str) -> u64 {
    // Ghép khoá vào bộ đệm trên stack: cặp âm tiết tiếng Việt dài nhất là 7+1+7 ký tự,
    // mỗi ký tự tối đa 3 byte trong UTF-8. Cấp phát một `String` cho mỗi lần tra là
    // hàng triệu lần cấp phát trong một lượt giải mã.
    let mut buf = [0u8; 64];
    let Some(key) = join_key(&mut buf, &[first, second]) else {
        return 0;
    };
    compound_index().get(key).unwrap_or(0)
}

/// Nối các phần bằng dấu cách vào bộ đệm cho sẵn. `None` nếu không đủ chỗ.
fn join_key<'a>(buf: &'a mut [u8], parts: &[&str]) -> Option<&'a str> {
    let mut at = 0usize;
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            *buf.get_mut(at)? = b' ';
            at += 1;
        }
        let end = at.checked_add(p.len())?;
        buf.get_mut(at..end)?.copy_from_slice(p.as_bytes());
        at = end;
    }
    // An toàn: mọi byte ghi vào đều đến từ `&str` hợp lệ, ngăn cách bằng dấu cách ASCII.
    std::str::from_utf8(&buf[..at]).ok()
}

fn trigram_index() -> &'static LineIndex {
    static CACHE: OnceLock<LineIndex> = OnceLock::new();
    CACHE.get_or_init(|| LineIndex::build(TRIGRAMS_TSV))
}

/// Tần suất trigram, `0` nếu không có trong corpus.
///
/// Vắng mặt **không** nghĩa là "không thể" — [`crate::lm`] sẽ lùi về bigram rồi
/// unigram. Đó chính là chỗ mà tần suất thô làm sai ở L3.
pub fn trigram_frequency(a: &str, b: &str, c: &str) -> u64 {
    let mut buf = [0u8; 96];
    let Some(key) = join_key(&mut buf, &[a, b, c]) else {
        return 0;
    };
    trigram_index().get(key).unwrap_or(0)
}

/// Tổng số token trên tập câu đã dựng bigram/trigram — mẫu số cho xác suất unigram.
pub fn total_tokens() -> u64 {
    static CACHE: OnceLock<u64> = OnceLock::new();
    *CACHE.get_or_init(|| attested_map().values().map(|(_, sent)| *sent).sum())
}

/// Số liệu tổng quan, cho `writa-cli dict`.
pub fn stats() -> Stats {
    Stats {
        attested: attested_map().len(),
        accepted_foreign: accepted_set().len(),
        compounds: compound_index().len(),
        trigrams: trigram_index().len(),
        total_tokens: total_tokens(),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub attested: usize,
    pub accepted_foreign: usize,
    pub compounds: usize,
    pub trigrams: usize,
    pub total_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonology;

    #[test]
    fn lexicon_files_parse() {
        let s = stats();
        assert!(
            s.attested > 5_000,
            "âm tiết chứng thực quá ít: {}",
            s.attested
        );
        assert!(
            s.accepted_foreign > 500,
            "từ ngoại quá ít: {}",
            s.accepted_foreign
        );
        assert!(s.compounds > 10_000, "từ ghép quá ít: {}", s.compounds);
    }

    #[test]
    fn attested_is_a_subset_of_phonologically_valid() {
        // Bất biến: mọi thứ trong syllables.tsv phải là âm tiết hợp lệ. Nếu vỡ,
        // nghĩa là lexicon được dựng bằng một tập âm tiết khác với tập hiện tại
        // → phải chạy lại `writa-cli dump` + `build_lexicon.py`.
        let bad: Vec<&str> = attested_map()
            .keys()
            .copied()
            .filter(|s| !phonology::is_valid_syllable(s))
            .take(10)
            .collect();
        assert!(bad.is_empty(), "lexicon lệch pha với bảng ngữ âm: {bad:?}");
    }

    #[test]
    fn accepted_foreign_never_overlaps_valid_syllables() {
        // Nếu chồng nhau thì có gì đó sai: từ ngoại phải là thứ L1 KHÔNG nhận ra.
        let bad: Vec<&str> = accepted_set()
            .iter()
            .copied()
            .filter(|s| phonology::is_valid_syllable(s))
            .take(10)
            .collect();
        assert!(bad.is_empty(), "từ ngoại lại là âm tiết hợp lệ: {bad:?}");
    }

    #[test]
    fn common_words_are_attested() {
        for s in ["tôi", "của", "được", "người", "chia", "sẻ", "nghĩ", "ngành"] {
            assert!(is_attested(s), "{s} phải có trong corpus");
        }
    }

    #[test]
    fn recognises_loanwords_and_foreign_names() {
        for s in ["electron", "protein", "virus", "paris", "canada", "km"] {
            assert!(is_accepted_foreign(s), "{s} phải được chấp nhận");
        }
    }

    #[test]
    fn does_not_accept_arbitrary_garbage() {
        for s in ["chinhs", "nghanh", "xyzqwv", "asdfgh"] {
            assert!(!is_accepted_foreign(s), "{s} không được chấp nhận");
        }
    }

    #[test]
    fn truncated_fragments_are_not_accepted() {
        // Bản đầu của build_lexicon.py quét regex chữ-thường trên dòng còn chữ
        // hoa, nên bỏ chữ cái đầu của mọi từ viết hoa và sinh ra một lexicon đầy
        // mảnh vụn: `Wikipedia`→`ikipedia`, `Paris`→`aris`, `Giáo`→`iáo`.
        // Lỗi đó nguy hiểm hơn báo oan vì nó làm engine IM LẶNG trước lỗi thật.
        //
        // Chỉ liệt kê những mảnh KHÔNG THỂ là từ dùng thật. `ng`, `nh` từng bị
        // nghi oan ở đây: chúng đến từ teencode có thật trong corpus ("mọi ng" =
        // "mọi người"), không phải từ lỗi cắt chữ.
        for s in [
            "ikipedia", "aris", "anada", "instein", "iáo", "uảng", "olynesia",
        ] {
            assert!(
                !is_accepted_foreign(s),
                "{s:?} là mảnh vụn mất chữ đầu — lexicon dựng sai"
            );
        }
    }

    // Ghi chú về token rất ngắn (`ng`, `n`, `ko`, `dc`) có trong accepted.tsv:
    //
    // Ban đầu tôi cho rằng chúng là teencode từ trang Thảo luận: và viết một test
    // chặn chúng. Giả thuyết đó SAI. Sau khi lọc `<ns>0</ns>` (bỏ 36% số trang
    // trong dump vốn là Bản mẫu:/Thể loại:/Thảo luận:), chúng vẫn còn — vì đến từ
    // bài viết thật: `ng` nằm trong bài về ngữ âm tiếng Việt liệt kê "âm cuối
    // được viết bằng p, t, c, ch, m, n, ng"; `ko` từ "KO GmbH" và mã ngôn ngữ Hàn;
    // `n` từ biến số toán học.
    //
    // Vẫn giữ lọc namespace vì nó loại bỏ nhiễu thật, nhưng không chặn token ngắn:
    // chấp nhận chúng chỉ mất khả năng bắt lỗi gõ 1-2 chữ (rất hiếm, và thường
    // lại là âm tiết hợp lệ), còn CHẶN chúng thì báo oan lên `km`, `kg`, `cm`,
    // `tv`, `id` — vốn dày đặc trong văn bản thật. Đúng hướng lệch của dự án:
    // precision trước recall.

    #[test]
    fn tightness_separates_fixed_compounds_from_free_combinations() {
        // Từ ghép cố định — `sẻ` gần như luôn theo sau `chia`, và ngược lại `chia`
        // là ngữ cảnh chính của `sẻ`.
        assert!(
            compound_tightness("chia", "sẻ") >= 0.02,
            "chia sẻ = {}",
            compound_tightness("chia", "sẻ")
        );
        assert!(compound_tightness("sử", "dụng") >= 0.02);

        // Từ chức năng — đây là ca mà cổng MỘT chiều để lọt, khiến engine báo
        // `cửa → của` 71 lần và `cố → có` 50 lần trên 50 nghìn câu.
        // `P(của|nhà)` có thể không nhỏ, nhưng `P(nhà|của)` thì bé tí vì `của`
        // đứng cạnh hàng nghìn từ khác.
        for (a, b) in [("nhà", "của"), ("phần", "của"), ("người", "có")] {
            assert!(
                compound_tightness(a, b) < 0.02,
                "{a} {b} không phải từ ghép cố định nhưng tightness = {}",
                compound_tightness(a, b)
            );
        }
    }

    #[test]
    fn tightness_is_symmetric() {
        for (a, b) in [("chia", "sẻ"), ("sử", "dụng"), ("nhà", "của")] {
            assert_eq!(compound_tightness(a, b), compound_tightness(a, b));
            // min() nên luôn <= chiều xuôi
            assert!(compound_tightness(a, b) <= collocation_strength(a, b) + f64::EPSILON);
        }
    }

    #[test]
    fn compound_frequency_separates_real_from_wrong() {
        // Cặp gồm toàn âm tiết hợp lệ nên L1 bó tay; chỉ tần suất phân biệt được.
        assert!(compound_frequency("sử", "dụng") > 0);
        assert_eq!(compound_frequency("xử", "dụng"), 0);
        assert!(compound_frequency("chia", "sẻ") > 0);
    }
}
