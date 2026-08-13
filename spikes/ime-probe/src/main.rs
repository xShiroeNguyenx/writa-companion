//! P0 Spike — IME coexistence probe.
//!
//! # Câu hỏi cần trả lời
//!
//! Writa Tier 2 (real-time) dựng word buffer từ `WH_KEYBOARD_LL`. Nhưng UniKey/EVKey
//! cũng dùng low-level hook: chúng **chặn** phím gốc rồi **inject** ký tự đã compose.
//! Nếu ta không phân biệt được hai luồng đó, word buffer sẽ ra rác.
//!
//! Spike này trả lời: khi user gõ `tieengs` và UniKey biến nó thành `tiếng`,
//! hook của ta thấy chuỗi event nào, và **chiến lược reconstruct nào khớp với
//! text thật**?
//!
//! # Cách chạy
//!
//! ```text
//! cargo run -p ime-probe --release -- 40
//! ```
//!
//! Rồi trong 40 giây đó: mở Notepad, **bật UniKey**, gõ vài từ tiếng Việt
//! (ví dụ `tieengs Vieejt`, `xin chaof`). Nhấn F12 để dừng sớm.
//! Cuối cùng chương trình in ra 6 cách reconstruct — so với text bạn gõ thật
//! để biết cách nào đúng.
//!
//! # Ghi chú kỹ thuật
//!
//! - `SendInput` với `KEYEVENTF_UNICODE` xuất hiện ở hook dưới dạng
//!   `vkCode == VK_PACKET (0xE7)`, và **`scanCode` chứa UTF-16 code unit**.
//!   Đây là đường mà ký tự tiếng Việt đã compose đi vào.
//! - Cờ `LLKHF_INJECTED (0x10)` bật cho mọi event sinh bởi `SendInput`.
//! - `dwExtraInfo` được log lại vì nhiều IME đặt "chữ ký" riêng ở đây để
//!   nhận ra event của chính mình — nếu UniKey làm vậy, ta có tín hiệu
//!   phân biệt đáng tin hơn cả cờ INJECTED.
//! - Hook callback phải chạy nhanh: Windows tự tháo hook nếu vượt
//!   `LowLevelHooksTimeout` (mặc định 300ms). Vì vậy callback chỉ push vào
//!   `Vec`, việc in ấn để cho thread khác làm.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Console::SetConsoleOutputCP;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

// ---------------------------------------------------------------------------
// Hằng số — tự khai báo thay vì import để giảm bề mặt API phụ thuộc phiên bản
// ---------------------------------------------------------------------------

const LLKHF_EXTENDED: u32 = 0x01;
const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_ALTDOWN: u32 = 0x20;
const LLKHF_UP: u32 = 0x80;

const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_SHIFT: u32 = 0x10;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_F12: u32 = 0x7B;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_OEM_1: u32 = 0xBA;
const VK_OEM_PLUS: u32 = 0xBB;
const VK_OEM_COMMA: u32 = 0xBC;
const VK_OEM_MINUS: u32 = 0xBD;
const VK_OEM_PERIOD: u32 = 0xBE;
const VK_OEM_2: u32 = 0xBF;
const VK_PACKET: u32 = 0xE7;

// ---------------------------------------------------------------------------
// State toàn cục
// ---------------------------------------------------------------------------

static EVENTS: Mutex<Vec<Ev>> = Mutex::new(Vec::new());
static START: OnceLock<Instant> = OnceLock::new();
static MAIN_TID: AtomicU32 = AtomicU32::new(0);
static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);
static STOPPING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct Ev {
    ms: u128,
    /// `true` cho WM_KEYDOWN / WM_SYSKEYDOWN
    down: bool,
    vk: u32,
    scan: u32,
    flags: u32,
    extra: usize,
    decoded: Decoded,
}

#[derive(Clone, PartialEq)]
enum Decoded {
    Char(char),
    Backspace,
    Other(String),
}

impl Ev {
    fn injected(&self) -> bool {
        self.flags & LLKHF_INJECTED != 0
    }

    fn flag_str(&self) -> String {
        let mut s = Vec::new();
        if self.flags & LLKHF_EXTENDED != 0 {
            s.push("EXTENDED");
        }
        if self.flags & LLKHF_LOWER_IL_INJECTED != 0 {
            s.push("LOWER_IL_INJECTED");
        }
        if self.flags & LLKHF_INJECTED != 0 {
            s.push("INJECTED");
        }
        if self.flags & LLKHF_ALTDOWN != 0 {
            s.push("ALTDOWN");
        }
        if self.flags & LLKHF_UP != 0 {
            s.push("UP");
        }
        if s.is_empty() {
            "-".to_string()
        } else {
            s.join("|")
        }
    }

    fn line(&self) -> String {
        let kind = if self.down { "DOWN" } else { "UP  " };
        let what = match &self.decoded {
            Decoded::Char(c) => format!("'{c}' (U+{:04X})", *c as u32),
            Decoded::Backspace => "<BACKSPACE>".to_string(),
            Decoded::Other(s) => format!("<{s}>"),
        };
        format!(
            "[{:>7}ms] {kind} vk=0x{:02X}({:>3}) scan=0x{:04X} extra=0x{:X} {:<28} {}",
            self.ms,
            self.vk,
            self.vk,
            self.scan,
            self.extra,
            self.flag_str(),
            what
        )
    }
}

// ---------------------------------------------------------------------------
// Decode vkCode -> ký tự
// ---------------------------------------------------------------------------

fn vk_name(vk: u32) -> String {
    match vk {
        VK_RETURN => "ENTER".into(),
        VK_TAB => "TAB".into(),
        VK_ESCAPE => "ESC".into(),
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => "SHIFT".into(),
        0x11 | 0xA2 | 0xA3 => "CTRL".into(),
        0x12 | 0xA4 | 0xA5 => "ALT".into(),
        0x14 => "CAPSLOCK".into(),
        0x25 => "LEFT".into(),
        0x26 => "UP".into(),
        0x27 => "RIGHT".into(),
        0x28 => "DOWN".into(),
        0x2E => "DELETE".into(),
        0x24 => "HOME".into(),
        0x23 => "END".into(),
        0x5B | 0x5C => "WIN".into(),
        _ => format!("VK_0x{vk:02X}"),
    }
}

fn decode(vk: u32, scan: u32, shift: bool) -> Decoded {
    match vk {
        // SendInput(KEYEVENTF_UNICODE): scanCode giữ UTF-16 code unit.
        // Đây là cửa mà ký tự tiếng Việt đã compose của IME đi vào.
        VK_PACKET => match char::from_u32(scan) {
            Some(c) => Decoded::Char(c),
            None => Decoded::Other(format!("PACKET U+{scan:04X}")),
        },
        VK_BACK => Decoded::Backspace,
        VK_SPACE => Decoded::Char(' '),
        0x41..=0x5A => {
            let base = vk as u8 as char; // 'A'..='Z'
            Decoded::Char(if shift {
                base
            } else {
                base.to_ascii_lowercase()
            })
        }
        0x30..=0x39 if !shift => Decoded::Char(vk as u8 as char),
        0x60..=0x69 => Decoded::Char((b'0' + (vk - 0x60) as u8) as char), // numpad
        VK_OEM_PERIOD => Decoded::Char(if shift { '>' } else { '.' }),
        VK_OEM_COMMA => Decoded::Char(if shift { '<' } else { ',' }),
        VK_OEM_1 => Decoded::Char(if shift { ':' } else { ';' }),
        VK_OEM_2 => Decoded::Char(if shift { '?' } else { '/' }),
        VK_OEM_MINUS => Decoded::Char(if shift { '_' } else { '-' }),
        VK_OEM_PLUS => Decoded::Char(if shift { '+' } else { '=' }),
        _ => Decoded::Other(vk_name(vk)),
    }
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // code < 0 => bắt buộc chuyển tiếp ngay, không được xử lý
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if down || up {
            let vk = kb.vkCode;

            // Tự theo dõi Shift từ chính luồng event — đáng tin hơn GetKeyState
            // trong hook callback (state hàng đợi của thread ta có thể cũ).
            if matches!(vk, VK_SHIFT | VK_LSHIFT | VK_RSHIFT) {
                SHIFT_DOWN.store(down, Ordering::Relaxed);
            }

            // F12 = dừng sớm
            if down && vk == VK_F12 && !STOPPING.swap(true, Ordering::SeqCst) {
                let tid = MAIN_TID.load(Ordering::SeqCst);
                if tid != 0 {
                    let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
                }
            }

            let start = START.get_or_init(Instant::now);
            let ev = Ev {
                ms: start.elapsed().as_millis(),
                down,
                vk,
                scan: kb.scanCode,
                flags: kb.flags.0,
                extra: kb.dwExtraInfo,
                decoded: decode(vk, kb.scanCode, SHIFT_DOWN.load(Ordering::Relaxed)),
            };

            // Chỉ push — mọi việc nặng để thread in ấn làm, giữ hook nhanh
            // để Windows không tháo hook vì quá LowLevelHooksTimeout.
            if let Ok(mut g) = EVENTS.lock() {
                g.push(ev);
            }
        }
    }

    // TUYỆT ĐỐI không chặn phím — probe chỉ quan sát
    CallNextHookEx(None, code, wparam, lparam)
}

// ---------------------------------------------------------------------------
// Reconstruct — phần trả lời câu hỏi của spike
// ---------------------------------------------------------------------------

/// Dựng lại text từ chuỗi event theo một bộ lọc, có/không xử lý backspace.
fn reconstruct(events: &[Ev], filter: fn(&Ev) -> bool, apply_backspace: bool) -> String {
    let mut out = String::new();
    for ev in events.iter().filter(|e| e.down && filter(e)) {
        match &ev.decoded {
            Decoded::Char(c) => out.push(*c),
            Decoded::Backspace => {
                if apply_backspace {
                    out.pop();
                }
            }
            Decoded::Other(_) => {}
        }
    }
    out
}

fn report(events: &[Ev], w: &mut impl Write) -> std::io::Result<()> {
    let downs: Vec<&Ev> = events.iter().filter(|e| e.down).collect();
    let n_inj = downs.iter().filter(|e| e.injected()).count();
    let n_pkt = downs.iter().filter(|e| e.vk == VK_PACKET).count();
    let n_bs = downs
        .iter()
        .filter(|e| e.decoded == Decoded::Backspace)
        .count();
    let n_bs_inj = downs
        .iter()
        .filter(|e| e.decoded == Decoded::Backspace && e.injected())
        .count();

    writeln!(w, "\n{}", "=".repeat(78))?;
    writeln!(w, "TỔNG KẾT")?;
    writeln!(w, "{}", "=".repeat(78))?;
    writeln!(w, "Tổng event (down+up)     : {}", events.len())?;
    writeln!(w, "Event keydown            : {}", downs.len())?;
    writeln!(w, "  ├─ KHÔNG injected      : {}", downs.len() - n_inj)?;
    writeln!(w, "  └─ INJECTED            : {n_inj}")?;
    writeln!(w, "VK_PACKET (Unicode inject): {n_pkt}")?;
    writeln!(
        w,
        "Backspace                : {n_bs} (injected: {n_bs_inj})"
    )?;

    // dwExtraInfo — nếu UniKey đặt chữ ký riêng, nó lộ ra ở đây
    let mut extras: Vec<(usize, usize)> = Vec::new();
    for ev in &downs {
        match extras.iter_mut().find(|(v, _)| *v == ev.extra) {
            Some((_, c)) => *c += 1,
            None => extras.push((ev.extra, 1)),
        }
    }
    extras.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    writeln!(w, "\ndwExtraInfo gặp được (chữ ký của IME nếu có):")?;
    for (v, c) in &extras {
        writeln!(w, "  0x{v:X}  ×{c}")?;
    }

    writeln!(w, "\n{}", "-".repeat(78))?;
    writeln!(w, "6 CHIẾN LƯỢC RECONSTRUCT — so với text bạn gõ THẬT")?;
    writeln!(w, "{}", "-".repeat(78))?;
    /// (tên hiển thị, bộ lọc event, có xử lý backspace)
    type Strategy = (&'static str, fn(&Ev) -> bool, bool);

    let strategies: [Strategy; 6] = [
        ("A  tất cả, bỏ qua backspace   ", |_| true, false),
        ("B  tất cả, xử lý backspace    ", |_| true, true),
        ("C  chỉ NON-injected, bỏ qua BS", |e| !e.injected(), false),
        ("D  chỉ NON-injected, xử lý BS ", |e| !e.injected(), true),
        ("E  chỉ INJECTED, bỏ qua BS    ", |e| e.injected(), false),
        ("F  chỉ INJECTED, xử lý BS     ", |e| e.injected(), true),
    ];
    for (name, f, bs) in strategies {
        writeln!(w, "{name} → {:?}", reconstruct(events, f, bs))?;
    }

    writeln!(w, "\n{}", "-".repeat(78))?;
    writeln!(w, "CÁCH ĐỌC KẾT QUẢ")?;
    writeln!(w, "{}", "-".repeat(78))?;
    writeln!(
        w,
        "• Nếu F (chỉ INJECTED + xử lý BS) khớp text thật\n  \
         → PLAN A khả thi: tin phím injected làm nguồn sự thật cho word buffer."
    )?;
    writeln!(
        w,
        "• Nếu VK_PACKET = 0 và không có event INJECTED nào\n  \
         → UniKey ghi text bằng đường khác (WM_CHAR/TSF), hook không thấy được\n  \
         → PLAN B: poll text từ UIA thay vì dựng buffer từ keystroke."
    )?;
    writeln!(
        w,
        "• Nếu không chiến lược nào khớp (chuỗi lẫn lộn cả hai luồng)\n  \
         → PLAN B hoặc PLAN C (chỉ làm Tier 1 hotkey)."
    )?;
    writeln!(
        w,
        "• Nếu dwExtraInfo có một giá trị khác 0 lặp lại đều\n  \
         → đó là chữ ký UniKey, dùng nó lọc chính xác hơn cờ INJECTED."
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = SetConsoleOutputCP(65001); // CP_UTF8 để in được tiếng Việt
    }

    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);

    println!("{}", "=".repeat(78));
    println!("Writa — P0 Spike: IME coexistence probe");
    println!("{}", "=".repeat(78));
    println!("Đang ghi trong {secs} giây. Nhấn F12 để dừng sớm.\n");
    println!("VIỆC CẦN LÀM NGAY:");
    println!("  1. Mở Notepad, đảm bảo UniKey/EVKey ĐANG BẬT (kiểu Telex).");
    println!("  2. Gõ vài từ tiếng Việt, ví dụ:  tieengs Vieejt  /  xin chaof");
    println!("  3. GHI LẠI chính xác text bạn thấy hiện ra trong Notepad");
    println!("     — để so với 6 chiến lược reconstruct ở cuối.\n");
    println!("{}", "-".repeat(78));

    START.get_or_init(Instant::now);
    MAIN_TID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

    let hook: HHOOK = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0)? };

    // Thread in ấn live — giữ hook callback nhẹ
    let printer = std::thread::spawn(|| {
        let mut cursor = 0usize;
        loop {
            std::thread::sleep(Duration::from_millis(120));
            let snapshot: Vec<Ev> = match EVENTS.lock() {
                Ok(g) => {
                    if g.len() <= cursor {
                        if STOPPING.load(Ordering::SeqCst) {
                            return;
                        }
                        continue;
                    }
                    let s = g[cursor..].to_vec();
                    cursor = g.len();
                    s
                }
                Err(_) => return,
            };
            for ev in snapshot {
                println!("{}", ev.line());
            }
            if STOPPING.load(Ordering::SeqCst) {
                return;
            }
        }
    });

    // Hẹn giờ dừng
    let tid = MAIN_TID.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        if !STOPPING.swap(true, Ordering::SeqCst) {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    });

    // Message loop — bắt buộc phải có để WH_KEYBOARD_LL nhận được event
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // Không có window nào, chỉ cần pump để hook chạy
    }

    STOPPING.store(true, Ordering::SeqCst);
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
    let _ = printer.join();

    let events = EVENTS.lock().map(|g| g.clone()).unwrap_or_default();

    // In ra console
    report(&events, &mut std::io::stdout())?;

    // Ghi log đầy đủ ra file (UTF-8, console legacy có thể không render nổi)
    let log_path = std::env::current_dir()?.join("ime-probe.log");
    {
        let mut f = BufWriter::new(File::create(&log_path)?);
        writeln!(f, "Writa ime-probe — {} event", events.len())?;
        writeln!(f, "{}", "-".repeat(78))?;
        for ev in &events {
            writeln!(f, "{}", ev.line())?;
        }
        report(&events, &mut f)?;
    }
    println!("\nLog đầy đủ: {}", log_path.display());

    Ok(())
}
