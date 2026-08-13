//! L3 — Sinh candidate sửa lỗi.
//!
//! # Vì sao KHÔNG dùng Levenshtein mù
//!
//! Cách thông thường là sinh mọi chuỗi cách bản gốc 1-2 phép sửa rồi lọc bằng từ
//! điển. Với tiếng Việt cách đó vừa đắt vừa kém: khoảng cách ký tự không phản ánh
//! lỗi thật. `sẻ` và `sẽ` cách nhau 1 ký tự nhưng cũng như vậy là `sẻ` và `sẹ`,
//! `sẻ` và `bẻ` — trong khi thực tế người Việt nhầm hỏi↔ngã chứ không nhầm
//! hỏi↔nặng, và không ai gõ `bẻ` khi muốn `sẻ`.
//!
//! Lỗi tiếng Việt **có cấu trúc**: nó xảy ra ở một trong ba thành phần của âm tiết
//! (âm đầu, vần, thanh) theo những cặp nhầm lẫn hữu hạn, đoán được trước. Nên L3
//! phân tích âm tiết ra ba phần ([`phonology::decompose`]), thay **một** phần theo
//! bảng luật, rồi dựng lại ([`phonology::compose`]).
//!
//! Kết quả: mỗi candidate mang theo **lý do ngữ âm cụ thể** ([`Reason`]) — thứ mà
//! Levenshtein không cho được, và thứ UI cần để giải thích cho user.
//!
//! # Ba lớp siết lại
//!
//! Luật cố tình rộng, còn độ chính xác đến từ ba lớp lọc xếp sau:
//!
//! 1. [`phonology::compose`] chỉ trả `Some` khi tổ hợp hợp lệ về ngữ âm.
//! 2. [`dict::is_attested`] loại âm tiết hợp lệ nhưng không ai dùng.
//! 3. Xếp hạng theo tần suất corpus, cao nhất trước.
//!
//! Nhờ vậy thêm luật vào bảng không làm sinh ra đề xuất rác — nó chỉ mở rộng vùng
//! tìm kiếm.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{dict, phonology};

const RULES_TSV: &str = include_str!("../../../data/confusion/rules.tsv");

/// Lý do một candidate được đề xuất. UI dùng để giải thích cho user, và eval dùng
/// để biết luật nào đang hiệu quả.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Nhầm thanh hỏi ↔ ngã — lỗi phổ biến nhất của người Việt.
    ToneConfusion,
    /// Thanh không hợp với vần: vần đóng bằng p/t/c/ch chỉ nhận sắc và nặng.
    /// `mat` → `mát` / `mạt`.
    ToneRepair,
    /// Nhầm âm đầu: `s`/`x`, `ch`/`tr`, `ngh`/`ng`, `l`/`n`…
    Onset,
    /// Nhầm vần: âm cuối `n`/`ng`, `t`/`c`, hoặc nguyên âm `iê`/`ia`, `i`/`y`…
    Rime,
    /// Gõ Telex mà bộ gõ tắt: `chinhs` → `chính`, `tieengs` → `tiếng`.
    Telex,
    /// Biến thể **đều đúng** theo hai quy chuẩn: `kỹ`/`kĩ`, `lý`/`lí`, `quý`/`quí`.
    ///
    /// Vẫn hữu ích khi sửa một âm tiết sai, nhưng tuyệt đối không được dùng làm căn
    /// cứ báo lỗi real-word — cả hai dạng đúng thì không có gì để báo.
    Variant,
}

impl Reason {
    /// Lý do này có được dùng để phán quyết lỗi *real-word* không?
    ///
    /// [`Reason::Variant`] thì không: báo `kì → kỳ` là lỗi của engine, không phải
    /// lỗi của người viết.
    pub fn can_flag_real_word(self) -> bool {
        self != Reason::Variant
    }
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    /// Lý do đại diện. Với candidate nhiều phép sửa, đây là lý do **không phải
    /// biến thể** đầu tiên — xem [`Reason::can_flag_real_word`].
    pub reason: Reason,
    /// Số thành phần âm tiết bị đổi so với bản gốc: 1 hoặc 2.
    ///
    /// Có mặt vì hai phép sửa **a priori kém khả dĩ hơn** một phép sửa, và lớp
    /// phán quyết phải biết điều đó để đòi bằng chứng mạnh hơn. Xem
    /// [`crate::CheckOptions::extra_edit_margin`].
    pub edits: u8,
    /// Tần suất trong corpus — dùng xếp hạng. Luôn `> 0` vì candidate chưa chứng
    /// thực đã bị loại.
    pub frequency: u64,
}

// ---------------------------------------------------------------------------
// Bảng luật
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Rules {
    /// Âm đầu → các âm đầu hay bị nhầm với nó.
    onset: HashMap<&'static str, Vec<&'static str>>,
    /// Vần → các vần hay bị nhầm với nó, kèm cờ "đây là biến thể đều đúng".
    /// Suy ra từ luật `coda`/`nucleus` bằng cách áp lên từng vần đã biết rồi giữ
    /// lại kết quả cũng là vần thật.
    rime: HashMap<&'static str, Vec<(&'static str, bool)>>,
    /// Thanh → các thanh hay bị nhầm với nó.
    tone: HashMap<usize, Vec<usize>>,
}

fn tone_index(name: &str) -> Option<usize> {
    Some(match name {
        "ngang" => phonology::TONE_NGANG,
        "huyền" => phonology::TONE_HUYEN,
        "sắc" => phonology::TONE_SAC,
        "hỏi" => phonology::TONE_HOI,
        "ngã" => phonology::TONE_NGA,
        "nặng" => phonology::TONE_NANG,
        _ => return None,
    })
}

/// Thêm cặp song hướng vào bảng, không trùng lặp.
fn link<K: std::hash::Hash + Eq + Copy, V: PartialEq + Copy>(
    map: &mut HashMap<K, Vec<V>>,
    a: K,
    b: V,
) {
    let slot = map.entry(a).or_default();
    if !slot.contains(&b) {
        slot.push(b);
    }
}

fn rules() -> &'static Rules {
    static CACHE: OnceLock<Rules> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut r = Rules::default();
        // Luật vần khai báo ở dạng đoạn thay thế; ta cần bảng vần→vần nên phải áp
        // từng luật lên toàn bộ 160 vần đã biết. Giữ lại kết quả cũng là vần thật —
        // đó là bước tự lọc, khiến khai báo `nucleus o ô` không sinh ra `ôan` rác.
        let all_rimes: Vec<&'static str> =
            phonology::rimes().iter().map(|x| x.text.as_str()).collect();

        for line in RULES_TSV.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
            let ([kind, from, to] | [kind, from, to, _]) = cols[..] else {
                continue;
            };
            let is_variant = cols.get(3) == Some(&"variant");

            match kind {
                "onset" => {
                    link(&mut r.onset, from, to);
                    link(&mut r.onset, to, from);
                }
                "tone" => {
                    if let (Some(a), Some(b)) = (tone_index(from), tone_index(to)) {
                        link(&mut r.tone, a, b);
                        link(&mut r.tone, b, a);
                    }
                }
                "coda" | "nucleus" => {
                    for rime in &all_rimes {
                        for (f, t) in [(from, to), (to, from)] {
                            let swapped = if kind == "coda" {
                                match rime.strip_suffix(f) {
                                    Some(head) => format!("{head}{t}"),
                                    None => continue,
                                }
                            } else {
                                if !rime.contains(f) {
                                    continue;
                                }
                                rime.replacen(f, t, 1)
                            };
                            // Chỉ nhận nếu kết quả cũng là vần thật. Đây là bước tự
                            // lọc khiến khai báo `nucleus o ô` không sinh ra `ôan`.
                            if let Some(hit) = all_rimes.iter().find(|x| **x == swapped) {
                                link(&mut r.rime, *rime, (*hit, is_variant));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        r
    })
}

// ---------------------------------------------------------------------------
// Telex
// ---------------------------------------------------------------------------

/// Chữ ghép Telex → chữ có dấu phụ. Thứ tự quan trọng: xét `dd` trước để `dd`
/// không bị `d` đơn cắt mất.
const TELEX_DIGRAPHS: [(&str, &str); 7] = [
    ("dd", "đ"),
    ("aa", "â"),
    ("ee", "ê"),
    ("oo", "ô"),
    ("aw", "ă"),
    ("ow", "ơ"),
    ("uw", "ư"),
];

/// Phím thanh Telex đặt ở cuối âm tiết.
fn telex_tone_key(c: char) -> Option<usize> {
    Some(match c {
        's' => phonology::TONE_SAC,
        'f' => phonology::TONE_HUYEN,
        'r' => phonology::TONE_HOI,
        'x' => phonology::TONE_NGA,
        'j' => phonology::TONE_NANG,
        _ => return None,
    })
}

fn expand_telex(s: &str) -> String {
    let mut out = s.to_string();
    for (from, to) in TELEX_DIGRAPHS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Sinh candidate
// ---------------------------------------------------------------------------

/// Bộ thu candidate, tự lọc và khử trùng.
struct Collector<'a> {
    original: &'a str,
    out: Vec<Candidate>,
}

impl<'a> Collector<'a> {
    fn new(original: &'a str) -> Self {
        Self {
            original,
            out: Vec::new(),
        }
    }

    /// Nhận một candidate nếu nó hợp lệ, đã chứng thực, và khác bản gốc.
    fn offer(&mut self, text: Option<String>, reason: Reason, edits: u8) {
        let Some(text) = text else { return };
        if text == self.original {
            return;
        }
        // Đã chứng thực nghĩa là vừa hợp lệ ngữ âm vừa có người dùng thật.
        let frequency = dict::frequency(&text);
        if frequency == 0 {
            return;
        }
        // Cùng một chuỗi có thể đến từ nhiều đường. Giữ đường **rẻ nhất**: nếu
        // `sẵn` vừa đạt được bằng một phép sửa vừa bằng hai, thì nó là candidate
        // một phép sửa, và không được bắt nó trả giá của đường vòng.
        if let Some(existing) = self.out.iter_mut().find(|c| c.text == text) {
            if edits < existing.edits {
                existing.edits = edits;
                existing.reason = reason;
            }
            return;
        }
        self.out.push(Candidate {
            text,
            reason,
            edits,
            frequency,
        });
    }

    fn finish(mut self) -> Vec<Candidate> {
        // Ít phép sửa trước: ở cùng mức tần suất, cách giải thích đơn giản hơn gần
        // như luôn là cách đúng.
        self.out
            .sort_by_key(|c| (c.edits, std::cmp::Reverse(c.frequency), c.text.clone()));
        self.out
    }
}

/// Mọi candidate sửa lỗi cho một âm tiết, tần suất cao nhất trước.
///
/// Đầu vào phải là chữ thường, NFC — [`crate::token`] lo việc đó.
///
/// Hoạt động với **cả âm tiết sai và âm tiết đúng**: với âm tiết đúng nó trả về
/// các dạng hay bị nhầm, và đó là thứ L4 cần để phát hiện lỗi *real-word* như
/// `chia sẽ` (mọi âm tiết đều hợp lệ nhưng tổ hợp thì sai).
pub fn for_syllable(syllable: &str) -> Vec<Candidate> {
    let r = rules();
    let mut c = Collector::new(syllable);

    for a in phonology::decompose(syllable) {
        // Ba trục độc lập, mỗi trục bắt đầu bằng chính bản gốc (0 phép sửa). Tích
        // Descartes của chúng bao trọn cả candidate một phép sửa lẫn hai phép sửa
        // mà không phải viết hai nhánh riêng.
        let onsets = onset_axis(r, a.onset);
        let rimes = rime_axis(r, a.rime);
        let tones = tone_axis(r, a.onset, a.rime, a.tone);

        for &(onset, o) in &onsets {
            for &(rime, m) in &rimes {
                for &(tone, t) in &tones {
                    let edits = o.is_some() as u8 + m.is_some() as u8 + t.is_some() as u8;
                    if edits == 0 || edits > MAX_EDITS {
                        continue;
                    }
                    c.offer(
                        phonology::compose(onset, rime, tone),
                        pick_reason([o, m, t]),
                        edits,
                    );
                }
            }
        }
    }

    // Telex: chỉ thử khi âm tiết gốc không hợp lệ. `xoong` là vần `oo` thật, nếu
    // luôn bung Telex thì nó thành `xông` — sửa hỏng một từ đúng.
    if !phonology::is_valid_syllable(syllable) {
        for text in telex_candidates(syllable) {
            c.offer(Some(text), Reason::Telex, 1);
        }
    }

    c.finish()
}

/// Số thành phần âm tiết được phép đổi cùng lúc.
///
/// # Vì sao không phải 1
///
/// Bản đầu chỉ đổi **một** thành phần, và nó bỏ lọt nguyên một nhóm lỗi rất phổ
/// biến: lỗi cộng dồn. `sẳng sàng` → `sẵn sàng` cần đổi *cả* thanh (hỏi→ngã) *lẫn*
/// âm cuối (ng→n). Hai luật đó đều đã có sẵn trong bảng — chỉ là chưa bao giờ được
/// ghép lại. Với người viết mắc lỗi phát âm vùng miền, hai lỗi cùng lúc trên một âm
/// tiết là chuyện thường chứ không phải ngoại lệ, vì cùng một giọng nói sinh ra cả
/// hai.
///
/// # Vì sao không phải 3
///
/// Đổi cả ba thành phần thì gần như không còn ràng buộc nào với bản gốc, và mô hình
/// ngôn ngữ sẽ chọn âm tiết hợp ngữ cảnh thay vì âm tiết người viết định gõ. Đó là
/// lúc "sửa lỗi chính tả" lặng lẽ biến thành "viết lại hộ".
const MAX_EDITS: u8 = 2;

/// Một trục biến đổi: giá trị gốc kèm `None`, rồi các phương án kèm lý do.
type Axis<T> = Vec<(T, Option<Reason>)>;

fn onset_axis(r: &'static Rules, onset: &'static str) -> Axis<&'static str> {
    let mut out: Axis<&str> = vec![(onset, None)];
    for &alt in r.onset.get(onset).into_iter().flatten() {
        out.push((alt, Some(Reason::Onset)));
    }
    out
}

fn rime_axis(r: &'static Rules, rime: &'static str) -> Axis<&'static str> {
    let mut out: Axis<&str> = vec![(rime, None)];
    for &(alt, is_variant) in r.rime.get(rime).into_iter().flatten() {
        let reason = if is_variant {
            Reason::Variant
        } else {
            Reason::Rime
        };
        out.push((alt, Some(reason)));
    }
    out
}

fn tone_axis(r: &'static Rules, onset: &str, rime: &str, tone: usize) -> Axis<usize> {
    let mut out: Axis<usize> = vec![(tone, None)];
    for &alt in r.tone.get(&tone).into_iter().flatten() {
        out.push((alt, Some(Reason::ToneConfusion)));
    }
    // Thanh không hợp với vần đóng (`mat` không tồn tại) → mọi thanh khác đều là
    // ứng viên, vì ta biết chắc thanh hiện tại sai.
    if phonology::compose(onset, rime, tone).is_none() {
        for alt in 0..phonology::TONE_COUNT {
            if !out.iter().any(|(t, _)| *t == alt) {
                out.push((alt, Some(Reason::ToneRepair)));
            }
        }
    }
    out
}

/// Lý do đại diện cho một candidate nhiều phép sửa.
///
/// Ưu tiên lý do **không phải biến thể**: nếu trong hai phép sửa có một phép sửa lỗi
/// thật thì candidate đó là đề xuất sửa lỗi thật, và được quyền làm căn cứ báo lỗi
/// real-word. Chỉ khi mọi phép sửa đều là biến thể đều-đúng thì candidate mới mang
/// nhãn [`Reason::Variant`] và mất quyền đó.
fn pick_reason(reasons: [Option<Reason>; 3]) -> Reason {
    let present: Vec<Reason> = reasons.into_iter().flatten().collect();
    present
        .iter()
        .copied()
        .find(|r| *r != Reason::Variant)
        .unwrap_or(present[0])
}

/// Phục hồi chuỗi gõ Telex khi bộ gõ tắt: `chinhs` → `chính`, `ddi` → `đi`.
fn telex_candidates(syllable: &str) -> Vec<String> {
    let mut out = Vec::new();

    // Chỉ bung chữ ghép
    let expanded = expand_telex(syllable);
    if expanded != syllable && phonology::is_valid_syllable(&expanded) {
        out.push(expanded.clone());
    }

    // Bung chữ ghép + phím thanh cuối
    let mut chars = expanded.chars();
    if let Some(last) = chars.next_back() {
        if let Some(tone) = telex_tone_key(last) {
            let stem: String = chars.collect();
            for a in phonology::decompose(&stem) {
                if let Some(text) = phonology::compose(a.onset, a.rime, tone) {
                    if !out.contains(&text) {
                        out.push(text);
                    }
                }
            }
        }
    }
    out
}

/// Số liệu bảng luật, cho `writa-cli rules`.
pub fn rule_stats() -> (usize, usize, usize) {
    let r = rules();
    (
        r.onset.values().map(Vec::len).sum(),
        r.rime.values().map(Vec::len).sum(),
        r.tone.values().map(Vec::len).sum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(syllable: &str) -> Vec<String> {
        for_syllable(syllable).into_iter().map(|c| c.text).collect()
    }

    fn suggests(syllable: &str, want: &str) -> bool {
        texts(syllable).iter().any(|t| t == want)
    }

    #[test]
    fn rules_table_is_populated() {
        let (onset, rime, tone) = rule_stats();
        assert!(onset >= 20, "luật âm đầu quá ít: {onset}");
        assert!(rime >= 100, "luật vần quá ít: {rime}");
        assert!(tone >= 2, "luật thanh quá ít: {tone}");
    }

    #[test]
    fn suggests_hoi_nga_swap() {
        // Lỗi số một của người Việt.
        assert!(suggests("sẽ", "sẻ"));
        assert!(suggests("sẻ", "sẽ"));
        assert!(suggests("nghỉ", "nghĩ"));
        assert!(suggests("nghĩ", "nghỉ"));
        assert!(suggests("mẫu", "mẩu"));
        assert!(suggests("bãi", "bải") || suggests("bải", "bãi"));
    }

    #[test]
    fn repairs_orthographic_onset_errors() {
        // Đây là loại lỗi L1 bắt được; L3 phải sửa được nó.
        assert!(suggests("nghành", "ngành"));
        assert!(suggests("ngiên", "nghiên"));
        assert!(suggests("ngi", "nghi"));
    }

    #[test]
    fn suggests_onset_confusions() {
        assert!(suggests("xử", "sử"));
        assert!(suggests("sử", "xử"));
        assert!(suggests("chuyện", "truyện"));
        assert!(suggests("truyện", "chuyện"));
        assert!(suggests("dành", "giành"));
        assert!(suggests("giành", "dành"));
        assert!(suggests("lên", "nên"));
    }

    #[test]
    fn suggests_coda_and_nucleus_confusions() {
        assert!(suggests("hoàn", "hoàng"));
        assert!(suggests("hoàng", "hoàn"));
        assert!(suggests("mất", "mác") || suggests("bắt", "bắc"));
        assert!(suggests("kỹ", "kĩ"));
    }

    #[test]
    fn repairs_tone_on_checked_rimes() {
        // Vần đóng bằng t chỉ nhận sắc/nặng, nên `mat` sửa thành `mát`/`mạt`.
        let got = texts("mat");
        assert!(got.contains(&"mát".to_string()), "thiếu mát: {got:?}");
        assert!(got.contains(&"mạt".to_string()), "thiếu mạt: {got:?}");
    }

    #[test]
    fn recovers_telex_typed_without_ime() {
        assert!(suggests("chinhs", "chính"), "{:?}", texts("chinhs"));
        assert!(suggests("tieengs", "tiếng"), "{:?}", texts("tieengs"));
        assert!(suggests("ddi", "đi"), "{:?}", texts("ddi"));
        assert!(
            suggests("hoaf", "hoà") || suggests("hoaf", "hòa"),
            "{:?}",
            texts("hoaf")
        );
    }

    #[test]
    fn telex_does_not_break_valid_syllables() {
        // `xoong` dùng vần `oo` thật. Nếu luôn bung Telex thì nó thành `xông` —
        // sửa hỏng một từ vốn đúng.
        assert!(!suggests("xoong", "xông"), "{:?}", texts("xoong"));
        assert!(!suggests("đoóc", "đốc"));
    }

    #[test]
    fn never_suggests_an_unattested_syllable() {
        // Bất biến: mọi đề xuất phải là âm tiết hợp lệ VÀ có người dùng thật.
        for s in [
            "sẽ", "nghành", "mat", "chinhs", "hoàn", "xử", "kỹ", "tieengs",
        ] {
            for c in for_syllable(s) {
                assert!(
                    phonology::is_valid_syllable(&c.text),
                    "{s} → {} không hợp lệ",
                    c.text
                );
                assert!(
                    dict::is_attested(&c.text),
                    "{s} → {} chưa chứng thực",
                    c.text
                );
                assert!(c.frequency > 0);
            }
        }
    }

    #[test]
    fn never_suggests_the_input_itself() {
        for s in ["sẻ", "tiếng", "nghành", "mat"] {
            assert!(!texts(s).contains(&s.to_string()));
        }
    }

    #[test]
    fn ranks_by_edit_count_then_corpus_frequency() {
        // Thứ tự là (ít phép sửa trước, rồi tần suất giảm dần). Số phép sửa đứng
        // trước vì ở cùng mức tần suất, cách giải thích đơn giản hơn gần như luôn là
        // cách đúng — `sẻ` (một phép sửa từ `sẽ`) phải đứng trên mọi phương án phải
        // đổi hai thành phần mới tới được.
        let got = for_syllable("sẽ");
        assert!(!got.is_empty());
        for pair in got.windows(2) {
            assert!(pair[0].edits <= pair[1].edits, "chưa xếp theo số phép sửa");
            if pair[0].edits == pair[1].edits {
                assert!(
                    pair[0].frequency >= pair[1].frequency,
                    "trong cùng số phép sửa, chưa xếp theo tần suất giảm dần"
                );
            }
        }
    }

    #[test]
    fn combines_two_rules_when_one_is_not_enough() {
        // Lý do tồn tại của MAX_EDITS = 2. `sẳng` → `sẵn` cần đổi CẢ thanh
        // (hỏi→ngã) LẪN âm cuối (ng→n). Hai luật đều có sẵn trong bảng từ đầu; bản
        // chỉ-một-phép-sửa không bao giờ ghép chúng lại nên bỏ lọt hẳn nhóm lỗi này.
        let got = for_syllable("sẳng");
        let hit = got.iter().find(|c| c.text == "sẵn");
        assert!(hit.is_some(), "thiếu `sẵn`: {:?}", texts("sẳng"));
        assert_eq!(hit.unwrap().edits, 2);

        // Và phương án một phép sửa vẫn phải có mặt, vẫn đứng trước.
        assert!(got.iter().any(|c| c.text == "sẵng" && c.edits == 1));
        let first_two_edit = got.iter().position(|c| c.edits == 2).unwrap_or(got.len());
        let last_one_edit = got.iter().rposition(|c| c.edits == 1).unwrap_or(0);
        assert!(last_one_edit < first_two_edit);
    }

    #[test]
    fn a_genuine_fix_paired_with_a_variant_can_still_flag() {
        // `Reason::Variant` (`i`↔`y`) một mình không được làm căn cứ báo lỗi. Nhưng
        // khi nó đi kèm một phép sửa lỗi thật thì candidate đó LÀ một đề xuất sửa
        // lỗi, và mất quyền báo là sai.
        assert_eq!(
            pick_reason([Some(Reason::Variant), Some(Reason::Rime), None]),
            Reason::Rime
        );
        assert_eq!(
            pick_reason([Some(Reason::Variant), None, None]),
            Reason::Variant
        );
        assert!(!Reason::Variant.can_flag_real_word());
    }

    #[test]
    fn candidate_count_stays_bounded() {
        // Bảng luật rộng, nhưng ba lớp lọc phải giữ số candidate ở mức dùng được.
        // Nếu con số này phình lên, L4 sẽ chậm và dễ chọn sai.
        for s in ["sẽ", "tiếng", "hoàn", "nghành", "an", "mat"] {
            let n = for_syllable(s).len();
            assert!(n <= 40, "{s} sinh {n} candidate — quá nhiều");
        }
    }
}
