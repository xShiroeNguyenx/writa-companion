//! P4 — Bộ đệm từ đang gõ.
//!
//! Đây là phần **thuật toán** của Tier 2 real-time, tách hẳn khỏi Win32 để test được
//! mà không cần bàn phím thật. `hook.rs` chỉ bơm sự kiện vào đây.
//!
//! # Bài toán
//!
//! Hook bàn phím cho ta một dòng sự kiện phím. Ta cần biết **từ nào user vừa gõ
//! xong** để kiểm tra nó. Nghe đơn giản, nhưng có bốn thứ làm hỏng:
//!
//! 1. **Bộ gõ tiếng Việt.** UniKey/EVKey nuốt phím gốc rồi bơm ký tự đã ghép. Bộ
//!    đệm phải theo dòng đã ghép, không phải phím gốc — nếu không `tieengs` vào
//!    buffer thay vì `tiếng`. Xem [`KeySource`].
//! 2. **Backspace.** Phải lùi buffer, và bộ gõ dùng backspace rất nhiều khi ghép.
//! 3. **Con trỏ nhảy chỗ.** User bấm chuột hoặc mũi tên thì buffer không còn ứng với
//!    text trên màn hình nữa — phải vứt đi chứ không được đoán.
//! 4. **Đổi cửa sổ.** Cùng lý do.
//!
//! Nguyên tắc xuyên suốt: **khi nghi ngờ thì vứt buffer**. Buffer sai dẫn tới đề xuất
//! sai *và* thay nhầm chỗ — tệ hơn nhiều so với bỏ lỡ một từ.

/// Sự kiện phím đã chuẩn hoá, do [`crate::hook`] bơm vào.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// Một ký tự đi vào ô nhập.
    Char(char),
    Backspace,
    /// Phím kết thúc từ: space, Enter, Tab, dấu câu.
    WordBreak(char),
    /// Con trỏ có thể đã nhảy chỗ — mũi tên, Home/End, PageUp/Down, chuột.
    CaretMoved,
    /// Đổi focus sang cửa sổ khác.
    FocusChanged,
}

/// Nguồn của một phím.
///
/// # Bộ gõ tiếng Việt thực sự làm gì — đo trên UniKey, 2026-08-12
///
/// Giả thuyết ban đầu ("bộ gõ nuốt hết phím gốc, chỉ tin phím bơm") **sai**. Spike 5
/// trên 163 sự kiện thật cho thấy UniKey để **phần lớn** phím gốc đi qua bình thường,
/// và chỉ nuốt đúng **một** phím: phím kích hoạt ghép chữ. Gõ `nghanhf`:
///
/// ```text
/// n g h a n h        ← phím vật lý, đi thẳng vào ô nhập → "nghanh"
/// f                  ← phím vật lý, hook thấy nhưng bộ gõ NUỐT, app không nhận
/// BS BS BS           ← injected, xoá "anh"
/// à n h              ← injected qua VK_PACKET → "nghành"
/// ```
///
/// Nên mô hình đúng là: **tin cả hai luồng, trừ đi phím bị nuốt**. Phím bị nuốt nhận
/// ra được vì nó là phím vật lý cuối cùng ngay trước một loạt sự kiện injected —
/// UniKey phản hồi trong vòng 1 ms, và cửa sổ nhận diện từ 1 ms tới 60 ms đều cho
/// cùng kết quả, nên ranh giới này không mong manh.
///
/// [`crate::hook`] lo việc nhận diện đó và chèn một [`KeyEvent::Backspace`] bù vào
/// đúng chỗ, nên bộ đệm ở đây không cần biết gì về bộ gõ cả.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// User gõ trực tiếp.
    Physical,
    /// Do `SendInput` bơm — bộ gõ, hoặc chính Writa khi tự sửa.
    Injected,
}

/// Độ dài từ tối đa giữ trong buffer.
///
/// Âm tiết tiếng Việt dài nhất là 7 chữ (`nghiêng`, `nghiệng`). Cho gấp đôi để chứa
/// cả trạng thái trung gian của bộ gõ, rồi cắt — chuỗi dài hơn thế chắc chắn không
/// phải một âm tiết đang gõ.
const MAX_WORD_LEN: usize = 16;

/// Bộ đệm từ đang gõ.
#[derive(Debug, Default)]
pub struct WordBuffer {
    current: String,
    /// Bỏ qua phím do CHÍNH ta bơm khi tự sửa, nếu không sẽ tự phản hồi vòng lặp.
    suppress: usize,
}

impl WordBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Báo trước rằng ta sắp bơm `n` sự kiện phím khi tự sửa, để không tự nghe lại.
    pub fn expect_own_input(&mut self, n: usize) {
        self.suppress += n;
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn clear(&mut self) {
        self.current.clear();
    }

    /// Nhận một sự kiện phím. Trả về từ vừa hoàn thành, nếu có.
    ///
    /// "Hoàn thành" nghĩa là user vừa gõ space, dấu câu, hoặc Enter — tức từ đó đã
    /// xong và kiểm tra được. Chỉ khi đó mới đáng kiểm tra: kiểm tra giữa chừng thì
    /// mọi tiền tố đều là "sai chính tả".
    pub fn feed(&mut self, event: KeyEvent, source: KeySource) -> Option<String> {
        // Phím do chính ta bơm khi tự sửa — nuốt đi.
        if self.suppress > 0 && source == KeySource::Injected {
            self.suppress -= 1;
            return None;
        }

        match event {
            KeyEvent::Char(c) => {
                self.current.push(c);
                if self.current.chars().count() > MAX_WORD_LEN {
                    // Quá dài để là một âm tiết — bỏ, đừng giữ rác.
                    self.clear();
                }
                None
            }
            KeyEvent::Backspace => {
                self.current.pop();
                None
            }
            KeyEvent::WordBreak(_) => {
                if self.current.is_empty() {
                    return None;
                }
                let word = std::mem::take(&mut self.current);
                Some(word)
            }
            // Con trỏ nhảy chỗ hoặc đổi cửa sổ: buffer không còn ứng với text trên
            // màn hình. Vứt — đoán tiếp là cách thay nhầm chỗ.
            KeyEvent::CaretMoved | KeyEvent::FocusChanged => {
                self.clear();
                None
            }
        }
    }
}

/// Phím này có kết thúc một từ không?
pub fn is_word_break(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']' | '"' | '\''
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyEvent::*;
    use KeySource::*;

    fn type_word(buf: &mut WordBuffer, s: &str, src: KeySource) -> Option<String> {
        let mut last = None;
        for c in s.chars() {
            let ev = if is_word_break(c) {
                WordBreak(c)
            } else {
                Char(c)
            };
            last = buf.feed(ev, src);
        }
        last
    }

    #[test]
    fn emits_a_word_only_when_it_is_finished() {
        let mut b = WordBuffer::new();
        // Giữa chừng không được phát — mọi tiền tố đều trông như sai chính tả.
        assert_eq!(b.feed(Char('t'), Physical), None);
        assert_eq!(b.feed(Char('ô'), Physical), None);
        assert_eq!(b.feed(Char('i'), Physical), None);
        assert_eq!(b.current(), "tôi");
        assert_eq!(b.feed(WordBreak(' '), Physical), Some("tôi".to_string()));
        assert_eq!(b.current(), "");
    }

    #[test]
    fn backspace_rewinds() {
        let mut b = WordBuffer::new();
        type_word(&mut b, "tôii", Physical);
        b.feed(Backspace, Physical);
        assert_eq!(b.current(), "tôi");
        assert_eq!(b.feed(WordBreak(' '), Physical), Some("tôi".to_string()));
    }

    #[test]
    fn punctuation_ends_a_word() {
        let mut b = WordBuffer::new();
        assert_eq!(
            type_word(&mut b, "chào,", Physical),
            Some("chào".to_string())
        );
    }

    #[test]
    fn caret_movement_discards_the_buffer() {
        // Bất biến quan trọng nhất: buffer sai gây thay NHẦM CHỖ, tệ hơn bỏ lỡ từ.
        let mut b = WordBuffer::new();
        type_word(&mut b, "tiế", Physical);
        assert_eq!(b.feed(CaretMoved, Physical), None);
        assert_eq!(b.current(), "");
        // Và từ tiếp theo không được dính phần cũ
        assert_eq!(type_word(&mut b, "ng ", Physical), Some("ng".to_string()));
    }

    #[test]
    fn focus_change_discards_the_buffer() {
        let mut b = WordBuffer::new();
        type_word(&mut b, "abc", Physical);
        b.feed(FocusChanged, Physical);
        assert_eq!(b.current(), "");
    }

    #[test]
    fn reconstructs_a_real_unikey_composition() {
        // Chuỗi sự kiện THẬT do spike 5 ghi lại khi gõ `nghanhf` với UniKey Telex
        // (ime-probe.log, mốc 13117–13617ms). Đây là bằng chứng chứ không phải mô
        // hình tưởng tượng — bản thiết kế trước đó dựa trên giả thuyết "bộ gõ nuốt
        // hết phím gốc", và giả thuyết đó sai.
        //
        // `crate::hook` chèn Backspace bù ở vị trí phím `f` bị nuốt.
        let mut b = WordBuffer::new();
        for c in "nghanh".chars() {
            b.feed(Char(c), Physical);
        }
        b.feed(Char('f'), Physical); // hook thấy, nhưng bộ gõ nuốt — app không nhận
        b.feed(Backspace, Injected); // ← bù cho phím bị nuốt, do hook chèn
        for _ in 0..3 {
            b.feed(Backspace, Injected); // UniKey xoá "anh"
        }
        for c in "ành".chars() {
            b.feed(Char(c), Injected); // UniKey bơm qua VK_PACKET
        }

        assert_eq!(b.current(), "nghành");
        assert_eq!(b.feed(WordBreak(' '), Physical), Some("nghành".to_string()));
    }

    #[test]
    fn reconstructs_a_composition_that_fires_twice_in_one_word() {
        // `daudf` → `đàu`: bộ gõ ghép hai lần trong một từ (`dd`→`đ`, rồi thanh
        // huyền), nên có hai phím bị nuốt. Cũng lấy từ ime-probe.log.
        let mut b = WordBuffer::new();
        for c in "dau".chars() {
            b.feed(Char(c), Physical);
        }
        b.feed(Char('d'), Physical); // bị nuốt
        b.feed(Backspace, Injected); // bù
        for _ in 0..3 {
            b.feed(Backspace, Injected);
        }
        for c in "đau".chars() {
            b.feed(Char(c), Injected);
        }
        assert_eq!(b.current(), "đau");

        b.feed(Char('f'), Physical); // bị nuốt
        b.feed(Backspace, Injected); // bù
        for _ in 0..2 {
            b.feed(Backspace, Injected);
        }
        for c in "àu".chars() {
            b.feed(Char(c), Injected);
        }
        assert_eq!(b.current(), "đàu");
    }

    #[test]
    fn navigation_clears_the_buffer_mid_composition() {
        // Mũi tên và click chuột không đi qua bộ gõ. Chúng phải vứt buffer bất kể
        // đang ghép chữ dở hay không — nếu không, buffer sống sót qua một cú click
        // và Writa sẽ sửa nhầm chỗ.
        let mut b = WordBuffer::new();
        for c in "nghanh".chars() {
            b.feed(Char(c), Physical);
        }
        b.feed(CaretMoved, Physical);
        assert_eq!(b.current(), "");
    }

    #[test]
    fn our_own_corrections_do_not_feed_back() {
        // Khi Writa tự sửa, nó bơm Backspace + text. Nghe lại chính mình sẽ thành
        // vòng lặp phản hồi.
        let mut b = WordBuffer::new();
        b.expect_own_input(5);
        for c in "ngành".chars().take(5) {
            assert_eq!(b.feed(Char(c), Injected), None);
        }
        assert_eq!(b.current(), "", "không được nghe lại phím của chính mình");
    }

    #[test]
    fn overlong_input_is_dropped_not_kept() {
        let mut b = WordBuffer::new();
        type_word(&mut b, "abcdefghijklmnopqrstuvwxyz", Physical);
        assert!(
            b.current().chars().count() <= MAX_WORD_LEN,
            "buffer phình: {:?}",
            b.current()
        );
    }

    #[test]
    fn word_break_on_empty_buffer_emits_nothing() {
        let mut b = WordBuffer::new();
        assert_eq!(b.feed(WordBreak(' '), Physical), None);
        assert_eq!(b.feed(WordBreak(' '), Physical), None);
    }
}
