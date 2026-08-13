//! Kiểm chứng **hạ tầng** hook — không phải câu hỏi về bộ gõ.
//!
//! `ime-probe` trả lời câu hỏi khó (bộ gõ ghép chữ thì hook thấy gì) và cần người
//! ngồi gõ. Probe này trả lời câu hỏi dễ nhưng phải đúng trước đã:
//!
//! - Hook có cài được và có nhận sự kiện không? (thiếu message loop là hook im lặng
//!   không chạy, không báo lỗi)
//! - Cờ `LLKHF_INJECTED` có đọc đúng không?
//! - Ký tự Unicode bơm qua `VK_PACKET` có giải mã đúng không? Đây chính là đường mà
//!   `tiếng` của UniKey đi vào — nếu đường này sai thì Tier 2 mù với tiếng Việt.
//! - Bộ đệm từ có ghép lại đúng chuỗi đã bơm không?
//!
//! Nó tự bơm phím bằng `SendInput` rồi so những gì hook nhìn thấy với những gì đã
//! gửi. Vì `SendInput` cũng đặt cờ `INJECTED`, phím của chính ta trông y hệt phím
//! của bộ gõ — điều đó **tốt** cho phép đo này: nó kiểm đúng đường mà bộ gõ dùng.
//!
//! ```text
//! cargo run -p writa-win --bin hook-probe --release
//! ```
//!
//! ⚠️ Probe **bơm phím thật** vào cửa sổ đang focus. Hãy focus vào một ô nhập bỏ đi
//! (Notepad trống) trước khi chạy.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use writa_win::buffer::{KeyEvent, KeySource, WordBuffer};
use writa_win::hook::{self, HookEvent};
use writa_win::writer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = windows::Win32::System::Console::SetConsoleOutputCP(65001);
    }

    println!("{}", "=".repeat(70));
    println!("Writa — kiểm chứng hạ tầng hook");
    println!("{}", "=".repeat(70));
    println!("⚠️  Probe sẽ GÕ THẬT vào cửa sổ đang focus.");
    println!("    Hãy chuyển sang một ô nhập bỏ đi (Notepad trống). Còn 4 giây…\n");
    std::thread::sleep(Duration::from_secs(4));

    let seen: Arc<Mutex<Vec<HookEvent>>> = Arc::default();
    let sink = seen.clone();
    hook::start(move |ev| {
        if let Ok(mut g) = sink.lock() {
            g.push(ev);
        }
    })?;
    println!("Hook đã cài. Bắt đầu bơm phím…\n");
    std::thread::sleep(Duration::from_millis(300));

    // Cố ý có dấu tiếng Việt: đó là đường VK_PACKET, đường mà bộ gõ dùng.
    const SENT: &str = "tiếng Việt ";
    writer::type_text(SENT)?;
    std::thread::sleep(Duration::from_millis(400));

    // Backspace — bộ gõ dùng rất nhiều khi ghép chữ.
    writer::backspace(1)?;
    std::thread::sleep(Duration::from_millis(400));

    hook::stop();
    std::thread::sleep(Duration::from_millis(200));

    let events = seen.lock().unwrap().clone();

    println!("{}", "-".repeat(70));
    println!("Đã gửi   : {SENT:?} rồi 1 backspace");
    println!("Hook thấy: {} sự kiện", events.len());
    println!("{}", "-".repeat(70));
    // Probe này chỉ quan tâm sự kiện phím; `AcceptRequested` không xảy ra vì ta không
    // bật chặn Tab.
    let keys: Vec<(KeyEvent, KeySource, bool)> = events
        .iter()
        .filter_map(|e| match *e {
            HookEvent::Key {
                event,
                source,
                via_packet,
            } => Some((event, source, via_packet)),
            HookEvent::AcceptRequested => None,
        })
        .collect();
    for (event, source, packet) in &keys {
        println!("  {event:?}  {source:?}  packet={packet}");
    }

    let injected = keys
        .iter()
        .filter(|(_, s, _)| *s == KeySource::Injected)
        .count();
    let packets = keys.iter().filter(|(_, _, p)| *p).count();

    // Dựng lại bằng đúng bộ đệm mà Tier 2 sẽ dùng.
    let mut buf = WordBuffer::new();
    let mut words = Vec::new();
    for (event, source, _) in &keys {
        if let Some(w) = buf.feed(*event, *source) {
            words.push(w);
        }
    }

    println!("\n{}", "=".repeat(70));
    println!("KẾT LUẬN");
    println!("{}", "=".repeat(70));
    check("Hook nhận được sự kiện", !keys.is_empty());
    check("Cờ INJECTED đọc đúng", injected > 0);
    check("VK_PACKET giải mã đúng", packets > 0);
    check(
        "Ký tự tiếng Việt có dấu qua được",
        keys.iter()
            .any(|(e, _, _)| matches!(e, KeyEvent::Char(c) if !c.is_ascii())),
    );
    check(
        "Backspace nhận diện được",
        keys.iter().any(|(e, _, _)| *e == KeyEvent::Backspace),
    );
    println!("\nTừ bộ đệm ghép được: {words:?}   (mong đợi [\"tiếng\", \"Việt\"])");

    probe_release_modifiers();
    probe_latency();

    if events.is_empty() {
        println!("\n⚠️  Không sự kiện nào. Hai khả năng:");
        println!("   • SendInput bị chặn (UIPI) — chạy ở phiên làm việc tương tác bình thường");
        println!("   • Hook không cài được — kiểm quyền và phần mềm bảo mật");
    }
    Ok(())
}

fn check(what: &str, ok: bool) {
    println!("  {} {what}", if ok { "✅" } else { "❌" });
}

/// Đo độ trễ của **một từ hoàn thành** trong Tier 2.
///
/// PLAN.md đặt mốc p99 < 5 ms mỗi từ. Nhưng mốc đó chỉ tính phần engine, còn đường thật
/// của Tier 2 gọi thêm ba thứ Win32 trước khi engine chạy — và UIA nổi tiếng chậm. Nếu
/// tổng vượt nhịp gõ thì sự kiện dồn lại và gợi ý đến sau khi user đã gõ xong câu.
fn probe_latency() {
    use std::time::Instant;
    use writa_win::{caret, context};

    println!("\n{}", "=".repeat(70));
    println!("ĐỘ TRỄ mỗi từ hoàn thành (mốc PLAN.md: p99 < 5 ms)");
    println!("{}", "=".repeat(70));

    const ROUNDS: usize = 30;
    // Cụm 5 từ — đúng cỡ ngữ cảnh Tier 2 giữ lại, và có một lỗi real-word để buộc
    // engine đi hết đường sinh candidate + chấm điểm mô hình ngôn ngữ.
    const CONTEXT: &str = "nay tôi sữa lỗi chính";

    let mut ctx_ms = Vec::new();
    let mut pwd_ms = Vec::new();
    let mut caret_ms = Vec::new();
    let mut engine_ms = Vec::new();

    // Vòng khởi động: lần đầu phải nạp từ điển và dựng chỉ mục.
    let _ = writa_core::check(CONTEXT);

    for _ in 0..ROUNDS {
        let t = Instant::now();
        let ctx = context::current();
        ctx_ms.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        let _ = writa_win::selection::is_password_element();
        pwd_ms.push(t.elapsed().as_secs_f64() * 1000.0);

        if let Ok(ctx) = &ctx {
            let t = Instant::now();
            let _ = caret::locate(ctx);
            caret_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }

        let t = Instant::now();
        let _ = writa_core::check(CONTEXT);
        engine_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    let stat = |name: &str, mut v: Vec<f64>| {
        if v.is_empty() {
            println!("  {name:<34} (không đo được)");
            return 0.0;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = v[v.len() / 2];
        let p99 = v[(v.len() * 99 / 100).min(v.len() - 1)];
        println!("  {name:<34} p50 {p50:>7.2} ms   p99 {p99:>7.2} ms");
        p99
    };

    let a = stat("context::current (Win32)", ctx_ms);
    let b = stat("is_password_element (UIA)", pwd_ms);
    let c = stat("caret::locate (chỉ khi có gợi ý)", caret_ms);
    let d = stat("check_with (engine, 5 từ)", engine_ms);

    println!();
    println!("  Mỗi từ hoàn thành đi qua: context + is_password + engine");
    println!("    → p99 ước lượng {:>7.2} ms", a + b + d);
    println!("  Khi CÓ gợi ý thì thêm caret::locate");
    println!("    → p99 ước lượng {:>7.2} ms", a + b + c + d);
    check("Engine dưới mốc 5 ms", d < 5.0);
    check(
        "Cả đường dưới 50 ms (nhịp gõ nhanh nhất)",
        a + b + c + d < 50.0,
    );
}

/// Kiểm `writer::release_modifiers`.
///
/// Đây là bản sửa cho một lỗi cụ thể: phím tắt bắn lúc user **đang giữ** `Ctrl+Alt`,
/// nên `SendInput(VK_BACK)` ngay sau đó tới app đích dưới dạng `Ctrl+Alt+Backspace` —
/// tổ hợp mà hầu hết app bỏ qua. Phần xoá không xảy ra, phần gõ thì có, và bản sửa
/// **mọc thành từ mới** thay vì thay từ cũ.
///
/// Phép đo này không gõ chữ vào đâu cả: nó chỉ nhấn giữ phím phụ rồi kiểm tra xem
/// `release_modifiers` có thật sự nhả được chúng không.
fn probe_release_modifiers() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU};

    println!("\n{}", "=".repeat(70));
    println!("KIỂM release_modifiers — lỗi \"sửa thành từ mới\"");
    println!("{}", "=".repeat(70));

    writer::hold_for_test(&[VK_CONTROL, VK_MENU]);
    std::thread::sleep(Duration::from_millis(60));
    let ctrl_before = writer::is_key_down(VK_CONTROL.0);
    let alt_before = writer::is_key_down(VK_MENU.0);
    println!("  đang giữ:  Ctrl={ctrl_before}  Alt={alt_before}");

    writer::release_modifiers();
    std::thread::sleep(Duration::from_millis(60));
    let ctrl_after = writer::is_key_down(VK_CONTROL.0);
    let alt_after = writer::is_key_down(VK_MENU.0);
    println!("  sau khi nhả: Ctrl={ctrl_after}  Alt={alt_after}");

    check(
        "Giữ được phím phụ để dựng lại tình huống lỗi",
        ctrl_before && alt_before,
    );
    check(
        "release_modifiers nhả được cả hai",
        !ctrl_after && !alt_after,
    );
}
