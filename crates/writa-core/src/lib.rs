//! Writa engine — kiểm tra chính tả tiếng Việt.
//!
//! Crate này **không phụ thuộc OS** để build được sang WASM (dùng lại cho
//! VSCode extension và web demo). Mọi thứ liên quan Windows nằm ở `writa-win`.
//!
//! # Các lớp
//!
//! | Lớp | Module | Trạng thái |
//! |---|---|---|
//! | L0 chuẩn hoá | [`normalize`] | ✅ |
//! | L0 tách token + vùng bảo vệ | [`token`] | ✅ |
//! | L1 tính hợp lệ âm tiết | [`phonology`] | ✅ |
//! | L2 từ vựng từ corpus | [`dict`] | ✅ |
//! | L3 sinh candidate | [`candidate`] | ✅ |
//! | L4 mô hình ngôn ngữ + Viterbi | [`lm`] | ✅ |
//! | L5 dấu câu / khoảng trắng / viết hoa | [`rules`] | ✅ |
//! | L6 AI (opt-in, không có mạng ở đây) | [`ai`] | ✅ |
//!
//! Toàn bộ engine chạy **offline**. Lớp AI ở [`ai`] chỉ định nghĩa hợp đồng —
//! phần gọi mạng nằm ở crate riêng `writa-ai`, và chỉ chạy khi user chủ động bấm.

use std::ops::Range;

pub mod ai;
pub mod candidate;
pub mod diacritic;
pub mod dict;
pub mod lm;
pub mod normalize;
pub mod phonology;
pub mod rules;
pub mod token;

/// Loại lỗi. Mỗi loại đến từ một lớp khác nhau và có độ tin cậy khác nhau —
/// UI dùng thông tin này để quyết định gạch đỏ, gạch xanh, hay tự sửa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// L1 — âm tiết không tồn tại trong tiếng Việt. `nghành`, `ngiên`, `quyêt`.
    ///
    /// Đây là loại lỗi duy nhất **chắc chắn sai không cần ngữ cảnh**.
    InvalidSyllable,
    /// L2 — hợp lệ về ngữ âm nhưng chưa từng thấy trong corpus. `khoẻn`, `nhoẹt`.
    ///
    /// Chỉ là **tín hiệu nghi vấn**, không phải kết luận: corpus Wikipedia thiên
    /// văn phong chính luận, nên từ khẩu ngữ hoặc phương ngữ có thể vắng mặt.
    /// Vì vậy loại này mặc định TẮT — xem [`CheckOptions::flag_unattested`].
    UnattestedSyllable,
    /// L2 + L3 — **lỗi real-word**: mọi âm tiết đều hợp lệ nhưng tổ hợp thì sai.
    ///
    /// `chia sẽ`, `sữa lỗi`, `xử dụng`. Đây là loại lỗi người Việt mắc nhiều nhất
    /// và L1 hoàn toàn mù với nó, vì từng âm tiết đều là từ thật.
    ///
    /// Phát hiện bằng cách so tần suất từ ghép: `chia sẻ` xuất hiện dày trong corpus
    /// còn `chia sẽ` bằng 0. Luôn là [`Confidence::Likely`] — không bao giờ tự sửa,
    /// vì người viết có thể chủ ý.
    ConfusedSyllable,
    /// L5 — dấu câu và khoảng trắng: khoảng trắng đôi, khoảng trắng trước dấu phẩy,
    /// ngoặc không cân.
    Punctuation,
    /// L5 — chữ đầu câu không viết hoa. Mặc định TẮT, xem [`rules::RuleOptions`].
    Capitalization,
}

/// Độ tin cậy — quyết định hành vi UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Sai chắc chắn. Đủ điều kiện tự sửa **nếu** có đúng một candidate và
    /// candidate đó có tần suất cao (điều kiện thứ hai do L2 cấp, chưa có).
    Certain,
    /// Cần ngữ cảnh phán quyết — chỉ gợi ý, không bao giờ tự sửa.
    Likely,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Span theo byte trong text **gốc** truyền vào [`check`].
    pub span: Range<usize>,
    pub kind: DiagnosticKind,
    /// Dạng đã chuẩn hoá của đoạn bị báo lỗi.
    pub found: String,
    /// Đề xuất thay thế, tốt nhất trước. Rỗng cho tới khi có L3.
    pub candidates: Vec<String>,
    pub confidence: Confidence,
}

/// Tuỳ chọn kiểm tra.
#[derive(Debug, Clone, Copy)]
pub struct CheckOptions {
    /// Có báo âm tiết hợp lệ-ngữ-âm nhưng chưa chứng thực trong corpus không?
    ///
    /// Mặc định `false`, và lý do là **rủi ro chưa đo được** chứ không phải rủi ro
    /// đã đo thấy lớn.
    ///
    /// `writa-cli scan --unattested` cho thấy bật lên chỉ thêm ~75 báo lỗi trên
    /// 3,1 triệu từ. Nhưng con số đó **vô nghĩa vì vòng tròn**: tập câu dùng để đo
    /// là tập con của chính corpus dùng để dựng `syllables.tsv`, nên mọi âm tiết
    /// trong đó đương nhiên đã chứng thực.
    ///
    /// Rủi ro thật — báo oan từ khẩu ngữ, phương ngữ, từ mới mà Wikipedia không có
    /// — chỉ đo được trên văn bản từ **nguồn khác**: tin nhắn chat, bình luận diễn
    /// đàn, mạng xã hội. Chưa có nguồn đó nên chưa có số.
    ///
    /// Khi chưa biết, hướng lệch đúng là im lặng. Xem thêm ghi chú "hướng lệch có
    /// chủ đích" trong [`crate::phonology`].
    ///
    /// Khi L3/L4 lên, tín hiệu này hữu ích theo cách khác hẳn: không dùng để báo
    /// lỗi một mình, mà để **cộng điểm** cho candidate trong lúc giải mã Viterbi.
    pub flag_unattested: bool,

    /// Có phát hiện lỗi *real-word* bằng tần suất từ ghép không?
    ///
    /// Mặc định `true`. Đây là loại lỗi người Việt mắc nhiều nhất (`chia sẽ`,
    /// `xử dụng`) và L1 hoàn toàn mù với nó. Tắt để đo phần đóng góp riêng của nó
    /// vào false-positive: `writa-cli scan --no-realword`.
    pub detect_real_word: bool,

    /// Ngưỡng chênh lệch log của phát hiện real-word. Cao hơn = im lặng hơn.
    ///
    /// Đây là **tham số duy nhất** điều khiển độ mạnh tay của lớp real-word, thay
    /// cho ba ngưỡng thủ công mà bản dùng tần suất thô cần. Đưa ra ngoài để quét
    /// bằng số đo (`writa-cli scan --margin N`) thay vì chỉnh bằng cảm giác, và để
    /// UI về sau có một núm "độ nhạy" duy nhất.
    pub real_word_margin: f64,

    /// Bằng chứng **thêm** mà một candidate hai phép sửa phải có so với một phép sửa.
    ///
    /// L3 sinh được candidate đổi hai thành phần âm tiết cùng lúc — cần thiết cho
    /// nhóm lỗi cộng dồn như `sẳng sàng` → `sẵn sàng` (thanh *và* âm cuối). Nhưng
    /// hai phép sửa a priori kém khả dĩ hơn một phép sửa, và không tính chi phí đó
    /// thì lớp real-word sẽ thoải mái viết lại từ đúng thành từ khác hợp ngữ cảnh
    /// hơn.
    ///
    /// Ngưỡng thật cho candidate `n` phép sửa là
    /// `real_word_margin + extra_edit_margin × (n − 1)`.
    ///
    /// Xem [`DEFAULT_EXTRA_EDIT_MARGIN`] về cách chọn con số.
    pub extra_edit_margin: f64,

    /// Luật dấu câu / khoảng trắng / viết hoa. Xem [`rules::RuleOptions`].
    pub rules: rules::RuleOptions,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            flag_unattested: false,
            detect_real_word: true,
            real_word_margin: DEFAULT_REAL_WORD_MARGIN,
            extra_edit_margin: DEFAULT_EXTRA_EDIT_MARGIN,
            rules: rules::RuleOptions::default(),
        }
    }
}

/// Số candidate tối đa kèm theo mỗi lỗi. UI chỉ hiện được vài dòng, và candidate
/// thứ mười chưa bao giờ là đáp án đúng.
const MAX_CANDIDATES: usize = 5;

/// Ngưỡng mặc định cho [`CheckOptions::real_word_margin`].
///
/// Chọn bằng cách quét đường cong đánh đổi trên 35 nghìn lỗi đã tiêm và 50 nghìn
/// câu held-out, không phải bằng cảm giác:
///
/// | margin | Recall | Precision | F0.5 | FP/1000 |
/// |---|---|---|---|---|
/// | 3    | 96,6% | 95,1%  | 0,954 | 2,52 |
/// | 4,5  | 94,1% | 98,2%  | 0,974 | 1,20 |
/// | **6**| **90,7%** | **99,9%** | **0,979** | **0,53** |
/// | 9    | 78,1% | 100,0% | 0,947 | 0,13 |
/// | 12   | 60,0% | 100,0% | 0,882 | 0,05 |
///
/// `6` tối đa hoá F0.5 — độ đo ưu tiên precision gấp đôi recall, đúng hướng lệch
/// của dự án. Ngưỡng cao hơn mua precision bằng cái giá recall đắt hơn nhiều.
///
/// Con số này thay cho ba ngưỡng thủ công mà bản dùng tần suất thô phải dựng (bằng
/// chứng tối thiểu, tỉ lệ, độ chặt hai chiều). Mô hình ngôn ngữ có backoff làm được
/// cùng một việc bằng một tham số, vì nó phân biệt được "hiếm" với "sai" — thứ mà
/// tần suất thô không làm được.
pub const DEFAULT_REAL_WORD_MARGIN: f64 = 6.0;

/// Ngưỡng mặc định cho [`CheckOptions::extra_edit_margin`].
///
/// Quét với `real_word_margin = 6` cố định, trên 35 nghìn lỗi đã tiêm và 50 nghìn
/// câu held-out. Cột precision đã **bỏ nhóm B** (từ vay mượn ASCII) vì nhóm đó là
/// vấn đề của corpus chứ không phải của lớp phán quyết — xem `writa-cli eval`.
///
/// | extra | Recall | Precision | F0.5 | Báo oan tiếng Việt |
/// |---|---|---|---|---|
/// | *(chặn hết 2 phép sửa)* | 90,7% | 98,4% | 0,967 | 522 |
/// | 0 | 90,7% | 98,1% | 0,966 | 604 |
/// | 2 | 90,7% | 98,3% | 0,967 | 544 |
/// | 3 | 90,7% | 98,3% | 0,967 | 536 |
/// | **5** | **90,7%** | **98,4%** | **0,967** | **525** |
///
/// `5` là con số nhỏ nhất trả lại **đúng** precision của bản chỉ-một-phép-sửa. Nói
/// cách khác: nhóm candidate hai phép sửa được thêm vào mà không phải trả giá nào
/// đo được — vì nó chỉ thắng khi mô hình ngôn ngữ ủng hộ rất mạnh (`sẳng` → `sẵn`
/// chênh 23,45, còn ngưỡng chỉ là 11).
///
/// Recall đứng yên ở 90,7% trên bộ test này vì lỗi được tiêm mỗi lần **một** phép
/// sửa. Bộ test hiện tại **không đo được** thứ mà thay đổi này sinh ra để giải quyết
/// — lỗi cộng dồn kiểu `sẳng sàng`. Cần bổ sung nhóm lỗi hai-phép-sửa vào
/// `make-eval` thì con số recall mới nói lên điều gì đó về nó.
pub const DEFAULT_EXTRA_EDIT_MARGIN: f64 = 5.0;

/// Tổ hợp gốc đã đủ phổ biến thì bỏ qua luôn, không sinh candidate.
///
/// Thuần tuý để chạy nhanh: tuyệt đại đa số cặp từ trong văn bản thật là đúng, và
/// kiểm tra một lần tra bảng rẻ hơn nhiều so với sinh rồi chấm điểm hàng chục
/// phương án.
const SKIP_IF_COMPOUND_SEEN: u64 = 30;

/// Kiểm tra một đoạn text với tuỳ chọn mặc định.
///
/// Hiện chạy tới **L2**. Chưa bắt được lỗi *real-word* (`chia sẽ`, `sữa lỗi`,
/// `xử dụng`) vì loại đó cần L3 sinh candidate + L4 phán quyết theo ngữ cảnh.
///
/// Span trả về trỏ vào text gốc nên dùng thay thế trực tiếp được — kể cả khi
/// đầu vào ở dạng NFD. Xem [`token`] về lý do.
pub fn check(text: &str) -> Vec<Diagnostic> {
    check_with(text, CheckOptions::default())
}

/// Như [`check`] nhưng cho phép điều chỉnh hành vi.
pub fn check_with(text: &str, opts: CheckOptions) -> Vec<Diagnostic> {
    let all = token::tokenize(text);
    let words: Vec<token::Token> = all
        .iter()
        .filter(|t| t.kind == token::TokenKind::Word)
        .cloned()
        .collect();

    let mut out = rules::check(text, &all, opts.rules);
    let mut flagged_spans: Vec<std::ops::Range<usize>> = Vec::new();

    for tok in words.iter().filter(|t| t.protect.is_none()) {
        if let Some(d) = classify(tok, &opts) {
            flagged_spans.push(d.span.clone());
            out.push(d);
        }
    }

    if opts.detect_real_word {
        // Chỉ xét cặp mà cả hai âm tiết đều đã sạch ở lớp trên. Báo lỗi real-word
        // cho một từ vừa bị báo là âm tiết không tồn tại thì chỉ gây nhiễu.
        for d in real_word_errors(&words, &flagged_spans, &opts) {
            out.push(d);
        }
    }

    out.sort_by_key(|d| d.span.start);
    out
}

/// Phán quyết một token đã qua L0. Trả `None` nếu không có vấn đề gì.
fn classify(tok: &token::Token, opts: &CheckOptions) -> Option<Diagnostic> {
    let syl = &tok.normalized;

    if !phonology::is_valid_syllable(syl) {
        // L2 chặn ở đây: `electron`, `paris`, `km` không phải âm tiết tiếng Việt
        // nhưng là từ bình thường trong văn bản Việt. Nếu thiếu bước này, L1 báo
        // oan 20,52 lần / 1000 từ.
        if dict::is_accepted_foreign(syl) {
            return None;
        }

        let candidates = top_candidates(syl);

        // Token ASCII thuần mà **không nguồn nào** nghĩ ra được một âm tiết tiếng Việt
        // cho nó: im lặng.
        //
        // Đây là chỗ phân biệt `deadline` với `nghiep`, và nó phân biệt được vì hai
        // nhóm đó khác nhau ở đúng chỗ này:
        //
        // | Token | chỉ mục bỏ dấu | bảng nhầm lẫn | phán quyết |
        // |---|---|---|---|
        // | `nghiep` | → `nghiệp` | — | từ Việt thiếu dấu → báo |
        // | `nganh` | → `ngành` | — | báo |
        // | `nghanh` | — | → `nganh` | lỗi gõ → báo |
        // | `chinhs` | — | → `chính` (Telex) | báo |
        // | `deadline` | — | — | **từ ngoại → im lặng** |
        // | `meeting` `push` `check` `code` | — | — | **im lặng** |
        //
        // `dict::is_accepted_foreign` đã bắt nhóm từ ngoại **có mặt trong corpus**, nhưng
        // corpus là Wikipedia còn Writa thì chạy trong ô chat — nơi `deadline`, `meeting`,
        // `push`, `check` là từ hằng ngày mà Wikipedia gần như không có. Đó là lý do bước
        // này cần thiết chứ không dư.
        //
        // Đảo lại một quyết định cũ, và lý do đảo: trước đây ta cố tình báo cả những chỗ
        // không có gợi ý, với lập luận "giấu đi thì user tưởng câu mình đúng" — lấy
        // `nghiep` làm ví dụ. Nhưng `nghiep` **có** gợi ý (`nghiệp`), chỉ là chưa ai nối
        // chỉ mục bỏ dấu vào đây. Tiền đề của quyết định cũ sai, nên quyết định cũ sai.
        if candidates.is_empty() && syl.is_ascii() {
            return None;
        }

        return Some(Diagnostic {
            span: tok.span.clone(),
            kind: DiagnosticKind::InvalidSyllable,
            found: syl.clone(),
            candidates,
            confidence: Confidence::Certain,
        });
    }

    if opts.flag_unattested && !dict::is_attested(syl) {
        return Some(Diagnostic {
            span: tok.span.clone(),
            kind: DiagnosticKind::UnattestedSyllable,
            found: syl.clone(),
            candidates: top_candidates(syl),
            confidence: Confidence::Likely,
        });
    }

    None
}

fn top_candidates(syllable: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // Với token ASCII thuần, nguồn candidate **đúng** là chỉ mục bỏ dấu, không phải bảng
    // nhầm lẫn: người ta gõ `nghiep` là vì bộ gõ tắt, không phải vì nhầm âm đầu. Bảng
    // nhầm lẫn không với tới được `nghiệp` từ `nghiep` vì đó không phải một phép thay
    // thành phần — đó là thêm dấu.
    if syllable.chars().all(|c| c.is_ascii_alphabetic()) {
        out.extend(
            diacritic::options_for(syllable)
                .into_iter()
                .map(String::from),
        );
    }
    for c in candidate::for_syllable(syllable) {
        if !out.contains(&c.text) {
            out.push(c.text);
        }
    }
    out.truncate(MAX_CANDIDATES);
    out
}

/// Phát hiện lỗi *real-word* bằng mô hình ngôn ngữ.
///
/// `chia sẽ`, `xử dụng`, `sữa lỗi` — mọi âm tiết đều là từ thật nên L1 mù hoàn
/// toàn. Cách phát hiện: với từng vị trí, so điểm mô hình ngôn ngữ của bản gốc với
/// từng phương án thay thế, **trong cùng ngữ cảnh**.
///
/// Điểm khác then chốt so với bản L3 (dùng tần suất từ ghép thô): mô hình có
/// backoff nên phân biệt được **"hiếm"** với **"sai"**. Tần suất thô coi đếm bằng 0
/// là bằng chứng, và đó là nguồn của những báo oan `cát → các`, `dùng → vùng`,
/// `hộ → họ` — cặp hai từ đều đúng, chỉ là tổ hợp không nằm trong 154 nghìn bigram
/// ta giữ lại.
fn real_word_errors(
    words: &[token::Token],
    already_flagged: &[std::ops::Range<usize>],
    opts: &CheckOptions,
) -> Vec<Diagnostic> {
    let seq: Vec<&str> = words.iter().map(|t| t.normalized.as_str()).collect();
    let mut out: Vec<Diagnostic> = Vec::new();

    for (i, tok) in words.iter().enumerate() {
        if tok.protect.is_some() || already_flagged.contains(&tok.span) {
            continue;
        }
        // Chỉ xét âm tiết tiếng Việt thật. Bỏ bước này thì lớp real-word đi "sửa"
        // từ tiếng Anh thành âm tiết Việt: `bit → bít` (292 lần trên 50 nghìn câu),
        // `net → nét`, `hit → hít`, `bus → bú`. Chúng lọt vào vì L2 chấp nhận chúng
        // như từ vay mượn, nên `classify` đúng khi im lặng — nhưng im lặng ở đó
        // không có nghĩa là mời lớp này vào sửa.
        if !phonology::is_valid_syllable(&tok.normalized) {
            continue;
        }
        // Lối tắt cho tốc độ: bỏ qua khi từ này đã đi liền với **cả hai** từ bên
        // cạnh trong corpus. Tuyệt đại đa số vị trí rơi vào nhánh này.
        //
        // Phải là CẢ HAI, không phải một trong hai. Bản đầu dùng `hoặc` và bỏ sót
        // `chia sẽ điều này`: `sẽ điều` là tổ hợp có thật (`sẽ điều hành`,
        // `sẽ điều chỉnh`) nên vị trí bị bỏ qua, dù bên trái đã rõ là sai.
        let left_ok =
            i == 0 || dict::compound_frequency(seq[i - 1], seq[i]) >= SKIP_IF_COMPOUND_SEEN;
        let right_ok = i + 1 >= seq.len()
            || dict::compound_frequency(seq[i], seq[i + 1]) >= SKIP_IF_COMPOUND_SEEN;
        if left_ok && right_ok {
            continue;
        }

        let base = lm::local_log_score(&seq, i);
        let mut best: Option<(String, f64)> = None;

        for cand in candidate::for_syllable(&tok.normalized) {
            // Biến thể đều đúng (`kỹ`/`kĩ`) không phải lỗi, nên không được làm căn
            // cứ báo lỗi. Thiếu chốt này, engine báo `kì → kỳ` 49 lần trên 50 nghìn
            // câu — toàn bộ là báo oan.
            if !cand.reason.can_flag_real_word() {
                continue;
            }
            // Candidate càng xa bản gốc thì càng phải mang nhiều bằng chứng. Không
            // có bậc thang này, phương án hai phép sửa cạnh tranh ngang hàng với
            // phương án một phép sửa, và mô hình ngôn ngữ sẽ chuộng cái nào hợp ngữ
            // cảnh hơn — kể cả khi bản gốc vốn đã đúng.
            let required = opts.real_word_margin
                + opts.extra_edit_margin * f64::from(cand.edits.saturating_sub(1));
            let mut alt = seq.clone();
            alt[i] = cand.text.as_str();
            let gain = lm::local_log_score(&alt, i) - base;
            // So sánh phần bằng chứng **dôi ra so với ngưỡng của chính nó**, không
            // so điểm thô: điểm thô sẽ để candidate hai phép sửa thắng candidate
            // một phép sửa chỉ vì nó đi xa hơn nên dễ rơi vào tổ hợp tần suất cao.
            let surplus = gain - required;
            if surplus >= 0.0 && best.as_ref().is_none_or(|(_, s)| surplus > *s) {
                best = Some((cand.text.clone(), surplus));
            }
        }

        if let Some((replacement, _)) = best {
            out.push(Diagnostic {
                span: tok.span.clone(),
                kind: DiagnosticKind::ConfusedSyllable,
                found: tok.normalized.clone(),
                candidates: vec![replacement],
                confidence: Confidence::Likely,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flagged(text: &str) -> Vec<String> {
        check(text).into_iter().map(|d| d.found).collect()
    }

    #[test]
    fn catches_invalid_syllables() {
        assert_eq!(flagged("Tôi làm trong nghành này"), vec!["nghành"]);
        assert_eq!(flagged("đang ngiên cứu"), vec!["ngiên"]);
        assert_eq!(flagged("đã quyêt định"), vec!["quyêt"]);
    }

    #[test]
    fn stays_silent_on_correct_text() {
        for s in [
            "Tôi yêu tiếng Việt.",
            "Xin chào, rất vui được gặp bạn.",
            "Nghiên cứu về ngành công nghiệp này đã hoàn thành.",
            "Chia sẻ dữ liệu và giữ gìn kỹ thuật.",
            "Quyển truyện đó kể chuyện gì?",
            "Hôm nay trời hoà nhã, tôi khoẻ, chị Thuý cũng khỏe.",
        ] {
            assert!(
                check(s).is_empty(),
                "báo lỗi oan trong: {s:?} → {:?}",
                flagged(s)
            );
        }
    }

    #[test]
    fn does_not_flag_protected_spans() {
        // Toàn bộ nhóm mà vòng verify corpus cho thấy hay bị loại nhất.
        for s in [
            "Xem https://vi.wikipedia.org/wiki/Tiếng_Việt để biết thêm.",
            "Liên hệ khanh.nguyen@rivercrane.com.vn nhé.",
            "HĐND ban hành QĐ số 12 về COVID19 khổ A4.",
            "Tôi đến México rồi qua Đắk Lắk và Krông Nô.",
            "Chạy `cargo buildd` trong D:\\NGUYENKHANH\\proj rồi xem ../src/mainn.rs.",
            "Gửi @khanh xem #tiengviet nha.",
        ] {
            assert!(
                check(s).is_empty(),
                "báo lỗi oan trong: {s:?} → {:?}",
                flagged(s)
            );
        }
    }

    #[test]
    fn spans_are_usable_for_replacement() {
        let src = "Tôi làm trong nghành này";
        let d = &check(src)[0];
        assert_eq!(&src[d.span.clone()], "nghành");
        // Mô phỏng việc thay thế mà writa-win sẽ làm
        let mut fixed = src.to_string();
        fixed.replace_range(d.span.clone(), "ngành");
        assert_eq!(fixed, "Tôi làm trong ngành này");
    }

    #[test]
    fn spans_are_usable_for_replacement_on_nfd_input() {
        // Bất biến quan trọng nhất: đầu vào NFD vẫn thay đúng chỗ.
        let src = "Tôi làm trong nghài\u{0300}nh này"; // "nghàình" dạng tổ hợp
        let ds = check(src);
        assert_eq!(ds.len(), 1);
        assert_eq!(&src[ds[0].span.clone()], "nghài\u{0300}nh");
    }

    #[test]
    fn invalid_syllables_are_certain() {
        assert!(check("nghành")
            .iter()
            .all(|d| d.confidence == Confidence::Certain));
    }

    #[test]
    fn l2_stops_flagging_loanwords_and_foreign_names() {
        // Đây là việc L2 sinh ra để làm. Trước L2, mỗi từ dưới đây là một lần
        // báo oan; cả nhóm này chiếm 20,52 báo oan / 1000 từ khi đo thực tế.
        for s in [
            "Một electron mang điện tích âm.",
            "Virus này chứa protein vỏ ngoài.",
            "Paris là thủ đô nước Pháp.",
            "Canada rộng gần 10 triệu km vuông.",
            "Xét nghiệm dna cho kết quả dương tính.",
        ] {
            assert!(
                check(s).is_empty(),
                "báo oan trong: {s:?} → {:?}",
                flagged(s)
            );
        }
    }

    #[test]
    fn l2_still_catches_real_typos() {
        // Điều kiện sống còn: L2 nới lỏng cho từ ngoại nhưng KHÔNG được nới cho
        // lỗi gõ tay. Nếu test này vỡ thì ngưỡng lan toả đã quá lỏng.
        assert_eq!(flagged("Tôi làm trong nghành này"), vec!["nghành"]);
        // `cuu` cũng sai (đúng là `cứu`) — còn `dang`, `ve`, `tri` là âm tiết hợp
        // lệ nên đứng một mình thì không bắt được; loại đó phải chờ L3 + L4.
        assert_eq!(
            flagged("dang ngiên cuu ve chinhs tri"),
            vec!["ngiên", "cuu", "chinhs"]
        );
    }

    fn suggestions(text: &str) -> Vec<(String, Vec<String>)> {
        check(text)
            .into_iter()
            .map(|d| (d.found, d.candidates))
            .collect()
    }

    #[test]
    fn l3_fills_candidates_so_diagnostics_are_actionable() {
        // Trước L3, mọi Diagnostic đều có candidates rỗng — biết sai mà không biết
        // sửa thành gì thì UI không làm được gì cả.
        let ds = check("Tôi làm trong nghành này");
        assert_eq!(ds.len(), 1);
        assert!(
            ds[0].candidates.contains(&"ngành".to_string()),
            "phải đề xuất `ngành`: {:?}",
            ds[0].candidates
        );
        assert!(ds[0].candidates.len() <= MAX_CANDIDATES);

        let ds = check("đang ngiên cứu");
        assert!(ds[0].candidates.contains(&"nghiên".to_string()));
    }

    #[test]
    fn l3_catches_real_word_errors_l1_is_blind_to() {
        // Mọi âm tiết dưới đây đều là từ thật, nên L1 không thấy gì. Đây là loại
        // lỗi người Việt mắc nhiều nhất.
        let got = suggestions("Tôi muốn chia sẽ điều này");
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, "sẽ");
        assert_eq!(got[0].1, vec!["sẻ".to_string()]);

        let got = suggestions("Cách xử dụng phần mềm");
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, "xử");
        assert_eq!(got[0].1, vec!["sử".to_string()]);
    }

    #[test]
    fn real_word_detection_stays_silent_on_correct_text() {
        // Lớp này chỉ gợi ý nên precision là tất cả. Nếu nó báo oan vào văn bản
        // đúng thì user tắt engine.
        for s in [
            "Tôi muốn chia sẻ điều này với bạn.",
            "Cách sử dụng phần mềm rất đơn giản.",
            "Chúng tôi cần củng cố nền tảng.",
            "Quyển truyện đó kể chuyện gì?",
            "Anh ấy giành chiến thắng và để dành tiền.",
            "Suy nghĩ kỹ rồi hãy nghỉ ngơi.",
            "Hoàn thành công việc trước hoàng hôn.",
        ] {
            assert!(
                check(s).is_empty(),
                "báo oan trong {s:?} → {:?}",
                suggestions(s)
            );
        }
    }

    #[test]
    fn real_word_detection_can_be_turned_off() {
        let s = "Tôi muốn chia sẽ điều này";
        assert_eq!(check(s).len(), 1);
        let opts = CheckOptions {
            detect_real_word: false,
            ..Default::default()
        };
        assert!(check_with(s, opts).is_empty());
    }

    #[test]
    fn real_word_detection_is_silent_when_evidence_is_thin() {
        // Chốt an toàn quan trọng nhất: khi cả tổ hợp gốc lẫn mọi tổ hợp thay thế
        // đều hiếm, im lặng thay vì đoán. Nhờ vậy thuật ngữ chuyên ngành và từ ghép
        // ít gặp không bị báo oan.
        for s in ["Thuật ngữ vi mạch quang tử", "Kỹ thuật xạ trị proton"] {
            let ds: Vec<_> = check(s)
                .into_iter()
                .filter(|d| d.kind == DiagnosticKind::ConfusedSyllable)
                .collect();
            assert!(ds.is_empty(), "đoán bừa trong {s:?} → {ds:?}");
        }
    }

    #[test]
    fn never_flags_equally_valid_i_y_variants() {
        // `kỹ`=`kĩ`, `lý`=`lí`, `quý`=`quí` đều được chấp nhận. Báo chúng là lỗi
        // của engine, không phải của người viết.
        for s in [
            "Kĩ thuật này rất mới.",
            "Suy nghĩ kĩ trước khi làm.",
            "Lí thuyết và thực hành.",
            "Quí vị vui lòng chờ.",
            "Bác sĩ và bác sỹ đều đúng.",
            "Thời kì đổi mới.",
        ] {
            let ds: Vec<_> = check(s)
                .into_iter()
                .filter(|d| d.kind == DiagnosticKind::ConfusedSyllable)
                .collect();
            assert!(ds.is_empty(), "báo oan biến thể i/y trong {s:?} → {ds:?}");
        }
    }

    #[test]
    fn real_word_errors_are_never_certain() {
        // Không bao giờ tự sửa loại này: người viết có thể chủ ý.
        for d in check("Tôi muốn chia sẽ điều này") {
            if d.kind == DiagnosticKind::ConfusedSyllable {
                assert_eq!(d.confidence, Confidence::Likely);
            }
        }
    }

    #[test]
    fn unattested_flagging_is_off_by_default() {
        // `khoẻn` hợp lệ ngữ âm nhưng không có trong corpus.
        let s = "Trời hôm nay khoẻn quá";
        assert!(
            check(s).is_empty(),
            "mặc định không được báo âm tiết chưa chứng thực"
        );

        let opts = CheckOptions {
            flag_unattested: true,
            ..Default::default()
        };
        let ds = check_with(s, opts);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].kind, DiagnosticKind::UnattestedSyllable);
        assert_eq!(ds[0].confidence, Confidence::Likely);
    }
}
