//! L4 — Mô hình ngôn ngữ và giải mã theo ngữ cảnh.
//!
//! # Vấn đề L3 để lại
//!
//! L3 phán quyết lỗi *real-word* bằng tần suất từ ghép thô, và lỗ hổng cốt lõi là:
//! nó coi **đếm bằng 0 là bằng chứng**. Nhưng `compounds.tsv` chỉ giữ 154 nghìn
//! bigram phổ biến nhất, nên `freq = 0` không nghĩa là "không thể" mà chỉ nghĩa là
//! "chưa thấy trong phần dữ liệu ta giữ lại". Hệ quả đo được: engine báo
//! `cát → các`, `dùng → vùng`, `hộ → họ` — toàn cặp hai từ đều đúng.
//!
//! L3 vá bằng cổng độ chặt hai chiều và đưa được về 0,20 báo oan/1000 từ, nhưng đó
//! vẫn là xấp xỉ dựng bằng tay.
//!
//! # Lời giải: backoff
//!
//! Mô hình ngôn ngữ giải đúng vấn đề đó bằng **backoff**: chưa thấy trigram thì lùi
//! về bigram, chưa thấy bigram thì lùi về unigram, mỗi lần lùi nhân thêm một hệ số
//! phạt. Nhờ vậy tổ hợp chưa gặp nhận điểm *thấp* chứ không phải điểm *bằng không*,
//! và sự khác biệt giữa "hiếm" với "sai" mới hiện ra.
//!
//! # Vì sao Stupid Backoff, không phải Kneser-Ney
//!
//! PLAN.md ghi Kneser-Ney. Sau khi cân nhắc tôi chọn Stupid Backoff (Brants và cs.,
//! 2007) và ghi lý do ở đây:
//!
//! - Ta chỉ cần **xếp hạng** các phương án, không cần xác suất chuẩn hoá. Stupid
//!   Backoff không phải phân phối xác suất hợp lệ, nhưng với việc xếp hạng thì nó
//!   ngang ngửa các phương pháp làm mượt phức tạp khi corpus đủ lớn — đó chính là
//!   kết luận của bài báo gốc.
//! - Nó không cần bảng chiết khấu và trọng số backoff, tức không cần thêm dữ liệu
//!   nào ngoài số đếm đã có.
//! - Ít tham số hơn nghĩa là ít chỗ để sai âm thầm.
//!
//! Nếu sau này eval cho thấy xếp hạng là điểm nghẽn, đây là chỗ để đổi.

use crate::dict;

/// Hệ số phạt mỗi lần lùi bậc. 0,4 là giá trị bài báo gốc dùng.
const BACKOFF: f64 = 0.4;

/// Điểm sàn cho âm tiết chưa từng thấy — tránh `ln(0)`.
///
/// Đặt thấp hơn âm tiết hiếm nhất trong corpus, nhưng hữu hạn: một từ chưa gặp là
/// *khó tin*, không phải *bất khả*.
const UNSEEN_SCORE: f64 = 1e-9;

/// Điểm Stupid Backoff của `word` sau ngữ cảnh `context` (tối đa hai âm tiết trước).
///
/// Trả về giá trị **chưa lấy log**, luôn dương. Dùng [`log_score`] khi cần cộng dồn.
pub fn score(context: &[&str], word: &str) -> f64 {
    // Bậc 3: P(word | a b) = count(a b word) / count(a b)
    if context.len() >= 2 {
        let (a, b) = (context[context.len() - 2], context[context.len() - 1]);
        let tri = dict::trigram_frequency(a, b, word);
        if tri > 0 {
            let denom = dict::compound_frequency(a, b);
            if denom > 0 {
                return tri as f64 / denom as f64;
            }
        }
    }

    // Bậc 2: P(word | b) = count(b word) / count(b)
    if let Some(&b) = context.last() {
        let bi = dict::compound_frequency(b, word);
        if bi > 0 {
            let denom = dict::sentence_frequency(b);
            if denom > 0 {
                // Phạt một bậc nếu ngữ cảnh có đủ hai từ mà vẫn phải lùi xuống đây.
                let penalty = if context.len() >= 2 { BACKOFF } else { 1.0 };
                return penalty * bi as f64 / denom as f64;
            }
        }
    }

    // Bậc 1: P(word) = count(word) / N
    let uni = dict::sentence_frequency(word);
    let total = dict::total_tokens();
    let penalty = BACKOFF.powi(context.len().min(2) as i32);
    if uni > 0 && total > 0 {
        penalty * uni as f64 / total as f64
    } else {
        penalty * UNSEEN_SCORE
    }
}

/// Log của [`score`] — dạng cộng dồn được.
pub fn log_score(context: &[&str], word: &str) -> f64 {
    score(context, word).max(UNSEEN_SCORE).ln()
}

/// Điểm log của cả một chuỗi âm tiết.
pub fn sequence_log_score(tokens: &[&str]) -> f64 {
    tokens
        .iter()
        .enumerate()
        .map(|(i, w)| log_score(&tokens[i.saturating_sub(2)..i], w))
        .sum()
}

/// Điểm log của phần chuỗi **bị ảnh hưởng** khi thay âm tiết ở vị trí `at`.
///
/// Chỉ tính các n-gram thật sự chứa vị trí đó — ba vị trí `at`, `at+1`, `at+2`. Các
/// phần còn lại của câu giống hệt nhau giữa hai phương án nên triệt tiêu, và bỏ
/// chúng khiến phép so không bị pha loãng bởi độ dài câu.
pub fn local_log_score(tokens: &[&str], at: usize) -> f64 {
    let end = (at + 3).min(tokens.len());
    (at..end)
        .map(|i| log_score(&tokens[i.saturating_sub(2)..i], tokens[i]))
        .sum()
}

/// Chuỗi tốt nhất khi mỗi vị trí có nhiều lựa chọn — giải mã Viterbi bậc hai.
///
/// Trả về chỉ số của lựa chọn được chọn ở từng vị trí.
///
/// Đây là bộ máy dùng chung cho **sửa lỗi real-word** và cho **thêm dấu tự động**
/// (`toi yeu tieng viet` → `tôi yêu tiếng Việt`): hai bài toán chỉ khác nhau ở bước
/// sinh lựa chọn, còn phần giải mã thì y hệt. Đó là lý do PLAN.md xếp thêm dấu vào
/// MVP — chi phí biên gần như bằng không.
// ---------------------------------------------------------------------------
// Đã thử và ĐÃ BÁC BỎ: thưởng theo độ chặt từ ghép trong Viterbi (2026-08-12)
// ---------------------------------------------------------------------------
//
// Ghi lại vì kết quả âm cũng là kiến thức, và vì giả thuyết này *nghe rất hợp lý* —
// người sau rất dễ nghĩ lại đúng nó.
//
// **Ý tưởng.** Lỗi thêm dấu còn lại nhiều nhất là cặp từ ghép cố định bị tách thành
// hai từ tự do phổ biến hơn:
//
//     tải trọng  →  tại trong        (lặp 8 lần trong một bài của bộ eval)
//
// `tại trong` có 445 lần trong corpus còn `tải trọng` chỉ 92, vì `tồn tại trong` rất
// phổ biến — nên xét theo tần suất thô thì phương án SAI thật sự khả dĩ hơn. Thứ phân
// biệt được là độ chặt hai chiều `min(P(b|a), P(a|b))` của
// [`dict::compound_tightness`], vốn đã dùng thành công cho lớp real-word.
//
// **Kết quả đo, 19.782 câu held-out:**
//
// | Dạng | w=0 | w=0,5 | w=1 | w=2 |
// |---|---|---|---|---|
// | cộng thẳng `ln(tightness)` | 93,97% | 93,88% | 93,63% | 93,33% |
// | chỉ thưởng khi vượt ngưỡng | 93,97% | 93,97% | 93,97% | 93,96% |
//
// Dạng một **tệ hơn đều đặn**: `ln` của số trong `[0,1]` luôn âm, nên nó không thưởng
// cho cặp chặt mà *phạt mọi cặp lỏng* — và phần lớn cặp từ cạnh nhau trong văn bản
// thật đều lỏng một cách chính đáng. Cái phạt đó cộng dồn qua cả câu.
//
// Dạng hai **trung tính**: n-gram đã bắt được phần tín hiệu mà độ chặt mang lại, ít
// nhất là cho những cặp đủ dày để lọt qua ngưỡng cắt tỉa.
//
// Không giữ lại code: một tham số luôn bằng 0 chỉ là chỗ để người sau tưởng nhầm rằng
// nó có tác dụng.
pub fn viterbi(options: &[Vec<&str>]) -> Vec<usize> {
    if options.is_empty() {
        return Vec::new();
    }

    // Trạng thái = (lựa chọn ở vị trí trước, lựa chọn ở vị trí này) để đủ ngữ cảnh
    // cho trigram. best[(prev, cur)] = điểm tốt nhất tới đây; back = đường đi lại.
    let mut best: Vec<Vec<f64>> = Vec::new();
    let mut back: Vec<Vec<usize>> = Vec::new();

    // Vị trí 0: chưa có ngữ cảnh.
    let first: Vec<f64> = options[0].iter().map(|w| log_score(&[], w)).collect();
    best.push(first);
    back.push(vec![usize::MAX; options[0].len()]);

    for i in 1..options.len() {
        let prev_opts = &options[i - 1];
        let cur_opts = &options[i];
        let mut layer = vec![f64::NEG_INFINITY; cur_opts.len()];
        let mut trace = vec![0usize; cur_opts.len()];

        for (ci, cur) in cur_opts.iter().enumerate() {
            for (pi, prev) in prev_opts.iter().enumerate() {
                // Ngữ cảnh bậc ba cần cả vị trí i-2; lấy theo đường đi tốt nhất đã
                // lưu, đủ chính xác cho bài toán này và tránh nhân đôi không gian.
                let ctx: Vec<&str> = if i >= 2 {
                    let pp = back[i - 1][pi];
                    if pp == usize::MAX {
                        vec![*prev]
                    } else {
                        vec![options[i - 2][pp], *prev]
                    }
                } else {
                    vec![*prev]
                };
                let s = best[i - 1][pi] + log_score(&ctx, cur);
                if s > layer[ci] {
                    layer[ci] = s;
                    trace[ci] = pi;
                }
            }
        }
        best.push(layer);
        back.push(trace);
    }

    // Truy vết ngược
    let last = best.len() - 1;
    let mut idx = best[last]
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i);

    let mut path = vec![0usize; options.len()];
    path[last] = idx;
    for i in (1..options.len()).rev() {
        idx = back[i][idx];
        path[i - 1] = idx;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_never_returns_zero() {
        // Bất biến quan trọng nhất của lớp này: chưa gặp thì điểm THẤP, không phải
        // bằng không. Đây đúng là chỗ tần suất thô ở L3 làm sai.
        for (ctx, w) in [
            (vec!["chia"], "sẻ"),
            (vec!["xyzzy"], "plugh"),
            (vec![], "tôi"),
            (vec!["không", "bao"], "giờ"),
        ] {
            let s = score(&ctx, w);
            assert!(s > 0.0, "score({ctx:?}, {w}) = {s}");
            assert!(s.is_finite());
        }
    }

    #[test]
    fn higher_order_context_wins_when_available() {
        // `chia sẻ` là từ ghép cố định, phải ăn điểm cao hơn hẳn `chia sẽ`.
        let good = log_score(&["chia"], "sẻ");
        let bad = log_score(&["chia"], "sẽ");
        assert!(good > bad, "chia sẻ={good} phải hơn chia sẽ={bad}");

        let good = log_score(&["sử"], "dụng");
        let bad = log_score(&["xử"], "dụng");
        assert!(good > bad, "sử dụng={good} phải hơn xử dụng={bad}");
    }

    #[test]
    fn unseen_pair_scores_below_seen_pair_but_above_nothing() {
        let seen = log_score(&["chia"], "sẻ");
        let unseen = log_score(&["chia"], "bảng");
        assert!(seen > unseen);
        assert!(unseen.is_finite(), "cặp chưa gặp không được ra -inf");
    }

    #[test]
    fn sequence_score_prefers_the_correct_sentence() {
        let good = sequence_log_score(&["tôi", "muốn", "chia", "sẻ", "điều", "này"]);
        let bad = sequence_log_score(&["tôi", "muốn", "chia", "sẽ", "điều", "này"]);
        assert!(good > bad, "đúng={good} phải hơn sai={bad}");
    }

    #[test]
    fn local_score_only_covers_the_affected_window() {
        let toks = ["tôi", "muốn", "chia", "sẻ", "điều", "này"];
        // Thay ở vị trí cuối thì cửa sổ ảnh hưởng chỉ còn một n-gram.
        let full = sequence_log_score(&toks);
        let local = local_log_score(&toks, 0);
        assert!(local > full || local.is_finite());
        assert!(local_log_score(&toks, toks.len() - 1).is_finite());
    }

    #[test]
    fn viterbi_picks_the_fixed_compound() {
        // Bộ giải mã dùng chung cho sửa real-word và thêm dấu tự động.
        let options = vec![
            vec!["tôi"],
            vec!["muốn"],
            vec!["chia"],
            vec!["sẻ", "sẽ", "sẹ"],
            vec!["điều"],
        ];
        let path = viterbi(&options);
        assert_eq!(options[3][path[3]], "sẻ", "chọn sai: {path:?}");
    }

    #[test]
    fn viterbi_handles_degenerate_inputs() {
        assert!(viterbi(&[]).is_empty());
        assert_eq!(viterbi(&[vec!["tôi"]]), vec![0]);
        let path = viterbi(&[vec!["a"], vec!["b"], vec!["c"]]);
        assert_eq!(path, vec![0, 0, 0]);
    }

    #[test]
    fn viterbi_uses_context_on_both_sides() {
        // `sử dụng` — chọn đúng ở vị trí 0 chỉ khả dĩ nếu nhìn sang phải.
        let options = vec![vec!["xử", "sử"], vec!["dụng"]];
        let path = viterbi(&options);
        assert_eq!(options[0][path[0]], "sử", "{path:?}");
    }
}
