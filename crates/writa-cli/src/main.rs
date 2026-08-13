//! Công cụ dòng lệnh của Writa.
//!
//! ```text
//! count                    kích thước tập âm tiết sinh ra
//! dump <file>              xuất tập âm tiết
//! verify <freq.tsv>        đối chiếu tập sinh ra với corpus thực
//! check "<text>"           kiểm tra một đoạn text, in vị trí lỗi
//! scan <sentences.txt>     đo FALSE-POSITIVE RATE trên văn bản thật
//! ```
//!
//! Hai lệnh cuối là hai vòng kiểm chứng quan trọng nhất:
//!
//! - `verify` bắt lỗ trong bảng ngữ âm (đã tìm ra bug nhập chữ `gì`/`gìn`/`quỳnh`).
//! - `scan` đo chỉ tiêu mà PLAN.md coi là quan trọng nhất của cả dự án:
//!   **< 2 false positive / 1000 từ**. Chạy trên văn bản Wikipedia — vốn gần như
//!   đúng chính tả — nên gần hết những gì engine báo ở đó đều là báo oan.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;

use writa_core::{phonology, token};

/// Ký tự chỉ có trong tiếng Việt. Token chứa ít nhất một ký tự này gần như chắc
/// chắn là tiếng Việt, nên nếu bảng của ta loại nó thì đó là **tín hiệu thật**.
/// Token ASCII thuần phần lớn là từ tiếng Anh trong bài — nhiễu, không phải lỗi.
const VN_ONLY_CHARS: &str = "ăâêôơưđàáảãạằắẳẵặầấẩẫậèéẻẽẹềếểễệìíỉĩịòóỏõọồốổỗộờớởỡợùúủũụừứửữựỳýỷỹỵ";

fn looks_vietnamese(s: &str) -> bool {
    s.chars().any(|c| VN_ONLY_CHARS.contains(c))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("count") => cmd_count(),
        Some("dump") => cmd_dump(args.get(1).map(Path::new)),
        Some("verify") => cmd_verify(
            Path::new(args.get(1).ok_or("thiếu đường dẫn tới file tần suất")?),
            args.iter()
                .position(|a| a == "--top")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        ),
        Some("check") => {
            cmd_check(args.get(1).ok_or("thiếu text cần kiểm tra")?);
            Ok(())
        }
        Some("scan") => cmd_scan(
            Path::new(args.get(1).ok_or("thiếu đường dẫn tới file câu")?),
            args.iter()
                .position(|a| a == "--top")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(40),
            args.iter().any(|a| a == "--unattested"),
            !args.iter().any(|a| a == "--no-realword"),
            flag(&args, "--margin", writa_core::DEFAULT_REAL_WORD_MARGIN),
            flag(
                &args,
                "--edit-margin",
                writa_core::DEFAULT_EXTRA_EDIT_MARGIN,
            ),
        ),
        Some("make-eval") => cmd_make_eval(
            Path::new(args.get(1).ok_or("thiếu file câu sạch")?),
            Path::new(args.get(2).ok_or("thiếu đường dẫn xuất")?),
        ),
        Some("eval") => cmd_eval(
            Path::new(args.get(1).ok_or("thiếu file eval")?),
            flag(&args, "--margin", writa_core::DEFAULT_REAL_WORD_MARGIN),
            flag(
                &args,
                "--edit-margin",
                writa_core::DEFAULT_EXTRA_EDIT_MARGIN,
            ),
        ),
        Some("eval-realtime") => cmd_eval_realtime(
            Path::new(args.get(1).ok_or("thiếu file câu held-out")?),
            Path::new(args.get(2).ok_or("thiếu file eval đã tiêm lỗi")?),
            flag(&args, "--margin", writa_core::DEFAULT_REAL_WORD_MARGIN),
            flag(
                &args,
                "--edit-margin",
                writa_core::DEFAULT_EXTRA_EDIT_MARGIN,
            ),
        ),
        Some("restore") => {
            let text = args.get(1).ok_or("thiếu text cần thêm dấu")?;
            println!("{}", writa_core::diacritic::restore(text));
            Ok(())
        }
        Some("eval-diacritic") => {
            cmd_eval_diacritic(Path::new(args.get(1).ok_or("thiếu file câu")?))
        }
        Some("explain") => {
            cmd_explain(args.get(1).ok_or("thiếu text cần giải thích")?);
            Ok(())
        }
        Some("dict") => {
            let s = writa_core::dict::stats();
            println!("Âm tiết đã chứng thực : {}", s.attested);
            println!("Từ ngoại được chấp nhận: {}", s.accepted_foreign);
            println!("Từ ghép 2 âm tiết      : {}", s.compounds);
            Ok(())
        }
        _ => {
            eprintln!("Cách dùng:");
            eprintln!("  writa-cli count");
            eprintln!("  writa-cli dump [file]");
            eprintln!("  writa-cli verify <freq.tsv> [--top N]");
            eprintln!("  writa-cli check \"<text>\"");
            eprintln!("  writa-cli scan <sentences.txt> [--top N] [--margin N] [--edit-margin N]");
            eprintln!("  writa-cli eval <eval.tsv> [--margin N] [--edit-margin N]");
            Ok(())
        }
    }
}

/// Số từ ngữ cảnh Tier 2 giữ lại. Phải khớp `realtime::CONTEXT_WORDS`.
const RT_CONTEXT: usize = 5;
/// Số vị trí cuối Tier 2 xét mỗi bước. Phải khớp `realtime::RECHECK_WORDS`.
const RT_RECHECK: usize = 2;

/// Mô phỏng Tier 2 trên một câu, trả về các **chỉ số từ** bị báo lỗi.
///
/// # Vì sao cần một phép đo riêng
///
/// `scan` và `eval` đưa **cả câu** cho engine. Tier 2 thì không bao giờ có cả câu: nó
/// chỉ có những từ đã gõ xong, tối đa `RT_CONTEXT` từ, và ngữ cảnh đó lại bị vứt sạch
/// mỗi lần user di con trỏ hay đổi cửa sổ.
///
/// Khác biệt đó không nhỏ. `chia sẽ` đứng một mình được chênh 5,56 — dưới ngưỡng 6 nên
/// im lặng — còn `muốn chia sẽ` thì vượt. Ngưỡng chọn trên câu đầy đủ vì thế **quá
/// chặt** với Tier 2, và không có phép đo này thì không cách nào biết chặt bao nhiêu.
fn simulate_realtime(sentence: &str, opts: writa_core::CheckOptions) -> Vec<usize> {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let mut flagged = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for end in 1..=words.len() {
        let start = end.saturating_sub(RT_CONTEXT);
        let window = &words[start..end];
        let text = window.join(" ");

        let mut starts = Vec::with_capacity(window.len());
        let mut at = 0usize;
        for w in window {
            starts.push(at);
            at += w.len() + 1;
        }

        let diagnostics = writa_core::check_with(&text, opts);
        for local in (window.len().saturating_sub(RT_RECHECK)..window.len()).rev() {
            let hit = diagnostics.iter().any(|d| {
                d.span.start == starts[local]
                    && !matches!(
                        d.kind,
                        writa_core::DiagnosticKind::Punctuation
                            | writa_core::DiagnosticKind::Capitalization
                    )
                    && !d.candidates.is_empty()
            });
            if hit {
                let global = start + local;
                if seen.insert(global) {
                    flagged.push(global);
                }
                break; // Tier 2 chỉ hiện MỘT gợi ý mỗi bước.
            }
        }
    }
    flagged
}

/// Đo Tier 2: báo oan trên văn bản sạch, và recall trên lỗi đã tiêm.
fn cmd_eval_realtime(
    heldout: &Path,
    injected: &Path,
    margin: f64,
    edit_margin: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let opts = writa_core::CheckOptions {
        real_word_margin: margin,
        extra_edit_margin: edit_margin,
        ..Default::default()
    };

    // --- Báo oan, trên văn bản (gần như) sạch -------------------------------
    let clean = fs::read_to_string(heldout)?;
    let (mut words_seen, mut fp_viet, mut fp_ascii) = (0u64, 0u64, 0u64);
    for line in clean.lines().take(20_000) {
        let words: Vec<&str> = line.split_whitespace().collect();
        words_seen += words.len() as u64;
        for i in simulate_realtime(line, opts) {
            if looks_vietnamese(words[i]) {
                fp_viet += 1;
            } else {
                fp_ascii += 1;
            }
        }
    }

    // --- Recall, trên lỗi đã tiêm -------------------------------------------
    let text = fs::read_to_string(injected)?;
    let (mut total, mut caught) = (0u64, 0u64);
    for line in text.lines().take(20_000) {
        let cols: Vec<&str> = line.split('\t').collect();
        let [start, _end, _wrong, _right, sentence] = cols[..] else {
            continue;
        };
        let Ok(start) = start.parse::<usize>() else {
            continue;
        };
        total += 1;

        // Chỉ số từ chứa vị trí byte đã tiêm.
        let mut at = 0usize;
        let mut target = None;
        for (i, w) in sentence.split_whitespace().enumerate() {
            // `split_whitespace` bỏ khoảng trắng nên phải dò lại vị trí thật.
            let Some(rel) = sentence[at..].find(w) else {
                break;
            };
            let abs = at + rel;
            if (abs..abs + w.len()).contains(&start) {
                target = Some(i);
                break;
            }
            at = abs + w.len();
        }
        let Some(target) = target else { continue };
        if simulate_realtime(sentence, opts).contains(&target) {
            caught += 1;
        }
    }

    let per_1000 = |n: u64| n as f64 * 1000.0 / words_seen.max(1) as f64;
    println!("{}", "=".repeat(70));
    println!("EVAL TIER 2 — mô phỏng ngữ cảnh {RT_CONTEXT} từ, xét {RT_RECHECK} vị trí cuối");
    println!("  margin = {margin}, edit-margin = {edit_margin}");
    println!("{}", "=".repeat(70));
    println!("Từ đã quét              : {words_seen}");
    println!(
        "Báo oan, token Việt     : {fp_viet}  →  {:.2} / 1000 từ",
        per_1000(fp_viet)
    );
    println!(
        "Báo oan, token ASCII    : {fp_ascii}  →  {:.2} / 1000 từ",
        per_1000(fp_ascii)
    );
    println!();
    println!("Lỗi đã tiêm             : {total}");
    println!("Tier 2 bắt được         : {caught}");
    println!(
        "  Recall                : {:.1}%",
        100.0 * caught as f64 / total.max(1) as f64
    );
    Ok(())
}

/// Đọc một tuỳ chọn dạng `--tên <số>`, hoặc trả giá trị mặc định.
fn flag(args: &[String], name: &str, default: f64) -> f64 {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Kiểm tra một đoạn text và chỉ ra vị trí lỗi bằng dấu mũi nhọn.
fn cmd_check(text: &str) {
    let diags = writa_core::check(text);
    println!("{text}");

    // Dựng dòng chỉ vị trí. Đếm theo ký tự hiển thị, không theo byte, để mũi nhọn
    // nằm đúng chỗ với text tiếng Việt.
    let mut marks: Vec<char> = vec![' '; text.chars().count()];
    for d in &diags {
        let start_col = text[..d.span.start].chars().count();
        let width = text[d.span.clone()].chars().count().max(1);
        for k in 0..width {
            if let Some(slot) = marks.get_mut(start_col + k) {
                *slot = '^';
            }
        }
    }
    if !diags.is_empty() {
        println!("{}", marks.iter().collect::<String>());
    }

    if diags.is_empty() {
        println!("\n✓ không thấy lỗi");
    } else {
        println!();
        for d in &diags {
            println!(
                "  {:?} · {:?} · byte {}..{}  →  {:?}",
                d.found, d.kind, d.span.start, d.span.end, d.candidates
            );
        }
    }
}

/// Sinh số giả ngẫu nhiên tất định — cùng đầu vào luôn cho cùng bộ test.
///
/// Tất định là yêu cầu, không phải tiện lợi: nếu bộ test đổi mỗi lần chạy thì
/// không so được hai lần đo với nhau, mà so được chính là mục đích.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

/// Tiêm lỗi vào câu sạch để đo **recall**.
///
/// # Vì sao cần
///
/// `scan` chỉ đo false-positive trên văn bản đúng — nó không cho biết engine BỎ SÓT
/// bao nhiêu. Không có recall thì không chọn được ngưỡng: ngưỡng vô cực cho FP bằng
/// 0 và cũng bắt được đúng 0 lỗi.
///
/// # Giới hạn phải nói rõ
///
/// Lỗi được tiêm bằng **chính bảng luật** mà engine dùng để sinh candidate. Nên số
/// recall ở đây đo **khả năng CHẤM ĐIỂM** (mô hình ngôn ngữ có xếp phương án đúng
/// lên đầu và vượt ngưỡng không), chứ không đo khả năng **sinh candidate**. Lỗi thật
/// mà bảng luật chưa phủ sẽ không xuất hiện ở đây.
///
/// Muốn đo trọn vẹn thì cần bộ lỗi thật gán nhãn tay — PLAN.md xếp ~500 câu cho
/// việc đó, và đó là việc còn lại.
fn cmd_make_eval(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(input)?;
    let mut rng = Lcg(0x5EED_1234_ABCD);
    let mut out = String::new();
    let mut n_sent = 0usize;
    let mut n_err = 0usize;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Chỉ lấy câu mà engine hiện KHÔNG báo gì. Nếu câu vốn đã bị báo thì không
        // tách được lỗi ta tiêm với lỗi sẵn có.
        if !writa_core::check(line).is_empty() {
            continue;
        }

        let words: Vec<token::Token> = token::tokenize(line)
            .into_iter()
            .filter(|t| t.kind == token::TokenKind::Word && t.protect.is_none())
            .filter(|t| writa_core::phonology::is_valid_syllable(&t.normalized))
            .collect();
        if words.len() < 4 {
            continue;
        }

        let target = &words[(rng.next() as usize) % words.len()];
        // Chỉ tiêm lỗi thuộc loại người Việt thật sự mắc, và loại bỏ biến thể
        // đều đúng — tiêm `kỹ`→`kĩ` rồi đòi engine báo là tự mâu thuẫn.
        let cands: Vec<_> = writa_core::candidate::for_syllable(&target.normalized)
            .into_iter()
            .filter(|c| {
                matches!(
                    c.reason,
                    writa_core::candidate::Reason::ToneConfusion
                        | writa_core::candidate::Reason::Onset
                        | writa_core::candidate::Reason::Rime
                )
            })
            .collect();
        if cands.is_empty() {
            continue;
        }
        let wrong = &cands[(rng.next() as usize) % cands.len()].text;

        let mut corrupted = line.to_string();
        corrupted.replace_range(target.span.clone(), wrong);
        // Vị trí lỗi trong câu ĐÃ hỏng
        let start = target.span.start;
        let end = start + wrong.len();

        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            start, end, wrong, target.normalized, corrupted
        ));
        n_sent += 1;
        n_err += 1;
    }

    fs::write(output, &out)?;
    println!(
        "Đã sinh {n_err} lỗi trong {n_sent} câu → {}",
        output.display()
    );
    println!("Định dạng: start<TAB>end<TAB>dạng_sai<TAB>dạng_đúng<TAB>câu");
    println!();
    println!("LƯU Ý: lỗi tiêm bằng chính bảng luật của engine, nên recall ở đây đo");
    println!("khả năng CHẤM ĐIỂM chứ không đo khả năng SINH candidate.");
    Ok(())
}

/// Đo precision / recall / F0.5 trên bộ test đã tiêm lỗi.
fn cmd_eval(path: &Path, margin: f64, edit_margin: f64) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let opts = writa_core::CheckOptions {
        real_word_margin: margin,
        extra_edit_margin: edit_margin,
        ..Default::default()
    };

    let (mut caught, mut caught_right, mut total) = (0u64, 0u64, 0u64);
    // Báo ở vị trí khác, tách làm hai vì chúng là **hai vấn đề độc lập**:
    //
    // - `extra_ascii`: từ vay mượn tần suất thấp (`subroutine`, `ketchup`,
    //   `gerrard`) mà lớp L2 chưa nhận. `scan` gọi đây là nhóm B và theo dõi riêng
    //   với ngưỡng riêng, vì lời giải của nó là *corpus lớn hơn*, không phải chỉnh
    //   ngưỡng phán quyết.
    // - `extra_viet`: báo oan trên token tiếng Việt thật — thứ mà mọi thay đổi ở
    //   L3/L4 thực sự tác động tới.
    //
    // Gộp chúng vào một con số precision là cách tự làm mù mình: nó nhảy khi ta
    // đổi corpus và đứng yên khi ta đổi thuật toán, tức là đúng ngược với thứ ta
    // cần biết.
    let (mut extra_ascii, mut extra_viet) = (0u64, 0u64);

    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        let [start, _end, _wrong, right, sentence] = cols[..] else {
            continue;
        };
        let Ok(start) = start.parse::<usize>() else {
            continue;
        };
        total += 1;

        let mut hit = false;
        for d in writa_core::check_with(sentence, opts) {
            // Dấu câu không thuộc phép đo này: lỗi được tiêm là lỗi CHÍNH TẢ, còn lớp L5
            // thì tất định và có ngân sách riêng. Đếm nó vào đây chỉ làm precision nhảy
            // theo mật độ dấu phẩy của corpus. `scan` báo nó thành nhóm D.
            if matches!(
                d.kind,
                writa_core::DiagnosticKind::Punctuation
                    | writa_core::DiagnosticKind::Capitalization
            ) {
                continue;
            }
            if d.span.start == start {
                hit = true;
                if d.candidates.iter().any(|c| c == right) {
                    caught_right += 1;
                }
            } else if d.found.is_ascii() {
                extra_ascii += 1;
            } else {
                extra_viet += 1;
            }
        }
        if hit {
            caught += 1;
        }
    }

    let recall = caught as f64 / total.max(1) as f64;
    let recall_fix = caught_right as f64 / total.max(1) as f64;
    let precision_all = caught as f64 / (caught + extra_ascii + extra_viet).max(1) as f64;
    let precision_viet = caught as f64 / (caught + extra_viet).max(1) as f64;
    // F0.5 — ưu tiên precision gấp đôi recall, đúng hướng lệch của dự án. Tính trên
    // precision đã bỏ nhóm B, để chỉ số này phản ánh chất lượng L3/L4.
    let f05 = if precision_viet + recall > 0.0 {
        1.25 * precision_viet * recall / (0.25 * precision_viet + recall)
    } else {
        0.0
    };

    println!("{}", "=".repeat(70));
    println!("EVAL — bộ test tiêm lỗi (margin = {margin}, edit-margin = {edit_margin})");
    println!("{}", "=".repeat(70));
    println!("Lỗi đã tiêm             : {total}");
    println!("Phát hiện đúng vị trí   : {caught}");
    println!("  … và đề xuất đúng     : {caught_right}");
    println!("Báo ở vị trí khác       : {}", extra_ascii + extra_viet);
    println!("  … token ASCII (nhóm B): {extra_ascii}   ← từ vay mượn, vấn đề của corpus");
    println!("  … token tiếng Việt    : {extra_viet}   ← báo oan thật của L3/L4");
    println!();
    println!("  Recall (phát hiện)    : {:.1}%", 100.0 * recall);
    println!("  Recall (sửa đúng)     : {:.1}%", 100.0 * recall_fix);
    println!("  Precision (toàn bộ)   : {:.1}%", 100.0 * precision_all);
    println!("  Precision (bỏ nhóm B) : {:.1}%", 100.0 * precision_viet);
    println!("  F0.5                  : {:.3}   (ưu tiên precision)", f05);
    Ok(())
}

/// Đo độ chính xác thêm dấu.
///
/// Bộ test tự sinh và **không bị vòng tròn theo cách nguy hiểm**: câu gốc là ground
/// truth, ta bỏ dấu rồi bắt engine dựng lại. Engine không hề thấy bản gốc. Điều kiện
/// duy nhất là câu gốc phải đúng chính tả — Wikipedia gần đúng, nên vài phần trăm
/// sai lệch cuối cùng là lỗi của corpus, không phải của engine.
fn cmd_eval_diacritic(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;

    let (mut total, mut correct) = (0u64, 0u64);
    // Tách riêng từ viết hoa: gần như toàn tên riêng, và mô hình n-gram cấp âm tiết
    // về nguyên tắc không biết `Lão Hạc` là tên tác phẩm. Gộp chung thì con số bị
    // tên riêng kéo xuống và che mất chất lượng trên văn xuôi thường.
    let (mut lower_total, mut lower_correct) = (0u64, 0u64);
    let (mut upper_total, mut upper_correct) = (0u64, 0u64);
    let (mut sent_total, mut sent_perfect) = (0u64, 0u64);
    let mut worst: Vec<(String, String)> = Vec::new();
    let started = std::time::Instant::now();

    for line in text.lines().take(20_000) {
        if line.trim().is_empty() {
            continue;
        }
        let stripped = writa_core::diacritic::remove(line);
        // Câu vốn đã không dấu thì không đo được gì.
        if stripped == line {
            continue;
        }
        let restored = writa_core::diacritic::restore(&stripped);

        sent_total += 1;
        let gold: Vec<&str> = line.split_whitespace().collect();
        let got: Vec<&str> = restored.split_whitespace().collect();
        if gold.len() != got.len() {
            continue; // lệch token: không so từng từ được
        }

        let mut all_ok = true;
        for (g, r) in gold.iter().zip(got.iter()) {
            total += 1;
            let capitalised = g.chars().next().is_some_and(char::is_uppercase);
            if capitalised {
                upper_total += 1;
            } else {
                lower_total += 1;
            }

            if g == r {
                correct += 1;
                if capitalised {
                    upper_correct += 1;
                } else {
                    lower_correct += 1;
                }
            } else {
                all_ok = false;
                if !capitalised && worst.len() < 30 {
                    // Chỉ giữ mẫu chữ thường: mẫu tên riêng đã biết trước là khó,
                    // không cho thêm thông tin gì để sửa.
                    worst.push(((*g).to_string(), (*r).to_string()));
                }
            }
        }
        if all_ok {
            sent_perfect += 1;
        }
    }

    let acc = 100.0 * correct as f64 / total.max(1) as f64;
    let sent_acc = 100.0 * sent_perfect as f64 / sent_total.max(1) as f64;

    println!("{}", "=".repeat(70));
    println!("EVAL — thêm dấu tự động");
    println!("{}", "=".repeat(70));
    println!("Nguồn            : {}", path.display());
    println!("Câu              : {sent_total}");
    println!("Từ               : {total}");
    println!();
    let pct = |c: u64, t: u64| 100.0 * c as f64 / t.max(1) as f64;

    println!("  Đúng theo TỪ (gộp) : {acc:.2}%   (mục tiêu PLAN.md: > 95%)");
    println!("  Đúng cả CÂU        : {sent_acc:.2}%");
    println!();
    println!("  Tách theo kiểu viết — hai bài toán khác hẳn nhau:");
    println!(
        "    chữ thường (văn xuôi) : {:.2}%   ({lower_total} từ)",
        pct(lower_correct, lower_total)
    );
    println!(
        "    viết hoa (tên riêng)  : {:.2}%   ({upper_total} từ)",
        pct(upper_correct, upper_total)
    );
    println!();
    println!(
        "  Thời gian          : {:.1}s",
        started.elapsed().as_secs_f64()
    );
    println!();
    println!("{}", "-".repeat(70));
    println!("MẪU SAI CHỮ THƯỜNG — cột trái là đúng, cột phải là engine đoán");
    println!("{}", "-".repeat(70));
    for (gold, got) in worst.iter().take(24) {
        println!("  {gold:<18} → {got}");
    }
    Ok(())
}

/// In điểm mô hình ngôn ngữ cho từng vị trí và từng phương án thay thế.
///
/// Đây là công cụ để chỉnh ngưỡng bằng SỐ ĐO thay vì bằng cảm giác: nó cho thấy
/// chính xác khoảng cách log giữa bản gốc và phương án tốt nhất ở mỗi vị trí.
fn cmd_explain(text: &str) {
    let words: Vec<token::Token> = token::tokenize(text)
        .into_iter()
        .filter(|t| t.kind == token::TokenKind::Word)
        .collect();
    let seq: Vec<&str> = words.iter().map(|t| t.normalized.as_str()).collect();

    println!("{text}");
    println!("{}", "=".repeat(76));
    println!(
        "{:<12} {:>10} {:>10}  phương án tốt nhất",
        "âm tiết", "điểm gốc", "chênh"
    );
    println!("{}", "-".repeat(76));

    for (i, tok) in words.iter().enumerate() {
        let base = writa_core::lm::local_log_score(&seq, i);
        let mut ranked: Vec<(String, f64, String)> = Vec::new();

        for cand in writa_core::candidate::for_syllable(&tok.normalized) {
            let mut alt = seq.clone();
            alt[i] = cand.text.as_str();
            let gain = writa_core::lm::local_log_score(&alt, i) - base;
            ranked.push((cand.text.clone(), gain, format!("{:?}", cand.reason)));
        }
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));

        let note = tok
            .protect
            .map_or(String::new(), |r| format!("(bỏ qua: {r:?})"));
        match ranked.first() {
            Some((text, gain, reason)) => println!(
                "{:<12} {:>10.2} {:>10.2}  {} [{}] {}",
                tok.normalized, base, gain, text, reason, note
            ),
            None => println!("{:<12} {base:>10.2}             — {note}", tok.normalized),
        }
        // Vài phương án kế tiếp để thấy khoảng cách giữa chúng
        let indent = " ".repeat(23);
        for (text, gain, reason) in ranked.iter().skip(1).take(3) {
            println!("{indent}{gain:>10.2}  {text} [{reason}]");
        }
    }
}

/// Đo false-positive rate trên văn bản thật.
fn cmd_scan(
    path: &Path,
    top: usize,
    flag_unattested: bool,
    detect_real_word: bool,
    real_word_margin: f64,
    extra_edit_margin: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let opts = writa_core::CheckOptions {
        flag_unattested,
        detect_real_word,
        real_word_margin,
        extra_edit_margin,
        ..Default::default()
    };
    let started = std::time::Instant::now();

    if flag_unattested {
        eprintln!("{}", "!".repeat(76));
        eprintln!("CẢNH BÁO: con số dưới đây VÔ NGHĨA nếu file câu này lấy từ cùng");
        eprintln!("corpus đã dùng để dựng data/lexicon/syllables.tsv.");
        eprintln!();
        eprintln!("Phép đo sẽ bị vòng tròn: mọi âm tiết trong text đương nhiên đã");
        eprintln!("\"chứng thực\" vì chính nó góp phần tạo ra danh sách chứng thực.");
        eprintln!("Muốn số có nghĩa, dùng văn bản từ NGUỒN KHÁC — chat, diễn đàn,");
        eprintln!("mạng xã hội — nơi có khẩu ngữ và phương ngữ mà Wikipedia không có.");
        eprintln!("{}", "!".repeat(76));
        eprintln!();
    }

    let mut n_lines = 0u64;
    let mut n_words = 0u64;
    let mut n_protected = 0u64;
    let mut lines_with_flag = 0u64;
    // Tách hai loại báo lỗi. Gộp chúng lại thành một con số là cách tự lừa mình:
    // hai loại có nguyên nhân khác nhau và cần lời giải khác nhau.
    let mut flags_vn = 0u64;
    let mut flags_ascii = 0u64;
    let mut flags_realword = 0u64;
    let mut flags_punct = 0u64;
    let mut by_token_vn: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_token_ascii: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_realword: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_punct: BTreeMap<String, u64> = BTreeMap::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        n_lines += 1;

        for tok in token::tokenize(line) {
            if tok.kind == token::TokenKind::Word {
                if tok.protect.is_some() {
                    n_protected += 1;
                } else {
                    n_words += 1;
                }
            }
        }

        let diags = writa_core::check_with(line, opts);
        if !diags.is_empty() {
            lines_with_flag += 1;
        }
        for d in diags {
            // Dấu câu và viết hoa đếm RIÊNG, và đây không phải chi tiết trình bày.
            //
            // Trước đây chúng bị gộp vào nhóm A/B theo nội dung token, và hậu quả là hai
            // con số cốt lõi của dự án bị bóp méo nặng: nhóm B đọc ra 24,03/1000 trong khi
            // phần **từ ngoại** thật chỉ khoảng 6, còn 17,6 kia là dấu phẩy với dấu chấm.
            //
            // Nó cũng chính là lý do số cũ trong tài liệu "không tái lập được": 0,25 và
            // 6,40 được đo TRƯỚC khi lớp L5 tồn tại. Chênh lệch đo lại (0,63 và 17,63)
            // khớp với phần dấu câu (0,62 và 17,61) tới hai chữ số thập phân — cả hai số
            // đều luôn đúng với thứ chúng đo, chỉ phép so sánh là vô nghĩa.
            if matches!(
                d.kind,
                writa_core::DiagnosticKind::Punctuation
                    | writa_core::DiagnosticKind::Capitalization
            ) {
                flags_punct += 1;
                *by_punct.entry(d.found).or_default() += 1;
                continue;
            }
            // Lỗi real-word đếm riêng: nó do lớp L3 gợi ý theo ngữ cảnh, rủi ro
            // khác hẳn hai nhóm kia nên gộp vào sẽ che mất tín hiệu.
            if d.kind == writa_core::DiagnosticKind::ConfusedSyllable {
                flags_realword += 1;
                let entry = format!(
                    "{} → {}",
                    d.found,
                    d.candidates.first().map_or("?", String::as_str)
                );
                *by_realword.entry(entry).or_default() += 1;
            } else if looks_vietnamese(&d.found) {
                flags_vn += 1;
                *by_token_vn.entry(d.found).or_default() += 1;
            } else {
                flags_ascii += 1;
                *by_token_ascii.entry(d.found).or_default() += 1;
            }
        }
    }

    let per_1k = |n: u64| {
        if n_words > 0 {
            1000.0 * n as f64 / n_words as f64
        } else {
            0.0
        }
    };

    println!("{}", "=".repeat(76));
    println!("SCAN — false-positive rate trên văn bản thật");
    println!("{}", "=".repeat(76));
    println!("Nguồn                  : {}", path.display());
    println!("Câu                    : {n_lines}");
    println!("Từ được kiểm tra       : {n_words}");
    println!("Từ được bảo vệ (bỏ qua): {n_protected}");
    println!("Câu có ít nhất 1 báo   : {lines_with_flag}");
    println!();
    println!("HAI LOẠI BÁO LỖI — nguyên nhân khác nhau, lời giải khác nhau");
    println!();
    println!(
        "  A. Token CÓ dấu Việt   : {:>8}  →  {:.2} / 1000 từ",
        flags_vn,
        per_1k(flags_vn)
    );
    println!("     Đây là chỉ tiêu CỐT LÕI của engine. Ngưỡng MVP: < 2,00");
    println!(
        "     {}",
        if per_1k(flags_vn) < 2.0 {
            "ĐẠT"
        } else {
            "CHƯA ĐẠT"
        }
    );
    println!();
    println!(
        "  B. Token ASCII thuần   : {:>8}  →  {:.2} / 1000 từ",
        flags_ascii,
        per_1k(flags_ascii)
    );
    println!("     Từ ngoại lai lẫn trong văn bản Việt (electron, protein, of, and).");
    println!("     KHÔNG phải lỗi bảng ngữ âm — cần từ điển từ vay mượn ở L2.");
    println!("     Tần suất là thứ phân biệt được: từ vay mượn xuất hiện nhiều,");
    println!("     còn lỗi gõ tay (nghanh, chinhs) thì thưa.");
    println!();
    println!(
        "  C. Real-word (L3)      : {:>8}  →  {:.2} / 1000 từ",
        flags_realword,
        per_1k(flags_realword)
    );
    println!("     Mọi âm tiết hợp lệ nhưng tổ hợp sai: chia sẽ, xử dụng.");
    println!("     Lớp RỦI RO NHẤT về báo oan vì nó phán quyết theo ngữ cảnh.");
    println!("     So với `scan --no-realword` để biết phần đóng góp riêng của nó.");
    println!();
    println!(
        "  D. Dấu câu / viết hoa   : {:>8}  →  {:.2} / 1000 từ",
        flags_punct,
        per_1k(flags_punct)
    );
    println!("     Lớp L5, tất định, KHÔNG phải lỗi chính tả — đếm riêng vì gộp vào");
    println!("     A/B sẽ bóp méo hai chỉ tiêu cốt lõi. Trên Wikipedia phần lớn nhóm");
    println!("     này là `1.000`, `T.P`, định dạng chú thích — tức chính đáng.");
    println!();
    println!("Thời gian quét: {:.1}s", started.elapsed().as_secs_f64());

    for (title, map) in [
        (
            "A — CÓ DẤU VIỆT (lỗi engine hoặc lỗi thật trong corpus)",
            &by_token_vn,
        ),
        ("B — ASCII THUẦN (việc của L2)", &by_token_ascii),
        (
            "C — REAL-WORD (gợi ý của L3, soát kỹ nhóm này)",
            &by_realword,
        ),
        ("D — DẤU CÂU / VIẾT HOA (lớp L5)", &by_punct),
    ] {
        println!();
        println!("{}", "-".repeat(76));
        println!("TOP {top} · {title}");
        println!("{}", "-".repeat(76));
        let mut ranked: Vec<(&String, &u64)> = map.iter().collect();
        ranked.sort_by_key(|(t, c)| (std::cmp::Reverse(**c), (*t).clone()));
        for (tok, count) in ranked.iter().take(top) {
            println!("  {count:>7}  {tok}");
        }
        if ranked.len() > top {
            println!("  … còn {} token khác nhau nữa", ranked.len() - top);
        }
    }

    Ok(())
}

fn cmd_count() -> Result<(), Box<dyn std::error::Error>> {
    let set = phonology::syllable_set();
    println!("Âm đầu     : {}", phonology::onsets().len());
    println!("Vần        : {}", phonology::rimes().len());
    println!("Âm tiết    : {}", set.len());
    println!("\n(tham chiếu ~17.974 — ta sinh thêm biến thể vị trí dấu oa/oe/uy)");
    Ok(())
}

fn cmd_dump(out: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let set = phonology::syllable_set();
    match out {
        Some(p) => {
            let mut f = std::io::BufWriter::new(fs::File::create(p)?);
            for s in set {
                writeln!(f, "{s}")?;
            }
            println!("Đã ghi {} âm tiết vào {}", set.len(), p.display());
        }
        None => {
            for s in set {
                println!("{s}");
            }
        }
    }
    Ok(())
}

/// Đọc file `âm_tiết<TAB>tần_suất` do `scripts/extract_syllables.py` sinh ra.
fn read_freq(path: &Path) -> Result<Vec<(String, u64)>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut it = line.split('\t');
        if let (Some(s), Some(c)) = (it.next(), it.next()) {
            if let Ok(c) = c.trim().parse::<u64>() {
                out.push((s.to_string(), c));
            }
        }
    }
    Ok(out)
}

fn cmd_verify(freq_path: &Path, top: usize) -> Result<(), Box<dyn std::error::Error>> {
    let set = phonology::syllable_set();
    let freq = read_freq(freq_path)?;

    let total_tokens: u64 = freq.iter().map(|(_, c)| c).sum();
    let mut accepted_tokens = 0u64;
    let mut rejected_vn: Vec<(&str, u64)> = Vec::new();
    let mut rejected_ascii_tokens = 0u64;
    let mut rejected_vn_tokens = 0u64;
    let mut seen_in_corpus: BTreeSet<&str> = BTreeSet::new();

    for (syl, count) in &freq {
        if set.contains(syl) {
            accepted_tokens += count;
            seen_in_corpus.insert(syl.as_str());
        } else if looks_vietnamese(syl) {
            rejected_vn_tokens += count;
            rejected_vn.push((syl.as_str(), *count));
        } else {
            rejected_ascii_tokens += count;
        }
    }
    rejected_vn.sort_by_key(|(s, c)| (std::cmp::Reverse(*c), *s));

    let pct = |n: u64| 100.0 * n as f64 / total_tokens as f64;

    println!("{}", "=".repeat(76));
    println!("VERIFY bảng ngữ âm ↔ corpus thực");
    println!("{}", "=".repeat(76));
    println!("Nguồn tần suất        : {}", freq_path.display());
    println!("Tập âm tiết sinh ra   : {}", set.len());
    println!("Token trong corpus    : {total_tokens}");
    println!();
    println!("ĐỘ PHỦ (tính theo token — chỉ số quan trọng nhất)");
    println!(
        "  chấp nhận           : {:>12} ({:>5.2}%)",
        accepted_tokens,
        pct(accepted_tokens)
    );
    println!(
        "  loại, CÓ dấu Việt   : {:>12} ({:>5.2}%)  ← tín hiệu bảng còn thiếu",
        rejected_vn_tokens,
        pct(rejected_vn_tokens)
    );
    println!(
        "  loại, ASCII thuần   : {:>12} ({:>5.2}%)  ← phần lớn là từ ngoại, bỏ qua",
        rejected_ascii_tokens,
        pct(rejected_ascii_tokens)
    );
    println!();
    println!("ÂM TIẾT SINH RA nhưng KHÔNG xuất hiện trong corpus");
    println!(
        "  {} / {} ({:.1}%) — hợp lệ ngữ âm nhưng không ai dùng.",
        set.len() - seen_in_corpus.len(),
        set.len(),
        100.0 * (set.len() - seen_in_corpus.len()) as f64 / set.len() as f64
    );
    println!("  Nhóm này là đầu vào cho tín hiệu \"nghi vấn\" ở L2, không phải lỗi bảng.");

    println!();
    println!("{}", "-".repeat(76));
    println!("TOP {top} TOKEN CÓ DẤU VIỆT BỊ LOẠI — soát tay danh sách này");
    println!("{}", "-".repeat(76));
    println!("Token tần suất cao mà là tiếng Việt thật => bảng vần/âm đầu còn thiếu.");
    println!("Token là tên riêng, từ phiên âm, hoặc dính liền hai âm tiết => bỏ qua.");
    println!();
    for (syl, count) in rejected_vn.iter().take(top) {
        println!("  {count:>9}  {syl}");
    }
    if rejected_vn.len() > top {
        println!("  … còn {} token nữa", rejected_vn.len() - top);
    }

    Ok(())
}
