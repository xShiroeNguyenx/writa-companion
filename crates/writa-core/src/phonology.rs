//! L1 — Ngữ âm & tập âm tiết hợp lệ.
//!
//! # Vì sao lớp này là nền của cả engine
//!
//! Khác tiếng Anh, tiếng Việt có **tập âm tiết ĐÓNG và sinh được**: chỉ khoảng
//! 18 nghìn âm tiết hợp lệ, dựng từ `âm đầu × vần × thanh` với vài ràng buộc.
//! Hệ quả: âm tiết **không nằm trong tập** là **sai chính tả chắc chắn**, không
//! cần ngữ cảnh, không cần model. Đây là lớp duy nhất có precision 100%, và là
//! lớp duy nhất được phép auto-fix.
//!
//! Quan trọng không kém: ta **tự sinh** tập này từ bảng ngữ âm của mình
//! (`data/phonology/`), nên không phái sinh từ từ điển GPL nào — giữ được
//! license MIT cho toàn dự án.
//!
//! # Hướng lệch có chủ đích
//!
//! Bảng vần được để **hơi rộng** (ví dụ mọi vần đều cho phép âm đầu rỗng, kể cả
//! khi thực tế không có từ nào). Lý do: sai theo hướng rộng gây *bỏ sót lỗi*,
//! sai theo hướng hẹp gây *báo lỗi oan*. Với một tool chạy nền real-time, báo oan
//! là thứ giết niềm tin — nên luôn lệch về phía rộng.

use std::collections::BTreeSet;
use std::sync::OnceLock;

const ONSETS_TSV: &str = include_str!("../../../data/phonology/onsets.tsv");
const RIMES_TSV: &str = include_str!("../../../data/phonology/rimes.tsv");

// ---------------------------------------------------------------------------
// Thanh điệu
// ---------------------------------------------------------------------------

/// Thứ tự thanh dùng xuyên suốt: ngang, huyền, sắc, hỏi, ngã, nặng.
pub const TONE_COUNT: usize = 6;

pub const TONE_NGANG: usize = 0;
pub const TONE_HUYEN: usize = 1;
pub const TONE_SAC: usize = 2;
pub const TONE_HOI: usize = 3;
pub const TONE_NGA: usize = 4;
pub const TONE_NANG: usize = 5;

/// Nguyên âm cơ sở → 6 dạng mang thanh.
const TONE_TABLE: &[(char, [char; TONE_COUNT])] = &[
    ('a', ['a', 'à', 'á', 'ả', 'ã', 'ạ']),
    ('ă', ['ă', 'ằ', 'ắ', 'ẳ', 'ẵ', 'ặ']),
    ('â', ['â', 'ầ', 'ấ', 'ẩ', 'ẫ', 'ậ']),
    ('e', ['e', 'è', 'é', 'ẻ', 'ẽ', 'ẹ']),
    ('ê', ['ê', 'ề', 'ế', 'ể', 'ễ', 'ệ']),
    ('i', ['i', 'ì', 'í', 'ỉ', 'ĩ', 'ị']),
    ('o', ['o', 'ò', 'ó', 'ỏ', 'õ', 'ọ']),
    ('ô', ['ô', 'ồ', 'ố', 'ổ', 'ỗ', 'ộ']),
    ('ơ', ['ơ', 'ờ', 'ớ', 'ở', 'ỡ', 'ợ']),
    ('u', ['u', 'ù', 'ú', 'ủ', 'ũ', 'ụ']),
    ('ư', ['ư', 'ừ', 'ứ', 'ử', 'ữ', 'ự']),
    ('y', ['y', 'ỳ', 'ý', 'ỷ', 'ỹ', 'ỵ']),
];

/// Nguyên âm trước — quyết định chọn c/k, g/gh, ng/ngh.
const FRONT_VOWELS: [char; 4] = ['e', 'ê', 'i', 'y'];

/// Ba vần mà **cả hai vị trí đặt dấu đều được coi là đúng** theo các quy chuẩn
/// khác nhau: `hòa`/`hoà`, `khỏe`/`khoẻ`, `thúy`/`thuý`.
///
/// Engine phải sinh CẢ HAI dạng, và tuyệt đối không báo lỗi dạng nào. Việc
/// chuẩn hoá về một kiểu chỉ là rule tuỳ chọn (opt-in) ở lớp L5.
const AMBIGUOUS_TONE_PLACEMENT: [&str; 3] = ["oa", "oe", "uy"];

fn is_base_vowel(c: char) -> bool {
    TONE_TABLE.iter().any(|(b, _)| *b == c)
}

fn apply_tone(chars: &mut [char], pos: usize, tone: usize) {
    if let Some((_, forms)) = TONE_TABLE.iter().find(|(b, _)| *b == chars[pos]) {
        chars[pos] = forms[tone];
    }
}

// ---------------------------------------------------------------------------
// Âm đầu
// ---------------------------------------------------------------------------

/// Lớp nguyên âm mà một âm đầu được phép đứng trước.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NucleusClass {
    /// Chỉ trước nguyên âm trước (e, ê, i, y) — gh, ngh, k
    Front,
    /// Chỉ trước nguyên âm sau/giữa (a, ă, â, o, ô, ơ, u, ư) — c, g, ng
    Back,
    /// Trước mọi vần
    Any,
}

#[derive(Clone, Debug)]
pub struct Onset {
    pub text: String,
    pub class: NucleusClass,
}

#[derive(Clone, Debug)]
pub struct Rime {
    /// Dạng chữ viết, chưa mang thanh (ví dụ `"iêng"`).
    pub text: String,
    /// Vần chỉ dùng khi âm đầu rỗng hoặc âm đầu là `"qu"`.
    ///
    /// Nhóm vần bắt đầu bằng `y`: `yên`, `yêu`, `y` → cho ra `yên`, `yêu`,
    /// `y tế`, và cùng `qu` cho ra `quyên`, `quyết`, `quy`.
    pub qu_or_bare_only: bool,
}

impl Rime {
    /// Vần đóng bằng p/t/c/ch chỉ nhận thanh sắc và nặng.
    ///
    /// `mất`/`mặt` hợp lệ; `mat`, `màt`, `mảt`, `mãt` đều không. Suy ra bằng luật
    /// thay vì khai báo tay trong TSV để loại bỏ một lớp lỗi nhập liệu.
    pub fn is_checked(&self) -> bool {
        // "ch" phải xét riêng: nó kết thúc bằng 'h', không rơi vào tập dưới.
        self.text.ends_with("ch") || self.text.ends_with(['c', 'p', 't'])
    }

    pub fn allowed_tones(&self) -> &'static [usize] {
        if self.is_checked() {
            &[TONE_SAC, TONE_NANG]
        } else {
            &[
                TONE_NGANG, TONE_HUYEN, TONE_SAC, TONE_HOI, TONE_NGA, TONE_NANG,
            ]
        }
    }

    /// Chữ cái đầu của vần — cái quyết định chọn c/k, g/gh, ng/ngh.
    ///
    /// Xét theo CHỮ CÁI đầu (không phải nhân âm tiết) là đúng về chính tả:
    /// `nguyên` = ng + uyên (bắt đầu `u` → lớp Back → `ng` hợp lệ), còn
    /// `nghuyên` bị loại vì `ngh` đòi nguyên âm trước.
    fn first_letter(&self) -> char {
        self.text.chars().next().unwrap_or(' ')
    }

    /// Tách vần thành (cụm nguyên âm, âm cuối).
    ///
    /// Mọi vần tiếng Việt đều có dạng nguyên-âm-trước-phụ-âm-sau:
    /// `iêng` → (`iê`, `ng`), `oanh` → (`oa`, `nh`), `uyêt` → (`uyê`, `t`).
    fn split(&self) -> (Vec<char>, Vec<char>) {
        let chars: Vec<char> = self.text.chars().collect();
        let cut = chars
            .iter()
            .position(|c| !is_base_vowel(*c))
            .unwrap_or(chars.len());
        (chars[..cut].to_vec(), chars[cut..].to_vec())
    }

    /// Vị trí (index trong cụm nguyên âm) sẽ nhận dấu thanh.
    ///
    /// Trả về **hai** vị trí cho `oa`/`oe`/`uy` vì cả hai cách đặt dấu đều đúng.
    ///
    /// Thứ tự luật:
    /// 1. Có `ê` hoặc `ơ` → đặt lên đó. (`tiếng`, `được`, `người`, `quyết`, `rượu`)
    /// 2. Có âm cuối → đặt lên nguyên âm CUỐI của cụm. (`oán`, `uốn`, `xuất`, `huých`)
    /// 3. Không âm cuối, 3 nguyên âm → đặt lên nguyên âm GIỮA. (`oái`, `muối`, `khuỷu`)
    /// 4. Không âm cuối, ≤2 nguyên âm → đặt lên nguyên âm ĐẦU. (`ái`, `áo`, `mía`, `múa`)
    fn tone_positions(&self) -> Vec<usize> {
        let (v, coda) = self.split();
        if v.is_empty() {
            return Vec::new();
        }
        if let Some(i) = v.iter().position(|c| *c == 'ê') {
            return vec![i];
        }
        if let Some(i) = v.iter().position(|c| *c == 'ơ') {
            return vec![i];
        }
        if !coda.is_empty() {
            return vec![v.len() - 1];
        }
        if AMBIGUOUS_TONE_PLACEMENT.contains(&self.text.as_str()) {
            return vec![0, 1];
        }
        match v.len() {
            1 | 2 => vec![0],
            _ => vec![1],
        }
    }

    /// Mọi dạng mang thanh của vần này (đã gộp biến thể vị trí dấu).
    pub fn toned_forms(&self) -> Vec<String> {
        let positions = self.tone_positions();
        let base: Vec<char> = self.text.chars().collect();
        let mut out = BTreeSet::new();
        for &tone in self.allowed_tones() {
            for &pos in &positions {
                let mut chars = base.clone();
                apply_tone(&mut chars, pos, tone);
                out.insert(chars.into_iter().collect::<String>());
            }
        }
        out.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Parse bảng
// ---------------------------------------------------------------------------

fn parse_lines(tsv: &str) -> impl Iterator<Item = Vec<&str>> {
    tsv.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').map(str::trim).collect())
}

pub fn onsets() -> &'static [Onset] {
    static CACHE: OnceLock<Vec<Onset>> = OnceLock::new();
    CACHE.get_or_init(|| {
        parse_lines(ONSETS_TSV)
            .map(|f| Onset {
                text: f[0].to_string(),
                class: match f.get(1).copied().unwrap_or("A") {
                    "F" => NucleusClass::Front,
                    "B" => NucleusClass::Back,
                    _ => NucleusClass::Any,
                },
            })
            .collect()
    })
}

pub fn rimes() -> &'static [Rime] {
    static CACHE: OnceLock<Vec<Rime>> = OnceLock::new();
    CACHE.get_or_init(|| {
        parse_lines(RIMES_TSV)
            .map(|f| Rime {
                text: f[0].to_string(),
                qu_or_bare_only: f.get(1).copied() == Some("q"),
            })
            .collect()
    })
}

// ---------------------------------------------------------------------------
// Sinh tập âm tiết
// ---------------------------------------------------------------------------

/// Âm đầu này có được đứng trước vần bắt đầu bằng `first` không?
fn onset_fits(onset: &Onset, rime: &Rime) -> bool {
    let is_front = FRONT_VOWELS.contains(&rime.first_letter());
    let class_ok = match onset.class {
        NucleusClass::Front => is_front,
        NucleusClass::Back => !is_front,
        NucleusClass::Any => true,
    };
    if !class_ok {
        return false;
    }

    // Vần nhóm y chỉ đi với âm đầu rỗng hoặc "qu": quyên, quyết, quy.
    if rime.qu_or_bare_only && onset.text != "qu" {
        return false;
    }

    true
}

/// Nối âm đầu với vần đã mang thanh, xử lý **nhập chữ**.
///
/// Khi chữ cái cuối của âm đầu trùng chữ cái đầu của vần, chính tả tiếng Việt
/// viết chữ đó **một lần**:
///
/// ```text
/// gi + ì    → gì       (không phải "giì")
/// gi + ìn   → gìn
/// gi + iêng → giêng
/// qu + uynh → quynh    (quỳnh)
/// qu + uyên → quyên
/// ```
///
/// Trước đây chỗ này *chặn* các cặp đó thay vì nhập chữ — vòng verify đối chiếu
/// corpus phát hiện ra: `gì` (5462 lần), `quỳnh` (1242), `gìn` (615) đều bị loại
/// oan. Chữ bị bỏ là chữ cuối của ÂM ĐẦU, vì dấu thanh nằm trên nguyên âm của vần.
fn join(onset: &str, rime_first_letter: char, toned_rime: &str) -> String {
    if onset.ends_with(rime_first_letter) {
        let mut head = onset.to_string();
        head.pop();
        format!("{head}{toned_rime}")
    } else {
        format!("{onset}{toned_rime}")
    }
}

/// Sinh toàn bộ tập âm tiết hợp lệ, chữ thường, đã chuẩn hoá NFC.
///
/// Tự khử trùng lặp: nhiều cặp `(âm đầu, vần)` cho ra cùng một chuỗi —
/// `gi + a` và `g + ia` đều ra `gia`; `gi + êng` và `gi + iêng` (nhập chữ) đều
/// ra `giêng`; `qu + yên` và `qu + uyên` (nhập chữ) đều ra `quyên`.
pub fn generate_syllables() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for rime in rimes() {
        let first = rime.first_letter();
        for toned in rime.toned_forms() {
            // Âm đầu rỗng — mọi vần đều cho phép (xem "hướng lệch có chủ đích").
            out.insert(toned.clone());
            for onset in onsets() {
                if onset_fits(onset, rime) {
                    out.insert(join(&onset.text, first, &toned));
                }
            }
        }
    }
    out
}

/// Tập âm tiết, dựng một lần rồi cache.
pub fn syllable_set() -> &'static BTreeSet<String> {
    static CACHE: OnceLock<BTreeSet<String>> = OnceLock::new();
    CACHE.get_or_init(generate_syllables)
}

/// Âm tiết có hợp lệ về chính tả không?
///
/// Đầu vào phải là chữ thường và đã chuẩn hoá NFC — lớp L0 lo việc đó.
/// Ở đây chưa normalize để giữ hàm này ở mức chi phí một lần tra cứu.
pub fn is_valid_syllable(s: &str) -> bool {
    syllable_set().contains(s)
}

// ---------------------------------------------------------------------------
// Phân tích ngược — nền của L3
// ---------------------------------------------------------------------------

/// Một cách phân tích âm tiết thành ba thành phần.
///
/// L3 sinh candidate bằng cách thay **một** thành phần rồi dựng lại. Nhờ đó
/// candidate luôn có lý do ngữ âm cụ thể (đổi thanh, đổi âm đầu, đổi vần) thay vì
/// là kết quả của Levenshtein mù.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Analysis {
    /// Âm đầu, `""` nếu không có.
    pub onset: &'static str,
    /// Vần **chưa mang thanh**.
    pub rime: &'static str,
    pub tone: usize,
}

/// Tách dấu thanh: trả về (dạng không thanh, chỉ số thanh).
///
/// `tiếng` → (`tieng`… không, → `tiêng`, sắc): chỉ bỏ THANH, giữ nguyên dấu phụ
/// (mũ, móc, á) vì chúng thuộc chữ cái chứ không phải thanh điệu.
pub fn strip_tone(s: &str) -> (String, usize) {
    let mut tone = TONE_NGANG;
    let out = s
        .chars()
        .map(|c| {
            for (base, forms) in TONE_TABLE {
                if let Some(i) = forms.iter().position(|f| *f == c) {
                    if i != TONE_NGANG {
                        tone = i;
                    }
                    return *base;
                }
            }
            c
        })
        .collect();
    (out, tone)
}

fn rime_by_text(text: &str) -> Option<&'static Rime> {
    static CACHE: OnceLock<std::collections::HashMap<&'static str, &'static Rime>> =
        OnceLock::new();
    CACHE
        .get_or_init(|| rimes().iter().map(|r| (r.text.as_str(), r)).collect())
        .get(text)
        .copied()
}

fn onset_by_text(text: &str) -> Option<&'static Onset> {
    onsets().iter().find(|o| o.text == text)
}

/// Mọi cách phân tích cấu trúc của một chuỗi thành (âm đầu, vần, thanh).
///
/// # Khoan dung ở đầu vào — có chủ đích
///
/// Hàm này **cố tình phân tích được cả âm tiết SAI**. Đó không phải lỗ hổng mà là
/// mục đích chính: muốn sửa `nghành` thành `ngành` thì trước hết phải biết nó là
/// `ngh` + `anh` + huyền, rồi mới áp được luật `ngh`→`ng`. Tương tự `mat` phân tích
/// được thành `m` + `at` + ngang, và từ đó L3 thấy vần `at` chỉ nhận sắc/nặng nên
/// đề xuất `mát`/`mạt`.
///
/// Cặp đôi với nó là [`compose`], vốn **nghiêm ngặt**: chỉ trả về `Some` khi tổ hợp
/// thật sự hợp lệ. Khoan dung khi phân tích + nghiêm ngặt khi dựng lại = L3 sửa
/// được lỗi mà không bao giờ đề xuất một âm tiết không tồn tại.
///
/// Trả về **nhiều** kết quả vì nhập chữ khiến một chuỗi có hơn một cách đọc: `gì`
/// đọc được thành `gi`+`ì` (nhập chữ) và `g`+`ì` (trực tiếp). Cả hai đều sinh ra
/// candidate hữu ích nên trả hết.
///
/// Rỗng nếu chuỗi không tách được thành âm đầu + vần nào cả (`xyz`, `qwerty`).
pub fn decompose(syllable: &str) -> Vec<Analysis> {
    let (untoned, tone) = strip_tone(syllable);
    let mut out = Vec::new();

    let mut push = |onset: &'static str, rime: &'static str| {
        let a = Analysis { onset, rime, tone };
        if !out.contains(&a) {
            out.push(a);
        }
    };

    // Âm đầu rỗng
    if let Some(r) = rime_by_text(&untoned) {
        push("", r.text.as_str());
    }

    for onset in onsets() {
        // Nối trực tiếp: âm tiết = âm đầu + vần
        if let Some(rest) = untoned.strip_prefix(onset.text.as_str()) {
            if let Some(r) = rime_by_text(rest) {
                push(onset.text.as_str(), r.text.as_str());
            }
        }

        // Nhập chữ: âm tiết = âm đầu-bỏ-chữ-cuối + vần, và vần phải bắt đầu đúng
        // bằng chữ cuối đó. Đây là đường sinh ra `gì`, `quynh`, `giêng`.
        let mut head = onset.text.clone();
        if let Some(last) = head.pop() {
            if !head.is_empty() {
                if let Some(rest) = untoned.strip_prefix(&head) {
                    if rest.starts_with(last) {
                        if let Some(r) = rime_by_text(rest) {
                            push(onset.text.as_str(), r.text.as_str());
                        }
                    }
                }
            }
        }
    }

    out
}

/// Dựng lại âm tiết từ ba thành phần. `None` nếu tổ hợp không hợp lệ.
///
/// Áp đúng các ràng buộc của [`generate_syllables`], nên mọi thứ hàm này trả về
/// đều nằm trong tập âm tiết hợp lệ. Đó là điều kiện để L3 không bao giờ đề xuất
/// một âm tiết không tồn tại.
pub fn compose(onset: &str, rime: &str, tone: usize) -> Option<String> {
    let rime_def = rime_by_text(rime)?;
    if !rime_def.allowed_tones().contains(&tone) {
        return None;
    }

    let positions = rime_def.tone_positions();
    let pos = *positions.first()?;
    let mut chars: Vec<char> = rime.chars().collect();
    apply_tone(&mut chars, pos, tone);
    let toned: String = chars.into_iter().collect();

    if onset.is_empty() {
        return Some(toned);
    }
    let onset_def = onset_by_text(onset)?;
    if !onset_fits(onset_def, rime_def) {
        return None;
    }
    Some(join(onset, rime_def.first_letter(), &toned))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Âm tiết thật, PHẢI được chấp nhận. Bỏ sót ở đây = báo lỗi oan cho user.
    const MUST_ACCEPT: &[&str] = &[
        // thường gặp
        "tôi", "yêu", "tiếng", "việt", "của", "những", "được", "người", "không", "chia", "sẻ", "sẽ",
        "sửa", "sữa", "nghĩ", "nghỉ", "ngành", "nghe", // ràng buộc c/k, g/gh, ng/ngh
        "kỹ", "kì", "kê", "ki", "cá", "cô", "cứ", "ghi", "ghế", "gà", "gu", "nghiêng", "nghĩa",
        "ngoài", "nguy", "ngư", // nhóm vần y: âm đầu rỗng hoặc qu
        "yên", "yếu", "y", "quy", "quý", "quyên", "quyền", "quyết", "quyển",
        // âm đệm
        "nguyên", "khuyên", "chuyện", "truyện", "hoàn", "hoàng", "xuất", "khuỷu", "quê", "quà",
        "thuở", "huých", // nguyên âm đôi
        "muối", "uống", "rượu", "được", "người", "mía", "múa", "ứa", "cứu",
        // vần đóng: sắc và nặng
        "mất", "mặt", "sách", "sạch", "các", "lạc", "ích", "ịch", "việc", "việt",
        // biến thể vị trí dấu — CẢ HAI dạng đều đúng
        "hòa", "hoà", "khỏe", "khoẻ", "thúy", "thuý",
        // gi / g + i cùng cho ra một chuỗi
        "gia", "giêng", "giá", "giữ", "giành", "dành",
        // NHẬP CHỮ — do vòng verify đối chiếu corpus phát hiện.
        // Ba token này từng bị loại oan vì luật cũ *chặn* thay vì nhập chữ.
        "gì", "gìn", "quỳnh", "giếng", "quýnh",
    ];

    /// Âm tiết KHÔNG hợp lệ, phải bị loại. Đây là loại lỗi L1 bắt với precision 100%.
    const MUST_REJECT: &[&str] = &[
        // ngh + nguyên âm sau  (lỗi "nghành" cực phổ biến)
        "nghành", "nghạc", "nghoài", "nghuyên", // ng + nguyên âm trước
        "ngiên", "ngi", "nge", // gh / g đặt sai
        "ge", "gê", "ghà", "ghu", // c / k đặt sai
        "ce", "cê", "ci", "ka", "ko", "ku", // vần đóng mang thanh không cho phép
        "mat", "màt", "mảt", "mãt", "sach", "sàch", "lac", // vần không tồn tại
        "quyêt", "tieng", "ngiêng", "xuâts", // trùng chữ cái nối
        "quuyên", "giiêng",
    ];

    #[test]
    fn accepts_real_syllables() {
        let set = syllable_set();
        let missing: Vec<&str> = MUST_ACCEPT
            .iter()
            .copied()
            .filter(|s| !set.contains(*s))
            .collect();
        assert!(
            missing.is_empty(),
            "âm tiết thật bị loại oan (bảng vần/âm đầu còn thiếu): {missing:?}"
        );
    }

    #[test]
    fn rejects_invalid_syllables() {
        let set = syllable_set();
        let leaked: Vec<&str> = MUST_REJECT
            .iter()
            .copied()
            .filter(|s| set.contains(*s))
            .collect();
        assert!(
            leaked.is_empty(),
            "âm tiết sai lọt vào tập (ràng buộc quá lỏng): {leaked:?}"
        );
    }

    #[test]
    fn checked_rimes_only_take_sac_and_nang() {
        let r = Rime {
            text: "ach".to_string(),
            qu_or_bare_only: false,
        };
        assert!(r.is_checked());
        assert_eq!(r.toned_forms(), vec!["ách".to_string(), "ạch".to_string()]);
    }

    #[test]
    fn tone_lands_on_e_circumflex_and_o_horn() {
        let forms = |t: &str| {
            Rime {
                text: t.to_string(),
                qu_or_bare_only: false,
            }
            .toned_forms()
        };
        // ê / ơ luôn thắng mọi luật khác
        assert!(forms("iêng").contains(&"iếng".to_string()));
        assert!(forms("uyêt").contains(&"uyết".to_string()));
        assert!(forms("ươu").contains(&"ượu".to_string()));
        assert!(forms("ươi").contains(&"ười".to_string()));
        // không có ê/ơ, có âm cuối → nguyên âm cuối của cụm
        assert!(forms("uôn").contains(&"uốn".to_string()));
        assert!(forms("oan").contains(&"oán".to_string()));
        assert!(forms("uât").contains(&"uất".to_string()));
        // không âm cuối, 3 nguyên âm → nguyên âm giữa
        assert!(forms("uôi").contains(&"uối".to_string()));
        assert!(forms("oai").contains(&"oái".to_string()));
        // không âm cuối, 2 nguyên âm → nguyên âm đầu
        assert!(forms("ai").contains(&"ái".to_string()));
        assert!(forms("ua").contains(&"úa".to_string()));
    }

    #[test]
    fn ambiguous_placement_yields_both_variants() {
        for (rime, a, b) in [("oa", "óa", "oá"), ("oe", "óe", "oé"), ("uy", "úy", "uý")] {
            let forms = Rime {
                text: rime.to_string(),
                qu_or_bare_only: false,
            }
            .toned_forms();
            assert!(forms.contains(&a.to_string()), "{rime}: thiếu {a}");
            assert!(forms.contains(&b.to_string()), "{rime}: thiếu {b}");
        }
    }

    #[test]
    fn strips_tone_but_keeps_letter_diacritics() {
        // Chỉ bỏ THANH; mũ/móc/á thuộc chữ cái nên phải giữ.
        assert_eq!(strip_tone("tiếng"), ("tiêng".to_string(), TONE_SAC));
        assert_eq!(strip_tone("được"), ("đươc".to_string(), TONE_NANG));
        assert_eq!(strip_tone("hòa"), ("hoa".to_string(), TONE_HUYEN));
        assert_eq!(strip_tone("sẽ"), ("se".to_string(), TONE_NGA));
        assert_eq!(strip_tone("sẻ"), ("se".to_string(), TONE_HOI));
        assert_eq!(strip_tone("tôi"), ("tôi".to_string(), TONE_NGANG));
    }

    #[test]
    fn decomposes_common_syllables() {
        let find = |s: &str, onset: &str, rime: &str, tone: usize| {
            decompose(s)
                .iter()
                .any(|a| a.onset == onset && a.rime == rime && a.tone == tone)
        };
        assert!(find("tiếng", "t", "iêng", TONE_SAC));
        assert!(find("sẻ", "s", "e", TONE_HOI));
        assert!(find("nghĩa", "ngh", "ia", TONE_NGA));
        assert!(find("được", "đ", "ươc", TONE_NANG));
        assert!(find("nguyên", "ng", "uyên", TONE_NGANG));
        assert!(find("an", "", "an", TONE_NGANG));
        assert!(find("yêu", "", "yêu", TONE_NGANG));
        // nhập chữ — cả hai cách đọc đều được trả về
        assert!(find("gì", "gi", "i", TONE_HUYEN));
        assert!(find("quỳnh", "qu", "uynh", TONE_HUYEN));
    }

    #[test]
    fn decompose_returns_nothing_for_non_syllables() {
        // Không tách được thành âm đầu + vần nào cả.
        for s in ["xyz", "qwerty", "zzz", ""] {
            assert!(decompose(s).is_empty(), "{s:?} không nên phân tích được");
        }
    }

    #[test]
    fn decompose_analyses_misspellings_so_l3_can_repair_them() {
        // Đây là công dụng chính của decompose, không phải tác dụng phụ.
        let find = |s: &str, onset: &str, rime: &str, tone: usize| {
            decompose(s)
                .iter()
                .any(|a| a.onset == onset && a.rime == rime && a.tone == tone)
        };

        // `nghành` sai vì ngh không đứng trước nguyên âm sau. Phân tích ra được thì
        // áp luật ngh→ng là sửa xong.
        assert!(find("nghành", "ngh", "anh", TONE_HUYEN));
        assert_eq!(compose("ng", "anh", TONE_HUYEN).as_deref(), Some("ngành"));

        // `ngiên` sai vì ng không đứng trước nguyên âm trước → luật ng→ngh.
        assert!(find("ngiên", "ng", "iên", TONE_NGANG));
        assert_eq!(compose("ngh", "iên", TONE_NGANG).as_deref(), Some("nghiên"));

        // `mat` sai vì vần đóng bằng t chỉ nhận sắc/nặng → đổi thanh là sửa xong.
        assert!(find("mat", "m", "at", TONE_NGANG));
        assert_eq!(
            compose("m", "at", TONE_NGANG),
            None,
            "compose phải nghiêm ngặt"
        );
        assert_eq!(compose("m", "at", TONE_SAC).as_deref(), Some("mát"));
        assert_eq!(compose("m", "at", TONE_NANG).as_deref(), Some("mạt"));
    }

    #[test]
    fn compose_is_the_inverse_of_decompose() {
        // Bất biến: phân tích rồi dựng lại phải ra đúng âm tiết ban đầu.
        // Đây là điều kiện để L3 thay một thành phần mà không làm hỏng phần còn lại.
        for s in [
            "tiếng", "việt", "sẻ", "sẽ", "nghĩa", "được", "người", "nguyên", "quỳnh", "gì",
            "giêng", "muối", "hoàn", "hoàng", "xuất", "khuỷu", "an", "yêu",
        ] {
            let analyses = decompose(s);
            assert!(!analyses.is_empty(), "không phân tích được {s}");
            assert!(
                analyses
                    .iter()
                    .any(|a| compose(a.onset, a.rime, a.tone).as_deref() == Some(s)),
                "dựng lại {s} không khớp: {analyses:?} → {:?}",
                analyses
                    .iter()
                    .map(|a| compose(a.onset, a.rime, a.tone))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn compose_never_produces_an_invalid_syllable() {
        // Quét toàn bộ không gian tổ hợp: mọi thứ compose trả về Some đều PHẢI
        // nằm trong tập hợp lệ. Nếu vỡ, L3 sẽ đề xuất âm tiết không tồn tại.
        let mut checked = 0usize;
        for onset in std::iter::once("").chain(onsets().iter().map(|o| o.text.as_str())) {
            for rime in rimes() {
                for tone in 0..TONE_COUNT {
                    if let Some(s) = compose(onset, &rime.text, tone) {
                        assert!(
                            is_valid_syllable(&s),
                            "compose({onset:?}, {:?}, {tone}) = {s:?} không hợp lệ",
                            rime.text
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 10_000, "quét quá ít tổ hợp: {checked}");
    }

    #[test]
    fn set_size_is_in_expected_range() {
        let n = syllable_set().len();
        // Tham chiếu ~17.974 (Lương Hiếu Thi). Ta sinh thêm biến thể vị trí dấu
        // oa/oe/uy nên hơi cao hơn. Chặn hai đầu để bắt hồi quy khi sửa bảng.
        assert!(
            (15_000..=22_000).contains(&n),
            "tập âm tiết có {n} phần tử — lệch xa mức kỳ vọng, kiểm tra lại bảng"
        );
    }
}
