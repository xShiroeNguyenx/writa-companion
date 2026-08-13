//! P4 — Tier 2: kiểm tra ngay lúc gõ.
//!
//! Nối [`writa_win::hook`] → [`writa_win::buffer`] → engine → overlay inline.
//!
//! # Ba quyết định định hình module này
//!
//! **1. Chỉ kiểm khi từ đã xong.** Kiểm giữa chừng thì mọi tiền tố đều "sai chính
//! tả" — `ngh` không phải âm tiết. Bộ đệm chỉ phát ra từ khi user gõ space, dấu câu
//! hay Enter.
//!
//! **2. Kiểm cả cụm, báo một từ.** Lỗi người Việt mắc nhiều nhất là *real-word*
//! (`chia sẽ`), và loại đó vô hình khi xét một âm tiết đơn lẻ — `sẽ` là từ hoàn toàn
//! đúng. Nên ta đưa vài từ gần nhất vào engine để nó có ngữ cảnh, rồi chỉ báo lỗi
//! nằm trên từ vừa gõ xong. Thiếu bước này, Tier 2 chỉ bắt được `nghành` và mù với
//! toàn bộ nhóm lỗi quan trọng hơn.
//!
//! **3. Không tự sửa theo mặc định.** Xem [`crate::config::Settings::auto_fix`].

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager};
use writa_core::{Confidence, DiagnosticKind};
use writa_win::buffer::{KeyEvent, WordBuffer};
use writa_win::hook::{self, HookEvent};
use writa_win::{caret, context, overlay, writer};

use crate::debug::dbg_log;
use crate::state::AppState;

pub const INLINE: &str = "inline";

/// Số từ giữ lại làm ngữ cảnh cho engine.
///
/// Mô hình ngôn ngữ là 3-gram nên hai từ bên trái là đủ về lý thuyết. Giữ 5 để lớp
/// từ ghép và luật dấu câu cũng có chỗ dựa, và vì chi phí gần như bằng không.
const CONTEXT_WORDS: usize = 5;

/// Bao nhiêu từ cuối được đem ra xét mỗi lần có từ mới hoàn thành.
///
/// # Vì sao không phải 1
///
/// Lớp real-word cần ngữ cảnh **hai bên**, nhưng đúng lúc một từ vừa kết thúc thì bên
/// phải của nó còn chưa tồn tại. Đo trên các cặp thật:
///
/// | Gõ xong | Xét riêng từ cuối | Có từ sau |
/// |---|---|---|
/// | `nay sữa` | im lặng | `sữa lỗi` → `sửa` |
/// | `kết quả suất` | im lặng | `suất sắc` → `xuất` |
/// | `chia sẽ` | `sẻ` ✅ | — |
///
/// Nên mỗi lần có từ mới, ta xét lại **cả từ trước nó** — lúc này nó đã có đủ hai
/// bên. Cái giá: gợi ý cho nhóm đó đến muộn một từ. Đổi lại là nó đến.
const RECHECK_WORDS: usize = 2;

/// Ngưỡng real-word được **nới ra** bao nhiêu cho Tier 2.
///
/// # Vì sao Tier 2 cần ngưỡng khác Tier 1
///
/// Ngưỡng 6 của [`writa_core::DEFAULT_REAL_WORD_MARGIN`] được chọn bằng cách đo trên
/// **câu đầy đủ** — đúng tình huống của Tier 1. Tier 2 thì không bao giờ có cả câu: nó
/// chỉ có tối đa [`CONTEXT_WORDS`] từ đã gõ xong, và ngữ cảnh đó bị vứt sạch mỗi lần
/// user di con trỏ. Ngữ cảnh ngắn hơn nghĩa là bằng chứng ít hơn cho cùng một lỗi.
///
/// Chênh lệch không nhỏ: `chia sẽ` — ví dụ chính tả tiêu biểu nhất của tiếng Việt, và
/// là ví dụ mở đầu README — đứng một mình chỉ được **5,56**, nên ngưỡng 6 làm Tier 2
/// im lặng trước đúng lỗi nó sinh ra để bắt. Thêm một từ ngữ cảnh (`muốn chia sẽ`) là
/// vượt ngay.
///
/// Quét lại bằng `writa-cli eval-realtime`, tức phép đo **mô phỏng đúng hình dạng
/// Tier 2** thay vì dùng lại số của câu đầy đủ:
///
/// | margin | Báo oan / 1000 từ | Recall |
/// |---|---|---|
/// | 6 | 0,48 | 88,7% |
/// | **5** | **0,91** | **91,8%** |
/// | 4,5 | 1,21 | 93,0% |
/// | 4 | 1,71 | 94,0% |
/// | 3,5 | 2,25 | 95,1% |
///
/// Chốt **1,0** (6 → 5): đổi gấp đôi báo oan lấy 3,1 điểm recall, vẫn dưới **một nửa**
/// ngân sách 2,0/1000 của MVP, và bắt được `chia sẽ`. Không đi xa hơn vì hướng lệch của
/// dự án là precision trước — 4,5 mua thêm 1,2 điểm recall bằng 33% báo oan nữa.
///
/// Là **độ nới**, không phải hằng số tuyệt đối, nên lựa chọn "Độ nhạy" của user vẫn có
/// tác dụng ở cả hai tier.
const REALTIME_MARGIN_RELIEF: f64 = 1.0;

/// Ngưỡng thấp nhất cho phép, kể cả khi user chọn mức nhạy nhất.
const REALTIME_MARGIN_FLOOR: f64 = 3.0;

/// Tuỳ chọn kiểm tra dành riêng cho Tier 2.
fn realtime_options(settings: &crate::config::Settings) -> writa_core::CheckOptions {
    let mut opts = settings.check_options();
    opts.real_word_margin =
        (opts.real_word_margin - REALTIME_MARGIN_RELIEF).max(REALTIME_MARGIN_FLOOR);
    opts
}

/// Sau bao nhiêu ký tự gõ thêm thì gợi ý tự tắt.
///
/// Gợi ý còn áp dụng được là vì ta biết chính xác phải xoá ngược bao nhiêu ký tự.
/// Càng gõ thêm nhiều thì con số đó càng dễ lệch — user có thể đã bấm chuột, dùng
/// mũi tên, hoặc chính app đã tự động thêm gì đó. Cắt sớm an toàn hơn đoán.
const MAX_TRAIL: usize = 24;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Suggestion {
    from: String,
    to: String,
    hotkey: String,
    certain: bool,
}

/// Một gợi ý đang hiện, kèm đủ thông tin để hoàn tác về text gốc.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    word: String,
    replacement: String,
    /// Ký tự đã kết thúc từ (space, dấu câu…). Phải gõ lại sau khi thay.
    breaker: char,
    /// Những gì user gõ thêm sau đó. Cũng phải gõ lại.
    trail: String,
}

impl Pending {
    /// Ghi nhận một phím user gõ **sau khi** gợi ý đã hiện.
    ///
    /// Trả `false` khi không còn theo dõi được nữa. Áp dụng gợi ý nghĩa là xoá ngược
    /// đúng `n` ký tự rồi gõ lại; mất dấu `n` thì thao tác đó xoá sai chỗ, nên khi
    /// nghi ngờ ta tắt gợi ý thay vì đoán.
    fn absorb(&mut self, event: KeyEvent) -> bool {
        match event {
            KeyEvent::Char(c) | KeyEvent::WordBreak(c) => self.trail.push(c),
            // Xoá vào phần đuôi thì còn theo được; xoá quá đuôi là chạm vào đoạn ta
            // định thay, và từ đó trở đi mọi con số đều sai.
            KeyEvent::Backspace => return self.trail.pop().is_some(),
            _ => {}
        }
        self.trail.chars().count() <= MAX_TRAIL
    }
}

/// Một chỗ engine đề nghị sửa, kèm vị trí trong cụm ngữ cảnh.
#[derive(Debug)]
struct Hit {
    /// Chỉ số trong [`Realtime::context`].
    index: usize,
    replacement: String,
    certain: bool,
}

#[derive(Default)]
pub struct Realtime {
    buffer: WordBuffer,
    /// Từ gần nhất, kèm ký tự đã kết thúc mỗi từ.
    ///
    /// Phải giữ cả ký tự ngắt vì khi sửa một từ **không phải từ cuối**, ta xoá ngược
    /// qua nó rồi gõ lại — và gõ lại bằng space trong khi user gõ dấu phẩy là làm
    /// hỏng câu của họ.
    context: VecDeque<(String, char)>,
    pending: Option<Pending>,
    /// App đang focus có được phép can thiệp không. Tính lại khi đổi cửa sổ.
    app_allowed: bool,
}

static STATE: Mutex<Option<Realtime>> = Mutex::new(None);

/// Bật hoặc tắt Tier 2.
///
/// Tắt là **tháo hook thật sự**, không phải bỏ qua sự kiện — xem
/// [`writa_win::hook::stop`].
pub fn set_enabled(app: &AppHandle, on: bool) {
    if on {
        // Tính quyền ngay chứ không đợi lần đổi cửa sổ đầu tiên: user bật realtime
        // trong cửa sổ cài đặt rồi quay lại app đang gõ dở, và nếu chỉ khởi tạo bằng
        // `default()` thì `app_allowed = false` cho tới khi họ tình cờ chuyển app —
        // tính năng trông như hỏng.
        *STATE.lock().unwrap() = Some(Realtime {
            app_allowed: app_allowed(app),
            ..Default::default()
        });
        let handle = app.clone();
        match hook::start(move |ev| on_event(&handle, ev)) {
            Ok(()) => dbg_log!(
                "realtime: BAT, app_allowed={}",
                STATE
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|r| r.app_allowed)
            ),
            Err(e) => dbg_log!("realtime: khong cai duoc hook: {e}"),
        }
    } else {
        hook::stop();
        *STATE.lock().unwrap() = None;
        hide(app);
    }
}

pub fn is_enabled() -> bool {
    hook::is_running()
}

fn on_event(app: &AppHandle, ev: HookEvent) {
    // Tab đã bị chặn ở tầng hook; việc còn lại là áp dụng. Không giữ khoá state khi
    // gõ phím — `accept` sẽ tự lấy lại.
    let HookEvent::Key { event, source, .. } = ev else {
        let app = app.clone();
        std::thread::spawn(move || accept(&app));
        return;
    };

    let mut guard = STATE.lock().unwrap();
    let Some(rt) = guard.as_mut() else {
        return;
    };

    // Tab và Enter di chuyển focus trong form — sang ô khác, có thể là ô mật khẩu — mà
    // **không** bắn `EVENT_SYSTEM_FOREGROUND` và cũng không phải click. Đây là lối duy
    // nhất còn lại để phần tử focus đổi mà ta không hay, nên tính lại quyền ở đây.
    //
    // Vẫn để chúng đi tiếp như dấu ngắt từ: Tab/Enter kết thúc một từ, và đó là sự thật
    // độc lập với việc focus có đổi hay không.
    if matches!(event, KeyEvent::WordBreak('\t') | KeyEvent::WordBreak('\n')) {
        rt.app_allowed = app_allowed(app);
    }

    match event {
        // Đổi cửa sổ: bộ đệm không còn ứng với gì cả, và app mới có thể là app bị
        // chặn. Tính lại quyền TRƯỚC khi nhận thêm phím nào.
        KeyEvent::FocusChanged => {
            rt.buffer.clear();
            rt.context.clear();
            rt.pending = None;
            rt.app_allowed = app_allowed(app);
            drop(guard);
            hide(app);
            return;
        }
        KeyEvent::CaretMoved => {
            rt.buffer.clear();
            rt.context.clear();
            rt.pending = None;
            drop(guard);
            hide(app);
            return;
        }
        _ => {}
    }

    if !rt.app_allowed {
        return;
    }
    if matches!(event, KeyEvent::WordBreak(_)) {
        dbg_log!("realtime: het tu, dem = {:?}", rt.buffer.current());
    }

    // Gợi ý đang hiện thì phải theo dõi mọi thứ user gõ thêm, để còn xoá ngược đúng.
    if rt.pending.as_mut().is_some_and(|p| !p.absorb(event)) {
        // Hết theo dõi được thì bỏ gợi ý — nhưng **vẫn phải nạp phím vào bộ đệm**.
        // Thoát sớm ở đây làm từ đang gõ hụt một ký tự, và một từ hụt ký tự thì mọi
        // phán quyết sau đó đều dựa trên dữ liệu sai.
        rt.pending = None;
        hide(app);
    }

    let breaker = match event {
        KeyEvent::WordBreak(c) => c,
        _ => '\0',
    };
    let Some(word) = rt.buffer.feed(event, source) else {
        return;
    };

    rt.context.push_back((word, breaker));
    while rt.context.len() > CONTEXT_WORDS {
        rt.context.pop_front();
    }

    // Trước đây ở đây có một lần gọi `is_password_element()` cho **mỗi từ**, làm lớp
    // chắn cuối. Đã bỏ, vì phép đo cho thấy nó đắt khủng khiếp mà gần như không thêm
    // được gì:
    //
    // | Việc | p50 | p99 |
    // |---|---|---|
    // | `is_password_element` (UIA) | 3,16 ms | **200,52 ms** |
    // | `check_with` (chính việc kiểm tra) | 0,13 ms | 0,20 ms |
    //
    // Nó tốn gấp một nghìn lần chính việc kiểm tra chính tả, và vì thread tiêu thụ chạy
    // tuần tự, một lần 200 ms làm mọi phím sau đó dồn lại — gợi ý đến sau khi user đã
    // gõ xong câu.
    //
    // Cái nó bảo vệ thì ba lối kia đã bịt: đổi cửa sổ, click chuột (đều báo lên là
    // `FocusChanged`), và Tab/Enter (xử lý ngay đầu hàm này). Mỗi lối đó đều gọi
    // `app_allowed`, vốn có hỏi UIA — chỉ là hỏi khi focus **thật sự có thể đã đổi**,
    // không phải sau từng từ.
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let found = decide(&rt.context, rt.pending.as_ref(), &settings);
    dbg_log!("realtime: xet {:?} -> {found:?}", rt.context);

    match found {
        Outcome::Keep => {}
        Outcome::Clear => {
            rt.pending = None;
            hide(app);
        }
        Outcome::Show { pending, certain } => {
            if certain && settings.auto_fix {
                rt.pending = None;
                drop(guard);
                apply(
                    app,
                    &pending.word,
                    &pending.replacement,
                    pending.breaker,
                    &pending.trail,
                );
                return;
            }
            let (word, replacement) = (pending.word.clone(), pending.replacement.clone());
            rt.pending = Some(pending);
            drop(guard);
            // Hiện `Tab` chứ không phải `hotkey_accept`: Tab là phím tay đang đặt sẵn
            // ở đó, còn phím tắt cấu hình được vẫn chạy song song như đường lùi.
            show(app, &word, &replacement, certain, "Tab");
        }
    }
}

/// Việc cần làm sau khi một từ vừa hoàn thành.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Hiện một gợi ý mới.
    Show { pending: Pending, certain: bool },
    /// Giữ nguyên những gì đang hiện.
    Keep,
    /// Không có gì để hiện.
    Clear,
}

/// Phần **quyết định** của Tier 2, tách khỏi Win32 và khỏi trạng thái toàn cục.
///
/// Tách ra để test được: hai lỗi nặng nhất của module này đều là lỗi *quyết định*, và
/// cả hai chỉ lộ ra khi gõ một câu dài trên máy thật — đúng thứ không lặp lại được
/// bằng tay.
fn decide(
    context: &VecDeque<(String, char)>,
    pending: Option<&Pending>,
    settings: &crate::config::Settings,
) -> Outcome {
    let Some(hit) = evaluate(context, settings) else {
        // **Không** xoá gợi ý đang hiện.
        //
        // Cửa sổ xét lại chỉ nhìn hai từ cuối, nên một từ bị báo rơi ra khỏi tầm nhìn
        // ngay khi user gõ thêm hai từ. Rơi khỏi tầm nhìn **không** có nghĩa là đã
        // được sửa. Bản đầu hiểu "lần này không thấy lỗi" thành "xoá gợi ý", nên gợi ý
        // chỉ sống đúng một nhịp gõ — gõ liền tay thì không kịp thấy gì cả.
        //
        // Gợi ý chỉ hết hạn bằng tín hiệu THẬT: gõ quá xa ([`MAX_TRAIL`]), di con trỏ,
        // đổi cửa sổ, xoá ngược qua nó, hoặc bấm nhận.
        return if pending.is_some() {
            Outcome::Keep
        } else {
            Outcome::Clear
        };
    };

    // Từ bị báo có thể không phải từ cuối, nên phải dựng lại cả phần đuôi để lát nữa
    // gõ lại y nguyên.
    let (word, breaker) = context[hit.index].clone();

    // Đúng gợi ý đang hiện: để yên. Vẽ lại làm bong bóng nháy, và tệ hơn là nó reset
    // phần đuôi đang được [`Pending::absorb`] tích luỹ.
    if pending.is_some_and(|p| p.word == word && p.replacement == hit.replacement) {
        return Outcome::Keep;
    }

    Outcome::Show {
        pending: Pending {
            word,
            replacement: hit.replacement,
            breaker,
            trail: context
                .iter()
                .skip(hit.index + 1)
                .map(|(w, b)| format!("{w}{b}"))
                .collect(),
        },
        certain: hit.certain,
    }
}

/// Chạy engine trên cụm từ và trả về đề xuất cho **một trong hai từ cuối**.
///
/// Ưu tiên từ mới nhất; chỉ khi nó sạch mới xét lại từ trước — xem [`RECHECK_WORDS`].
fn evaluate(context: &VecDeque<(String, char)>, settings: &crate::config::Settings) -> Option<Hit> {
    if context.is_empty() {
        return None;
    }
    // Ghép bằng space chứ không bằng ký tự ngắt thật: mô hình ngôn ngữ làm việc trên
    // chuỗi âm tiết, còn dấu câu thì lớp L5 lo và Tier 2 không xét tới nó.
    let words: Vec<&str> = context.iter().map(|(w, _)| w.as_str()).collect();
    let text = words.join(" ");

    // Vị trí byte đầu mỗi từ trong cụm đã ghép.
    let mut starts = Vec::with_capacity(words.len());
    let mut at = 0usize;
    for w in &words {
        starts.push(at);
        at += w.len() + 1; // +1 cho space
    }

    let diagnostics = writa_core::check_with(&text, realtime_options(settings));

    // Từ mới nhất trước, rồi lùi dần.
    for index in (context.len().saturating_sub(RECHECK_WORDS)..context.len()).rev() {
        if settings.ignores(words[index]) {
            continue;
        }
        let hit = diagnostics
            .iter()
            .filter(|d| d.span.start == starts[index])
            // Dấu câu và viết hoa không thuộc về Tier 2: cụm ngữ cảnh ta ghép bằng
            // space nên nó không phản ánh dấu câu thật user gõ, và phán quyết trên một
            // dữ liệu đã bị bóp méo thì tệ hơn là không phán quyết.
            .filter(|d| {
                matches!(
                    d.kind,
                    DiagnosticKind::InvalidSyllable
                        | DiagnosticKind::UnattestedSyllable
                        | DiagnosticKind::ConfusedSyllable
                )
            })
            .find_map(|d| {
                d.candidates.first().map(|c| Hit {
                    index,
                    replacement: c.clone(),
                    certain: d.confidence == Confidence::Certain,
                })
            });
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// App đang focus có được can thiệp không.
fn app_allowed(app: &AppHandle) -> bool {
    let allowed = app_allowed_inner(app);
    dbg_log!("realtime: app_allowed -> {allowed}");
    allowed
}

fn app_allowed_inner(app: &AppHandle) -> bool {
    let Ok(ctx) = context::current() else {
        dbg_log!("realtime:   khong co cua so foreground");
        return false;
    };
    dbg_log!(
        "realtime:   exe={} password={} blocklisted={} uia_password={}",
        ctx.exe,
        ctx.is_password_field,
        ctx.is_blocklisted(),
        writa_win::selection::is_password_element()
    );
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    // Cửa sổ của chính Writa không tính.
    let own = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    if own.is_some_and(|o| o == ctx.exe) {
        return false;
    }
    ctx.is_safe_to_assist()
        && !settings.blocks(&ctx.exe)
        && !writa_win::selection::is_password_element()
}

/// Áp dụng gợi ý đang hiện. Gọi từ phím tắt.
pub fn accept(app: &AppHandle) {
    let pending = {
        let mut guard = STATE.lock().unwrap();
        match guard.as_mut() {
            Some(rt) => rt.pending.take(),
            None => None,
        }
    };
    let Some(p) = pending else {
        return;
    };
    apply(app, &p.word, &p.replacement, p.breaker, &p.trail);
}

/// Gõ lại đoạn cuối với từ đã sửa.
fn apply(app: &AppHandle, word: &str, replacement: &str, breaker: char, trail: &str) {
    hide(app);

    let mut tail = String::from(replacement);
    if breaker != '\0' {
        tail.push(breaker);
    }
    tail.push_str(trail);

    let erase = word.chars().count() + usize::from(breaker != '\0') + trail.chars().count();

    // Bịt hook trong lúc tự bơm phím: xem `hook::set_muted`.
    hook::set_muted(true);
    let result = writer::replace_last(erase, &tail);
    // Phím bơm tới hook không hoàn toàn đồng bộ với lúc `SendInput` trả về; nhả sớm
    // thì bộ đệm nghe lại chính mình.
    std::thread::sleep(Duration::from_millis(40));
    hook::set_muted(false);

    if let Some(rt) = STATE.lock().unwrap().as_mut() {
        // Bộ đệm và ngữ cảnh vừa bị ta viết lại — bắt đầu lại từ đầu thay vì đoán.
        rt.buffer.clear();
        rt.context.clear();
        rt.pending = None;
    }
    dbg_log!("realtime: sua {word:?} -> {replacement:?} ({result:?})");
}

fn show(app: &AppHandle, from: &str, to: &str, certain: bool, hotkey: &str) {
    let sent = app.emit_to(
        INLINE,
        "writa://inline",
        Suggestion {
            from: from.to_string(),
            to: to.to_string(),
            hotkey: hotkey.to_string(),
            certain,
        },
    );
    dbg_log!("realtime: goi y {from:?} -> {to:?}, emit {sent:?}");
    // Chỉ chặn Tab trong đúng khoảng thời gian gợi ý đang hiện.
    hook::set_swallow_tab(true);
    // Cửa sổ hiện ra ở `fit_inline`, sau khi phía JS đo xong nội dung.
}

pub fn hide(app: &AppHandle) {
    // Nhả Tab về cho app đích ngay khi gợi ý biến mất. Đây là nửa còn lại của lời hứa
    // ở `hook::set_swallow_tab`: Tab chỉ bị chặn *đúng* lúc có gợi ý.
    hook::set_swallow_tab(false);
    if let Some(win) = app.get_webview_window(INLINE) {
        if let Ok(h) = win.hwnd() {
            overlay::hide(h.0 as isize);
        }
    }
}

/// Đặt kích thước overlay theo nội dung rồi hiện nó cạnh caret, **không lấy focus**.
pub fn fit_and_show(app: &AppHandle, width: f64, height: f64) -> Result<(), String> {
    dbg_log!("fit_inline: {width}x{height}");
    let win = app
        .get_webview_window(INLINE)
        .ok_or_else(|| "không tìm thấy overlay".to_string())?;
    let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;

    win.set_size(LogicalSize::new(width, height))
        .map_err(|e| e.to_string())?;

    // Neo dưới caret của app đích. `caret::locate` không bao giờ thất bại — bậc cuối
    // là vị trí chuột.
    if let Ok(ctx) = context::current() {
        let c = caret::locate(&ctx);
        let dy = if c.source.is_exact() {
            c.height + 4
        } else {
            20
        };
        overlay::move_to(hwnd, c.x, c.y + dy);
    }
    overlay::show_no_activate(hwnd);
    Ok(())
}

/// Gắn cờ không-lấy-focus cho overlay. Gọi một lần lúc khởi động.
pub fn prepare_overlay(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(INLINE) {
        if let Ok(h) = win.hwnd() {
            overlay::make_non_activating(h.0 as isize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;

    /// Gõ một câu, từng từ một, y như Tier 2 nhìn thấy.
    ///
    /// Trả về, cho mỗi từ hoàn thành, gợi ý **đang hiện** sau bước đó — chính là thứ
    /// user thấy trên màn hình.
    fn type_sentence(sentence: &str) -> Vec<Option<(String, String)>> {
        let settings = Settings::default();
        let mut context: VecDeque<(String, char)> = VecDeque::new();
        let mut pending: Option<Pending> = None;
        let mut visible = Vec::new();

        for word in sentence.split(' ') {
            context.push_back((word.to_string(), ' '));
            while context.len() > CONTEXT_WORDS {
                context.pop_front();
            }
            match decide(&context, pending.as_ref(), &settings) {
                Outcome::Keep => {}
                Outcome::Clear => pending = None,
                Outcome::Show { pending: p, .. } => pending = Some(p),
            }
            visible.push(
                pending
                    .as_ref()
                    .map(|p| (p.word.clone(), p.replacement.clone())),
            );
        }
        visible
    }

    #[test]
    fn suggestion_survives_typing_more_words() {
        // Đây là câu user gõ thật khi báo "không có gợi ý gì hết".
        //
        // Bản trước hiện gợi ý `sữa` ở bước 3 rồi **tự xoá nó ở bước 4**, vì cửa sổ
        // xét lại chỉ nhìn hai từ cuối và `sữa` đã rơi ra ngoài. Gõ liền tay thì gợi ý
        // sống chưa tới một giây — không ai thấy.
        let seen = type_sentence("nay sữa lỗi chính tã chia sẽ");

        assert_eq!(seen[0], None, "chưa đủ ngữ cảnh ở `nay`");
        assert_eq!(seen[1], None, "`sữa` chưa có ngữ cảnh bên phải");
        assert_eq!(
            seen[2],
            Some(("sữa".into(), "sửa".into())),
            "có `lỗi` rồi thì phải bắt được `sữa`"
        );
        // Bước 4 và 5 là chỗ bản cũ vỡ.
        assert_eq!(
            seen[3],
            Some(("sữa".into(), "sửa".into())),
            "gõ thêm một từ không được làm gợi ý biến mất"
        );
        assert_eq!(
            seen[4],
            Some(("tã".into(), "tả".into())),
            "`tã` mới hơn nên thay chỗ `sữa`"
        );
    }

    #[test]
    fn catches_the_canonical_vietnamese_error_with_only_two_words() {
        // `chia sẽ` là ví dụ chính tả tiêu biểu nhất của tiếng Việt và là ví dụ mở đầu
        // README. Đứng một mình nó chỉ được chênh 5,56, nên ngưỡng 6 của Tier 1 làm
        // Tier 2 im lặng trước đúng lỗi nó sinh ra để bắt. Xem
        // [`REALTIME_MARGIN_RELIEF`].
        let seen = type_sentence("chia sẽ");
        assert_eq!(seen[1], Some(("sẽ".into(), "sẻ".into())), "{seen:?}");

        // Và không được báo oan vào dạng viết đúng.
        assert!(type_sentence("chia sẻ").iter().all(|s| s.is_none()));
    }

    #[test]
    fn realtime_margin_stays_looser_than_tier_1_but_never_reckless() {
        let s = Settings::default();
        assert_eq!(s.check_options().real_word_margin, 6.0);
        assert_eq!(realtime_options(&s).real_word_margin, 5.0);

        // User chọn mức nhạy nhất thì Tier 2 vẫn không được rơi xuống dưới sàn.
        let nhay = Settings {
            real_word_margin: 3.0,
            ..Settings::default()
        };
        assert_eq!(
            realtime_options(&nhay).real_word_margin,
            REALTIME_MARGIN_FLOOR
        );

        // User chọn mức thận trọng thì Tier 2 cũng thận trọng theo.
        let than_trong = Settings {
            real_word_margin: 9.0,
            ..Settings::default()
        };
        assert_eq!(realtime_options(&than_trong).real_word_margin, 8.0);
    }

    #[test]
    fn real_word_layer_can_never_be_switched_off_from_settings() {
        // Bảo hiểm cho một lỗi đã xảy ra thật: `detectRealWord: false` lọt vào file cấu
        // hình và làm toàn bộ nhóm lỗi nhầm từ biến mất, khiến app trông như hỏng. Giờ
        // `Settings` không còn trường đó nữa; test này canh cho nó đừng quay lại.
        assert!(Settings::default().check_options().detect_real_word);
        let mut s = Settings {
            flag_unattested: true,
            real_word_margin: 30.0,
            ..Settings::default()
        };
        s.sanitize();
        assert!(s.check_options().detect_real_word);
    }

    #[test]
    fn hotkeys_are_normalised_so_spacing_does_not_look_like_a_conflict() {
        let mut s = Settings {
            hotkey_accept: "Ctrl + Alt + Space".into(),
            hotkey_check: "  Ctrl+Alt+V  ".into(),
            ..Settings::default()
        };
        s.sanitize();
        assert_eq!(s.hotkey_accept, "Ctrl+Alt+Space");
        assert_eq!(s.hotkey_check, "Ctrl+Alt+V");
    }

    #[test]
    fn adjacent_errors_still_produce_a_suggestion() {
        // Câu user gõ thật. Hai từ sai **cạnh nhau** (`sữa lổi`), nên mô hình ngôn ngữ
        // mất điểm tựa: `sữa lỗi` và `sửa lổi` đều không phải tổ hợp có thật.
        //
        // Ta vẫn phải nói được điều gì đó — im lặng hoàn toàn là hỏng. Nhưng đề xuất
        // cho `lổi` sẽ KHÔNG chính xác (`nổi` thắng `lỗi` trong ngữ cảnh đã méo), và
        // test này ghi nhận đúng hiện trạng đó thay vì giả vờ nó tốt hơn.
        let seen = type_sentence("nay tôi sữa lổi chính tẻ");
        assert!(seen[3].is_some(), "phải báo được gì đó ở `lổi`: {seen:?}");

        // Khi từ bên cạnh đúng thì đề xuất cũng đúng.
        let ok = type_sentence("nay tôi sữa lỗi");
        assert_eq!(ok[3], Some(("sữa".into(), "sửa".into())));
        let ok = type_sentence("nay tôi sửa lổi");
        assert_eq!(ok[3], Some(("lổi".into(), "lỗi".into())));
    }

    #[test]
    fn a_clean_sentence_shows_nothing() {
        let seen = type_sentence("hôm nay tôi đi học ở trường");
        assert!(seen.iter().all(|s| s.is_none()), "{seen:?}");
    }

    #[test]
    fn trail_lets_a_non_final_word_be_replaced_exactly() {
        // Sửa một từ không phải từ cuối nghĩa là xoá ngược qua cả phần đuôi rồi gõ
        // lại. Nếu phần đuôi dựng sai thì thao tác đó cắt mất chữ của user.
        let settings = Settings::default();
        let mut context = VecDeque::new();
        for w in ["nay", "sữa", "lỗi"] {
            context.push_back((w.to_string(), ' '));
        }
        let Outcome::Show { pending, .. } = decide(&context, None, &settings) else {
            panic!("phải bắt được `sữa`");
        };
        assert_eq!(pending.word, "sữa");
        assert_eq!(pending.breaker, ' ');
        assert_eq!(pending.trail, "lỗi ");

        // Mô phỏng đúng phép tính của `apply`.
        let erase = pending.word.chars().count() + 1 + pending.trail.chars().count();
        let typed = format!(
            "{}{}{}",
            pending.replacement, pending.breaker, pending.trail
        );
        assert_eq!(erase, "sữa lỗi ".chars().count());
        assert_eq!(typed, "sửa lỗi ");
    }

    #[test]
    fn absorb_stops_tracking_once_the_user_types_too_far() {
        let mut p = Pending {
            word: "sữa".into(),
            replacement: "sửa".into(),
            breaker: ' ',
            trail: String::new(),
        };
        for _ in 0..MAX_TRAIL {
            assert!(p.absorb(KeyEvent::Char('x')));
        }
        assert!(
            !p.absorb(KeyEvent::Char('x')),
            "quá xa thì phải bỏ theo dõi"
        );
    }

    #[test]
    fn absorb_gives_up_when_backspace_passes_the_suggestion() {
        let mut p = Pending {
            word: "sữa".into(),
            replacement: "sửa".into(),
            breaker: ' ',
            trail: "lỗi ".into(),
        };
        for _ in 0..4 {
            assert!(p.absorb(KeyEvent::Backspace));
        }
        // Xoá tiếp là chạm vào chính đoạn ta định thay — từ đó mọi con số đều sai.
        assert!(!p.absorb(KeyEvent::Backspace));
    }
}
