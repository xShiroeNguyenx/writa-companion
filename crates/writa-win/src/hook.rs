//! P4 — Hook bàn phím / chuột / đổi cửa sổ.
//!
//! Đây là **nguồn sự kiện** cho Tier 2. Nó không quyết định gì cả: chỉ chuẩn hoá
//! dòng sự kiện thô của Windows thành [`KeyEvent`] rồi đẩy sang [`crate::buffer`],
//! nơi có thuật toán và có test.
//!
//! # Ba ràng buộc của Windows định hình module này
//!
//! 1. **Hook phải có message loop.** `WH_KEYBOARD_LL` gọi callback bằng cách bơm
//!    message vào thread đã cài hook. Không pump message thì hook im lặng không
//!    hoạt động — không lỗi, không cảnh báo. Vì vậy cả ba hook sống trên **một
//!    thread riêng** có vòng lặp message của chính nó.
//!
//! 2. **Callback phải trả về nhanh.** Windows tự tháo hook nếu callback vượt
//!    `LowLevelHooksTimeout` (mặc định 300 ms), và nó tháo *âm thầm*. Nên callback
//!    ở đây chỉ giải mã vài trường rồi `send` qua channel; mọi việc nặng — tra từ
//!    điển, chấm điểm mô hình ngôn ngữ, vẽ overlay — làm ở thread khác.
//!
//! 3. **Callback không bắt được biến ngoài.** Chữ ký `extern "system"` bắt buộc là
//!    con trỏ hàm trần. Nên kênh gửi phải là biến toàn cục.
//!
//! # Vì sao hook cả chuột và đổi cửa sổ
//!
//! [`crate::buffer`] phải vứt bộ đệm khi con trỏ nhảy chỗ, nếu không Writa sẽ sửa
//! **nhầm chỗ** — tệ hơn nhiều so với bỏ lỡ một từ. Bàn phím cho biết mũi tên và
//! Home/End, nhưng cú click chuột thì chỉ `WH_MOUSE_LL` thấy, còn chuyển app thì chỉ
//! `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` thấy. Thiếu một trong ba là bộ đệm sống
//! sót qua đúng những lúc nó phải chết.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

use crate::buffer::{is_word_break, KeyEvent, KeySource};
use crate::{WinError, WinResult};

/// `EVENT_SYSTEM_FOREGROUND` — cửa sổ foreground vừa đổi.
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
/// `WINEVENT_OUTOFCONTEXT` — nhận callback ở process của ta, không nhúng DLL vào
/// process khác. Chậm hơn một chút, đổi lại không phải ship DLL và không bị
/// antivirus coi là tiêm mã.
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

const LLKHF_INJECTED: u32 = 0x10;

const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_SHIFT: u32 = 0x10;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_DELETE: u32 = 0x2E;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
/// `SendInput(KEYEVENTF_UNICODE)` xuất hiện ở hook với `vkCode == VK_PACKET`, và
/// **`scanCode` chứa mã UTF-16**. Đây là cửa mà ký tự đã ghép của bộ gõ đi vào.
const VK_PACKET: u32 = 0xE7;

/// Một sự kiện đã chuẩn hoá, kèm đủ thông tin để lớp trên quyết định.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    Key {
        event: KeyEvent,
        source: KeySource,
        /// Ký tự đến qua `VK_PACKET`.
        ///
        /// Đây là **bằng chứng trực tiếp** có bộ gõ đang ghép chữ, đáng tin hơn nhiều
        /// so với việc dò tên tiến trình `unikey.exe` / `evkey.exe`: nó đúng với mọi
        /// bộ gõ, kể cả bộ gõ chưa ai biết tên, và tự tắt khi user tắt bộ gõ.
        via_packet: bool,
    },
    /// User bấm Tab trong lúc gợi ý đang hiện, và ta đã **chặn** phím đó.
    ///
    /// Đây là ngoại lệ duy nhất của nguyên tắc "không bao giờ chặn phím" — xem
    /// [`set_swallow_tab`].
    AcceptRequested,
}

/// Hàm nhận sự kiện, do lớp trên cung cấp.
type Sink = Box<dyn Fn(HookEvent) + Send + 'static>;

/// Khoảng thời gian tối đa giữa phím vật lý và loạt sự kiện bơm để coi phím đó là
/// **bị bộ gõ nuốt**.
///
/// Đo trên UniKey (spike 5, 163 sự kiện thật): bộ gõ phản hồi trong vòng **1 ms** —
/// nó nằm ngay trong chuỗi hook nên buộc phải đồng bộ. Thử lại với cửa sổ 1, 2, 5,
/// 20 và 60 ms đều cho **cùng một** kết quả khớp tuyệt đối, nên ranh giới này không
/// mong manh; 20 ms là chỗ rộng rãi gấp hai chục lần độ trễ đo được mà vẫn hẹp hơn
/// nhịp gõ nhanh nhất của người (~60 ms/ký tự ở 200 từ/phút).
const SWALLOW_WINDOW_MS: u32 = 20;

static TX: OnceLock<Sender<HookEvent>> = OnceLock::new();
static SINK: Mutex<Option<Sink>> = Mutex::new(None);
static HOOK_TID: AtomicU32 = AtomicU32::new(0);
static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static MUTED: AtomicBool = AtomicBool::new(false);
static SWALLOW_TAB: AtomicBool = AtomicBool::new(false);
/// Ta đã chặn keydown của Tab, nên phải chặn cả keyup tương ứng.
static TAB_SWALLOWED: AtomicBool = AtomicBool::new(false);
/// Mốc thời gian của phím vật lý sinh-ký-tự gần nhất, `0` nếu phím gần nhất không
/// phải loại đó.
static LAST_PHYSICAL_MS: AtomicU32 = AtomicU32::new(0);

/// Bắt đầu nghe bàn phím. `sink` được gọi trên một thread riêng, không phải trong
/// hook — nên nó được phép chậm.
///
/// Gọi lại khi đang chạy là no-op.
pub fn start<F>(sink: F) -> WinResult<()>
where
    F: Fn(HookEvent) + Send + 'static,
{
    *SINK.lock().unwrap() = Some(Box::new(sink));

    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    // Kênh và thread tiêu thụ chỉ dựng một lần cho cả vòng đời tiến trình. Việc
    // bật/tắt thật sự nằm ở chỗ cài/tháo hook, không ở đây.
    if TX.get().is_none() {
        let (tx, rx) = channel::<HookEvent>();
        let _ = TX.set(tx);
        std::thread::spawn(move || {
            for ev in rx {
                if let Ok(guard) = SINK.lock() {
                    if let Some(f) = guard.as_ref() {
                        f(ev);
                    }
                }
            }
        });
    }

    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    std::thread::spawn(move || {
        HOOK_TID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

        let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) };
        let Ok(keyboard) = keyboard else {
            let _ = ready_tx.send(Err("SetWindowsHookExW(WH_KEYBOARD_LL)".into()));
            RUNNING.store(false, Ordering::SeqCst);
            return;
        };
        // Chuột và đổi cửa sổ chỉ để **vứt bộ đệm**; hỏng thì mất độ chính xác chứ
        // không mất tính năng, nên không đáng để cả hook bàn phím thất bại theo.
        let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) }.ok();
        let foreground = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };

        let _ = ready_tx.send(Ok(()));

        // Vòng lặp message: BẮT BUỘC, nếu không hook không bao giờ được gọi.
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {}

        unsafe {
            let _ = UnhookWindowsHookEx(keyboard);
            if let Some(m) = mouse {
                let _ = UnhookWindowsHookEx(m);
            }
            if !foreground.is_invalid() {
                let _ = UnhookWinEvent(foreground);
            }
        }
        HOOK_TID.store(0, Ordering::SeqCst);
        RUNNING.store(false, Ordering::SeqCst);
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(call)) => Err(WinError::Api {
            call: Box::leak(call.into_boxed_str()),
            code: 0,
        }),
        Err(_) => Err(WinError::Api {
            call: "hook thread",
            code: 0,
        }),
    }
}

/// Ngừng nghe và **tháo hook thật sự**.
///
/// Tháo chứ không phải bỏ qua sự kiện: một công cụ có hình dạng keylogger mà "tạm
/// dừng" vẫn còn cắm hook thì lời hứa tạm dừng đó không kiểm chứng được.
pub fn stop() {
    let tid = HOOK_TID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

/// Bịt tai trong lúc **chính ta** đang bơm phím.
///
/// Writa sửa lỗi bằng `SendInput`, và phím do `SendInput` bơm mang đúng cờ
/// `INJECTED` như phím của bộ gõ. Không bịt thì hai chuyện xảy ra cùng lúc: bộ đệm
/// nghe lại chính mình (vòng lặp phản hồi), và cơ chế bù-phím-bị-nuốt tưởng phím
/// cuối của user vừa bị nuốt nên xoá oan một ký tự.
///
/// Bịt hẳn ở tầng hook đơn giản hơn nhiều so với đếm số sự kiện cần bỏ qua ở tầng
/// trên — đếm thì phải khớp chính xác, mà số backspace lại phụ thuộc độ dài chuỗi
/// thay thế.
/// Cho phép dùng **Tab** làm phím nhận gợi ý, bằng cách chặn nó.
///
/// # Ngoại lệ có chủ ý của một nguyên tắc
///
/// Module này không chặn phím: một hook nuốt phím mà lỗi thì làm hỏng việc gõ của cả
/// máy, và cái giá đó quá lớn so với bất cứ tiện lợi nào. Tab là ngoại lệ duy nhất,
/// và nó có ba lớp thu hẹp:
///
/// 1. Chỉ chặn khi **đang có gợi ý hiện** — tức chỉ trong khoảnh khắc user vừa gõ
///    xong một từ sai. Không có gợi ý thì Tab đi qua bình thường, y như trước.
/// 2. Chỉ chặn phím **vật lý**; Tab do `SendInput` bơm vẫn đi qua.
/// 3. Chặn cả keydown lẫn keyup của cùng lần bấm, để app đích không nhận được một
///    keyup lẻ không có keydown đi trước.
///
/// Lý do đánh đổi: `Ctrl+Alt+Space` đúng nhưng phải rời tay khỏi vị trí gõ; Tab thì
/// nằm ngay đó. Với một tính năng dùng vài chục lần mỗi giờ, khác biệt đó là thật.
pub fn set_swallow_tab(on: bool) {
    SWALLOW_TAB.store(on, Ordering::SeqCst);
}

pub fn set_muted(muted: bool) {
    MUTED.store(muted, Ordering::SeqCst);
    if !muted {
        // Vào lại từ trạng thái bịt: không có "phím vật lý trước đó" nào đáng tin.
        LAST_PHYSICAL_MS.store(0, Ordering::Relaxed);
    }
}

fn emit(event: KeyEvent, source: KeySource, via_packet: bool) {
    send_event(HookEvent::Key {
        event,
        source,
        via_packet,
    });
}

fn send_event(ev: HookEvent) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(ev);
    }
}

/// Chuyển một phím thành sự kiện của bộ đệm.
///
/// Trả `None` cho phím không ảnh hưởng gì tới bộ đệm (Ctrl, Alt, F1…). Bỏ qua yên
/// lặng là đúng ở đây: coi mọi phím lạ là "con trỏ có thể đã nhảy" sẽ vứt bộ đệm mỗi
/// lần user chạm Shift.
fn classify(vk: u32, scan: u32, shift: bool) -> Option<KeyEvent> {
    match vk {
        VK_PACKET => char::from_u32(scan).map(|c| {
            if is_word_break(c) {
                KeyEvent::WordBreak(c)
            } else {
                KeyEvent::Char(c)
            }
        }),
        VK_BACK => Some(KeyEvent::Backspace),
        VK_SPACE => Some(KeyEvent::WordBreak(' ')),
        VK_RETURN => Some(KeyEvent::WordBreak('\n')),
        VK_TAB => Some(KeyEvent::WordBreak('\t')),
        // Con trỏ nhảy chỗ → bộ đệm không còn ứng với text trên màn hình.
        VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT
        | VK_DELETE | VK_ESCAPE => Some(KeyEvent::CaretMoved),
        0x41..=0x5A => {
            let base = vk as u8 as char;
            Some(KeyEvent::Char(if shift {
                base
            } else {
                base.to_ascii_lowercase()
            }))
        }
        0x30..=0x39 if !shift => Some(KeyEvent::Char(vk as u8 as char)),
        0xBA => Some(KeyEvent::WordBreak(if shift { ':' } else { ';' })),
        0xBC => Some(KeyEvent::WordBreak(if shift { '<' } else { ',' })),
        0xBE => Some(KeyEvent::WordBreak(if shift { '>' } else { '.' })),
        0xBF => Some(KeyEvent::WordBreak(if shift { '?' } else { '/' })),
        0xBD => Some(KeyEvent::Char(if shift { '_' } else { '-' })),
        _ => None,
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && !MUTED.load(Ordering::Relaxed) {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let vk = kb.vkCode;

        // Theo dõi Shift từ chính dòng sự kiện. `GetKeyState` trong hook đọc trạng
        // thái hàng đợi của thread ta, vốn có thể cũ hơn phím đang xử lý.
        if matches!(vk, VK_SHIFT | VK_LSHIFT | VK_RSHIFT) {
            SHIFT_DOWN.store(down, Ordering::Relaxed);
        }

        let injected_now = kb.flags.0 & LLKHF_INJECTED != 0;

        // Tab làm phím nhận gợi ý — ngoại lệ duy nhất được chặn phím.
        // Xem [`set_swallow_tab`].
        if vk == VK_TAB && !injected_now {
            if down && SWALLOW_TAB.load(Ordering::Relaxed) {
                TAB_SWALLOWED.store(true, Ordering::Relaxed);
                send_event(HookEvent::AcceptRequested);
                return LRESULT(1);
            }
            // Keyup của đúng lần bấm đã chặn: chặn luôn, đừng để app nhận một keyup
            // không có keydown đi trước.
            if !down && TAB_SWALLOWED.swap(false, Ordering::Relaxed) {
                return LRESULT(1);
            }
        }

        if down {
            let injected = injected_now;
            let source = if injected {
                KeySource::Injected
            } else {
                KeySource::Physical
            };
            let event = classify(vk, kb.scanCode, SHIFT_DOWN.load(Ordering::Relaxed));

            // Bù cho phím bị bộ gõ nuốt.
            //
            // UniKey để phím thường đi thẳng vào ô nhập, nhưng **nuốt** phím kích
            // hoạt ghép chữ (`f` trong `nghanhf`) rồi bơm backspace + phần đã ghép.
            // Hook của ta thấy phím đó vì nó chạy trước trong chuỗi hook, còn ô nhập
            // thì không bao giờ nhận. Thiếu bước bù này, bộ đệm thừa đúng một ký tự
            // cho mỗi lần ghép: `coông` thay vì `công`.
            //
            // Xem [`SWALLOW_WINDOW_MS`] về con số, và spike 5 trong SPIKE_RESULTS.md
            // về phép đo.
            if injected {
                let last = LAST_PHYSICAL_MS.swap(0, Ordering::Relaxed);
                if last != 0 && kb.time.wrapping_sub(last) <= SWALLOW_WINDOW_MS {
                    emit(KeyEvent::Backspace, KeySource::Injected, false);
                }
            } else {
                // Chỉ phím sinh ký tự mới bị nuốt được — phím điều hướng thì không.
                let produces_char = matches!(event, Some(KeyEvent::Char(_)));
                LAST_PHYSICAL_MS.store(
                    if produces_char { kb.time.max(1) } else { 0 },
                    Ordering::Relaxed,
                );
            }

            if let Some(event) = event {
                emit(event, source, vk == VK_PACKET);
            }
        }
    }
    // TUYỆT ĐỐI không chặn phím. Writa quan sát, không đứng chắn giữa user và app —
    // một hook nuốt phím mà lỗi thì làm hỏng việc gõ của cả máy.
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && !MUTED.load(Ordering::Relaxed) {
        let msg = wparam.0 as u32;
        if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            // `FocusChanged` chứ không phải `CaretMoved`, dù với bộ đệm hai cái giống
            // nhau (đều vứt sạch). Khác biệt nằm ở lớp trên: một cú click đổi được
            // **phần tử** đang focus mà không đổi cửa sổ — bấm vào ô mật khẩu trong
            // Chrome chẳng hạn — và `EVENT_SYSTEM_FOREGROUND` không bắn trong trường
            // hợp đó. Báo là đổi focus buộc lớp trên tính lại quyền.
            emit(KeyEvent::FocusChanged, KeySource::Physical, false);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    emit(KeyEvent::FocusChanged, KeySource::Physical, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_respect_shift() {
        assert_eq!(classify(0x41, 0, false), Some(KeyEvent::Char('a')));
        assert_eq!(classify(0x41, 0, true), Some(KeyEvent::Char('A')));
    }

    #[test]
    fn packet_carries_the_composed_character_in_the_scan_code() {
        // Đây là đường mà `tiếng` của UniKey đi vào. Đọc nhầm trường này thì Tier 2
        // mù hoàn toàn với tiếng Việt có dấu.
        assert_eq!(
            classify(VK_PACKET, 'ế' as u32, false),
            Some(KeyEvent::Char('ế'))
        );
        // Dấu câu bơm qua packet vẫn phải kết thúc từ.
        assert_eq!(
            classify(VK_PACKET, ',' as u32, false),
            Some(KeyEvent::WordBreak(','))
        );
    }

    #[test]
    fn navigation_keys_invalidate_the_buffer() {
        for vk in [VK_LEFT, VK_RIGHT, VK_HOME, VK_END, VK_DELETE, VK_ESCAPE] {
            assert_eq!(
                classify(vk, 0, false),
                Some(KeyEvent::CaretMoved),
                "vk={vk:#x}"
            );
        }
    }

    #[test]
    fn space_and_punctuation_end_a_word() {
        assert_eq!(classify(VK_SPACE, 0, false), Some(KeyEvent::WordBreak(' ')));
        assert_eq!(
            classify(VK_RETURN, 0, false),
            Some(KeyEvent::WordBreak('\n'))
        );
        assert_eq!(classify(0xBE, 0, false), Some(KeyEvent::WordBreak('.')));
    }

    #[test]
    fn modifier_keys_are_ignored_rather_than_treated_as_movement() {
        // Nếu phím lạ bị coi là "con trỏ đã nhảy" thì bộ đệm chết mỗi lần chạm Shift,
        // và Tier 2 sẽ không bao giờ hoàn thành nổi một từ viết hoa.
        for vk in [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, 0x11, 0x12, 0x14, 0x70] {
            assert_eq!(classify(vk, 0, false), None, "vk={vk:#x}");
        }
    }

    #[test]
    fn digits_with_shift_are_not_digits() {
        // Shift+2 là `@`, không phải `2`. Trả None còn hơn trả sai — bố cục bàn phím
        // khác nhau cho ký tự khác nhau, và đoán bừa sẽ bỏ rác vào bộ đệm.
        assert_eq!(classify(0x32, 0, false), Some(KeyEvent::Char('2')));
        assert_eq!(classify(0x32, 0, true), None);
    }
}
