//! P0 spike 2/3/4 — đo tích hợp Windows trên app thật.
//!
//! Khác `ime-probe` (đo hành vi hook bàn phím), binary này đo **bốn khả năng** mà
//! P2 và P4 dựa vào, trên bất kỳ app nào đang focus:
//!
//! 1. Đọc ngữ cảnh app (tên exe, control đang focus, có phải ô mật khẩu)
//! 2. Đọc vùng chọn — UIA trước, clipboard sau
//! 3. Định vị caret — chuỗi bốn bậc
//! 4. Ghi text (chỉ khi có `--write`, vì nó thay đổi nội dung app đích)
//!
//! Cách chạy:
//!
//! ```text
//! cargo run -p writa-win --bin win-probe --release            # chỉ đọc
//! cargo run -p writa-win --bin win-probe --release -- --write # có ghi thử
//! ```
//!
//! Đếm ngược 5 giây để kịp chuyển sang app cần đo và bôi đen một đoạn.

use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use writa_win::{caret, context, selection, writer};

fn main() {
    let write_test = std::env::args().any(|a| a == "--write");

    println!("{}", "=".repeat(74));
    println!("Writa — P0 spike: đo tích hợp Windows");
    println!("{}", "=".repeat(74));
    println!();
    println!("VIỆC CẦN LÀM NGAY (có 5 giây):");
    println!("  1. Chuyển sang app muốn đo — Notepad, Word, Chrome, Zalo, Teams…");
    println!("  2. Bôi đen một đoạn text tiếng Việt");
    if write_test {
        println!("  3. --write đang BẬT: probe sẽ gõ thử vào app đó. Đừng dùng file quan trọng.");
    }
    println!();

    for i in (1..=5).rev() {
        print!("\r  {i}… ");
        let _ = std::io::stdout().flush();
        sleep(Duration::from_secs(1));
    }
    println!("\r         ");

    // --- 1. Ngữ cảnh app -----------------------------------------------------
    println!("{}", "-".repeat(74));
    println!("1. NGỮ CẢNH APP");
    println!("{}", "-".repeat(74));

    let ctx = match context::current() {
        Ok(c) => c,
        Err(e) => {
            println!("  ❌ {e}");
            return;
        }
    };
    println!("  exe                : {}", ctx.exe);
    println!("  tiêu đề cửa sổ     : {}", truncate(&ctx.title, 50));
    println!(
        "  control đang focus : {}",
        if ctx.focused_control.is_some() {
            "✅ xác định được"
        } else {
            "⚠️  không xác định được (app tự vẽ?)"
        }
    );
    println!(
        "  ô mật khẩu (Win32) : {}",
        if ctx.is_password_field {
            "CÓ"
        } else {
            "không"
        }
    );
    let uia_password = selection::is_password_element();
    println!(
        "  ô mật khẩu (UIA)   : {}",
        if uia_password {
            "CÓ (hoặc không hỏi được UIA)"
        } else {
            "không"
        }
    );
    println!(
        "  → Writa {} hoạt động ở đây",
        if ctx.is_safe_to_assist() && !uia_password {
            "ĐƯỢC PHÉP"
        } else {
            "KHÔNG được phép"
        }
    );

    // --- 2. Đọc vùng chọn ----------------------------------------------------
    println!();
    println!("{}", "-".repeat(74));
    println!("2. ĐỌC VÙNG CHỌN");
    println!("{}", "-".repeat(74));

    match selection::read_selection_uia() {
        Ok(text) => println!("  UIA       : ✅ {:?}", truncate(&text, 60)),
        Err(e) => println!("  UIA       : ❌ {e}"),
    }
    match selection::read_selection_clipboard() {
        Ok(text) => println!("  clipboard : ✅ {:?}", truncate(&text, 60)),
        Err(e) => println!("  clipboard : ❌ {e}"),
    }

    // --- 3. Định vị caret ----------------------------------------------------
    println!();
    println!("{}", "-".repeat(74));
    println!("3. ĐỊNH VỊ CARET");
    println!("{}", "-".repeat(74));

    let pos = caret::locate(&ctx);
    println!(
        "  ({}, {})  cao {}px  qua {:?}",
        pos.x, pos.y, pos.height, pos.source
    );
    println!(
        "  → {}",
        if pos.source.is_exact() {
            "caret THẬT — overlay neo sát dòng đang gõ được"
        } else {
            "chỉ là phỏng đoán theo chuột — overlay nên lùi ra để không che text"
        }
    );

    // --- 4. Ghi text ---------------------------------------------------------
    println!();
    println!("{}", "-".repeat(74));
    println!("4. GHI TEXT");
    println!("{}", "-".repeat(74));

    if !write_test {
        println!("  (bỏ qua — chạy lại với --write để đo)");
    } else {
        // Chuỗi thử cố ý gồm chữ tiếng Việt khó: dấu mũ, móc, và thanh chồng dấu.
        const PROBE: &str = "tiếng Việt ưở đẫ";
        println!("  gõ thử: {PROBE:?}");
        match writer::type_text(PROBE) {
            Ok(()) => println!("  SendInput : ✅ đã gửi — kiểm tra app xem chữ có ĐÚNG DẤU không"),
            Err(e) => println!("  SendInput : ❌ {e}"),
        }
        sleep(Duration::from_millis(300));
        match writer::backspace(PROBE.chars().count()) {
            Ok(()) => println!("  dọn dẹp   : ✅ đã xoá lại"),
            Err(e) => println!("  dọn dẹp   : ❌ {e} — có thể còn sót chữ trong app"),
        }
    }

    println!();
    println!("{}", "=".repeat(74));
    println!("Chép kết quả vào bảng compatibility matrix trong SPIKE_RESULTS.md.");
    println!("Chạy lại với từng app trong danh sách để điền đủ bảng.");
}

fn truncate(s: &str, max: usize) -> String {
    let clean = s.replace(['\n', '\r'], " ");
    if clean.chars().count() <= max {
        return clean;
    }
    clean.chars().take(max).collect::<String>() + "…"
}
