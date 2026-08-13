//! Biến kết quả engine thành thứ popup hiển thị được.
//!
//! Lớp mỏng, nhưng là nơi duy nhất biết cả hai chế độ (kiểm tra chính tả và thêm
//! dấu) đều quy về **cùng một hình dạng**: một danh sách "chỗ này, đổi thành cái
//! kia, còn mấy phương án khác". Nhờ vậy popup chỉ cần một lối vẽ.

use std::ops::Range;

use writa_core::{check_with, diacritic, Confidence, DiagnosticKind};

use crate::config::Settings;
use crate::model::{Change, ChangeKind, Mode};

/// Số ký tự ngữ cảnh mỗi bên. Đủ để nhận ra chỗ nào trong câu, đủ ngắn để nằm gọn
/// một dòng trong popup rộng 520px.
const CONTEXT_CHARS: usize = 26;

pub fn build(mode: Mode, text: &str, settings: &Settings) -> Vec<Change> {
    match mode {
        Mode::Check => from_diagnostics(text, settings),
        Mode::Diacritic => from_restorations(text),
    }
}

fn from_diagnostics(text: &str, settings: &Settings) -> Vec<Change> {
    check_with(text, settings.check_options())
        .into_iter()
        .filter(|d| settings.check_punctuation || d.kind != DiagnosticKind::Punctuation)
        .filter(|d| !settings.ignores(&d.found))
        .enumerate()
        .map(|(id, d)| {
            let (before, after) = context(text, &d.span);
            Change {
                id,
                start: d.span.start,
                end: d.span.end,
                kind: match d.kind {
                    DiagnosticKind::InvalidSyllable => ChangeKind::Invalid,
                    DiagnosticKind::UnattestedSyllable => ChangeKind::Unattested,
                    DiagnosticKind::ConfusedSyllable => ChangeKind::Confused,
                    DiagnosticKind::Punctuation => ChangeKind::Punctuation,
                    DiagnosticKind::Capitalization => ChangeKind::Capitalization,
                },
                // Đoạn gốc, không phải dạng đã chuẩn hoá: user cần thấy đúng thứ họ
                // đã gõ, và span phải khớp với nó để thay đúng chỗ.
                from: text[d.span.clone()].to_string(),
                options: d.candidates,
                certain: d.confidence == Confidence::Certain,
                before,
                after,
            }
        })
        .collect()
}

fn from_restorations(text: &str) -> Vec<Change> {
    diacritic::restore_changes(text)
        .into_iter()
        .enumerate()
        .map(|(id, r)| {
            let (before, after) = context(text, &r.span);
            Change {
                id,
                start: r.span.start,
                end: r.span.end,
                kind: ChangeKind::Diacritic,
                from: r.from,
                options: r.options,
                // Thêm dấu không bao giờ chắc chắn: 94,47% đúng nghĩa là cứ khoảng
                // 18 âm tiết lại có một chỗ sai.
                certain: false,
                before,
                after,
            }
        })
        .collect()
}

/// Vài ký tự hai bên chỗ sửa, đã gộp xuống dòng thành khoảng trắng.
fn context(text: &str, span: &Range<usize>) -> (String, String) {
    let before: String = text[..span.start]
        .chars()
        .rev()
        .take(CONTEXT_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let after: String = text[span.end..].chars().take(CONTEXT_CHARS).collect();
    (flatten(&before), flatten(&after))
}

fn flatten(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Áp các quyết định của user lên bản gốc.
///
/// Đi từ cuối về đầu để span chưa xử lý không bị dịch, và **bỏ qua** span chồng lấn
/// thay vì cố ghép: hai thay đổi cùng đụng một đoạn thì kết quả phụ thuộc thứ tự,
/// mà thứ tự thì không có nghĩa gì với user.
pub fn apply(original: &str, changes: &[Change], decisions: &[crate::model::Decision]) -> String {
    let mut picked: Vec<(&Change, &str)> = decisions
        .iter()
        .filter_map(|d| {
            changes
                .iter()
                .find(|c| c.id == d.id)
                .map(|c| (c, d.replacement.as_str()))
        })
        .collect();
    picked.sort_by_key(|(c, _)| std::cmp::Reverse(c.start));

    let mut out = original.to_string();
    let mut lowest = original.len();
    for (c, replacement) in picked {
        if c.end > lowest {
            continue;
        }
        out.replace_range(c.start..c.end, replacement);
        lowest = c.start;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Decision;

    fn change(id: usize, start: usize, end: usize) -> Change {
        Change {
            id,
            start,
            end,
            kind: ChangeKind::Invalid,
            from: String::new(),
            options: Vec::new(),
            certain: true,
            before: String::new(),
            after: String::new(),
        }
    }

    #[test]
    fn applies_later_spans_without_shifting_earlier_ones() {
        // Bản sửa đầu dài hơn bản gốc, nên span thứ hai sẽ lệch nếu đi xuôi.
        let src = "toi hoc";
        let changes = vec![change(0, 0, 3), change(1, 4, 7)];
        let decisions = vec![
            Decision {
                id: 0,
                replacement: "tôi".into(),
            },
            Decision {
                id: 1,
                replacement: "học".into(),
            },
        ];
        assert_eq!(apply(src, &changes, &decisions), "tôi học");
    }

    #[test]
    fn a_subset_of_decisions_leaves_the_rest_alone() {
        let src = "toi hoc";
        let changes = vec![change(0, 0, 3), change(1, 4, 7)];
        let decisions = vec![Decision {
            id: 1,
            replacement: "học".into(),
        }];
        assert_eq!(apply(src, &changes, &decisions), "toi học");
    }

    #[test]
    fn overlapping_decisions_do_not_corrupt_the_text() {
        // Không hình dung ra được UI nào tạo ra tình huống này, nhưng nếu có thì kết
        // quả phải là "bỏ một cái", không phải "cắt chuỗi ở giữa ký tự".
        let src = "abcdef";
        let changes = vec![change(0, 0, 4), change(1, 2, 6)];
        let decisions = vec![
            Decision {
                id: 0,
                replacement: "X".into(),
            },
            Decision {
                id: 1,
                replacement: "Y".into(),
            },
        ];
        assert_eq!(apply(src, &changes, &decisions), "abY");
    }

    #[test]
    fn unknown_ids_are_ignored() {
        let src = "toi";
        let changes = vec![change(0, 0, 3)];
        let decisions = vec![Decision {
            id: 99,
            replacement: "xxx".into(),
        }];
        assert_eq!(apply(src, &changes, &decisions), "toi");
    }

    #[test]
    fn context_is_cut_on_character_boundaries() {
        // Cắt theo byte ở đây sẽ panic ngay chữ tiếng Việt đầu tiên.
        let src = "Tôi làm trong nghành công nghiệp này";
        let span = src.find("nghành").unwrap()..src.find("nghành").unwrap() + "nghành".len();
        let (before, after) = context(src, &span);
        assert!(before.ends_with("trong "), "{before:?}");
        assert!(after.starts_with(" công"), "{after:?}");
    }

    #[test]
    fn personal_dictionary_silences_a_word() {
        let mut s = Settings::default();
        let text = "Tôi làm trong nghành này";
        assert_eq!(from_diagnostics(text, &s).len(), 1);
        s.personal_dict = vec!["nghành".into()];
        assert!(from_diagnostics(text, &s).is_empty());
    }

    #[test]
    fn punctuation_can_be_turned_off_without_touching_the_engine() {
        let mut s = Settings::default();
        let text = "xin chào , bạn";
        assert!(!from_diagnostics(text, &s).is_empty());
        s.check_punctuation = false;
        assert!(from_diagnostics(text, &s).is_empty());
    }

    #[test]
    fn spans_point_at_the_original_text_not_the_normalized_form() {
        let text = "Tôi làm trong nghành này";
        let c = &from_diagnostics(text, &Settings::default())[0];
        assert_eq!(&text[c.start..c.end], c.from);
    }
}
