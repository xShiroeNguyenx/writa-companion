# Writa — App desktop sửa lỗi chính tả tiếng Việt system-wide

> Ngày lập: 2026-08-11 · Cập nhật trạng thái: 2026-08-12
>
> Tài liệu này là **kế hoạch gốc**, giữ nguyên để đối chiếu. Trạng thái thật và các
> chỗ thiết kế đã đổi so với kế hoạch nằm ở [SPIKE_RESULTS.md](SPIKE_RESULTS.md).
>
> | Phase | Trạng thái |
> |---|---|
> | P0 spike | 🟡 thư viện `writa-win` xong; spike 1/5/6 và ma trận app cần chạy tay |
> | P1 engine + data | ✅ L0–L6, FP 0,25–0,53/1000, F0.5 0,979 |
> | P2 Tier 1 shell | ✅ **app dùng được** — khay hệ thống, phím tắt, popup, cài đặt |
> | P3 thêm dấu | ✅ 94,47% văn xuôi (dưới mục tiêu 95%) |
> | P4 Tier 2 real-time | 🟡 `buffer.rs` xong; phần hook chờ spike 5 |
> | P5 rules + profiles | 🟡 L5 xong; biến thể vùng miền và per-app profile UI còn lại |
> | P6 AI + insights | 🟡 `writa-ai` gọi được Claude API; insights chưa có |
> | P7 ship | ⬜ |
>
> **Khác kế hoạch đáng kể:** popup Tier 1 *cố tình* lấy focus thay vì dùng
> `WS_EX_NOACTIVATE` — lý do ở SPIKE_RESULTS.md, mục "Spike 1".

---

## Context

Workspace `writa-companion/` hiện trống hoàn toàn — đây là greenfield project.

**Vấn đề cần giải:** người Việt gõ tiếng Việt sai chính tả liên tục ở mọi nơi (Zalo, Messenger, Gmail, Word, Teams, comment Facebook) và không có công cụ nào chạy xuyên app. Các giải pháp hiện có đều bị giới hạn phạm vi: Word chỉ trong Word, extension chỉ trong browser, LanguageTool không hỗ trợ tiếng Việt, Grammarly không biết tiếng Việt. Lỗi phổ biến nhất của người Việt — **hỏi/ngã** — thì gần như không có tool nào xử lý tốt.

**Mục tiêu:** một app desktop chạy nền, phát hiện lỗi chính tả tiếng Việt ở **bất kỳ ô nhập text nào trên máy**, đề xuất sửa inline, hoạt động 100% offline, latency dưới ngưỡng cảm nhận. MVP tiếng Việt, kiến trúc mở đường cho ngôn ngữ khác.

**Tên app:** Writa.

---

## Quyết định đã chốt

| Hạng mục | Quyết định |
|---|---|
| **Chiến lược capture** | Cả hai tier, **hotkey-on-selection trước** (Tier 1, universal), real-time keyboard hook sau (Tier 2, magic) |
| **Engine** | Offline lai tầng: syllable FST + dictionary + confusion-set + n-gram LM. AI chỉ là lớp opt-in về sau |
| **License** | **MIT/Apache-2.0** → **KHÔNG** dùng `hunspell-vi` (GPLv3). Phải tự build toàn bộ data từ nguồn permissive |
| **Stack** | **Tauri 2 + Rust core** + React/TS UI |
| **Thêm dấu tự động** | **Có trong MVP** (dùng chung ~90% hạ tầng với spell-check) |
| **Hành vi sửa** | Mặc định gợi ý + chờ `Tab`. Tự sửa chỉ với tập lỗi confidence 100%, có toast undo, có setting tắt |
| **Platform v1** | **Windows-only**. macOS/Linux ở roadmap |

---

## Insight cốt lõi định hình thiết kế

Bốn nhận định này quyết định toàn bộ kiến trúc — nếu bỏ chúng thì thiết kế sẽ khác hẳn.

### 1. Tiếng Việt có tập âm tiết ĐÓNG và sinh được — khác căn bản tiếng Anh

Tiếng Việt chỉ có ~**17.974** âm tiết hợp lệ về ngữ âm, sinh ra từ `25 âm đầu × 162 vần × 6 thanh` với ràng buộc thanh–vần ([nguồn liệt kê](https://www.hieuthi.com/blog/2017/03/21/all-vietnamese-syllables.html)). Hệ quả cực lớn:

- Ta **tự sinh được tập này bằng script từ bảng ngữ âm** → không copy dictionary nào → **không vướng license GPL**. Đây chính là cách thoát ràng buộc `hunspell-vi`.
- Bất kỳ âm tiết **không nằm trong tập** = **sai chính tả chắc chắn 100%**, không cần ngữ cảnh, không cần model. `"nghành"`, `"quyêt"`, `"khoăn"` → phát hiện bằng một lookup FST ~50µs. Tiếng Anh không có tính chất này.
- Trong 17.974 âm tiết hợp lệ ngữ âm, chỉ ~**7.000** thực sự xuất hiện trong văn bản. Nhóm "hợp lệ nhưng chưa từng thấy" là tín hiệu **nghi vấn**, cần ngữ cảnh phán quyết.

→ Engine phải chia rõ **hai loại lỗi khác bản chất**: *non-word* (giải quyết bằng lookup, precision 100%) và *real-word* (âm tiết hợp lệ nhưng sai ngữ cảnh — `"chia sẽ"`, `"sữa lỗi"` — bắt buộc cần LM).

### 2. Lỗi tiếng Việt tập trung vào một số cặp nhầm lẫn hữu hạn, đoán được trước

Không cần Levenshtein mù. Lỗi tiếng Việt thực tế **có cấu trúc**:

- **Thanh hỏi ↔ ngã** — lỗi số 1 (`sửa/sữa`, `chia sẻ/chia sẽ`, `mãi/mải`)
- **Âm đầu:** `s/x`, `ch/tr`, `r/d/gi`, `l/n` (Bắc), `v/d` (Nam)
- **Âm cuối:** `n/ng`, `t/c`, `nh/n` (Nam)
- **Nguyên âm:** `i/y`, `iê/ia`, `ươ/ưa`, `uô/ua`, `o/ô`
- **Cặp từ thật hay nhầm:** `dành/giành`, `chuyện/truyện`, `sử dụng/xử dụng`, `xuất sắc/suất sắc`, `bàng quan/bàng quang`, `tựu trung/tựu chung`
- **Thiếu dấu hoàn toàn** (gõ nhanh, bàn phím không IME)

→ Sinh candidate bằng **confusion-set có chủ đích** rẻ hơn và chính xác hơn Levenshtein rất nhiều. Bảng confusion-set thủ công chính là nơi chứa giá trị domain của app.

### 3. Thêm dấu tự động dùng chung hạ tầng với sửa lỗi real-word

Cả hai đều là cùng một bài toán: *sinh candidate cho từng âm tiết → decode chuỗi tốt nhất bằng n-gram LM*.

```
Sửa real-word:  "chia sẽ"  → candidates{sẻ, sẽ, sẹ...}      → Viterbi + LM → "chia sẻ"
Thêm dấu:       "chia se"  → candidates{se, sẻ, sẽ, sè, sé, sẹ} → Viterbi + LM → "chia sẻ"
```

Khác biệt duy nhất là **bước sinh candidate** (confusion-set vs strip-diacritics index). Bộ decoder, LM, dictionary dùng lại 100%. Vì vậy đưa "thêm dấu" vào MVP là quyết định đúng — chi phí biên rất nhỏ so với giá trị.

### 4. False positive là rủi ro tồn vong, không phải rủi ro chất lượng

Tool gõ real-time mà gạch đỏ sai sẽ bị tắt trong 5 phút và không bao giờ mở lại. Với tool chạy nền, **precision quan trọng hơn recall rất nhiều**.

→ Kéo theo 3 quyết định thiết kế bắt buộc:
- **Protected spans**: tuyệt đối không đụng URL, email, đường dẫn file, code, `@mention`, `#hashtag`, số, viết tắt ALL-CAPS, tên riêng trong personal dict.
- **Eval gate trong CI**: có metric FP-per-1000-words, vượt ngưỡng thì không được merge.
- **Ngưỡng theo lớp**: lớp non-word (precision 100%) auto-fix được; lớp real-word chỉ gợi ý, ngưỡng LM cao.

---

## Kiến trúc

```
┌──────────────────────────────────────────────────────────────────┐
│  Writa process (Tauri 2)                              tray icon  │
│                                                                   │
│  ┌─ writa-win (Rust, Windows-only) ──────────────────────────┐   │
│  │  CaptureTier1  hotkey → đọc selection (UIA / clipboard)   │   │
│  │  CaptureTier2  WH_KEYBOARD_LL → word buffer               │   │
│  │  CaretLocator  GetGUIThreadInfo → UIA TextPattern2 → mouse│   │
│  │  TextWriter    SendInput(KEYEVENTF_UNICODE) / clipboard   │   │
│  │  AppContext    exe name + IsPassword + app profile        │   │
│  └───────────────────────────────────────────────────────────┘   │
│                    │ text + context                              │
│                    ▼                                              │
│  ┌─ writa-core (Rust, portable — cũng build được WASM) ──────┐   │
│  │  normalize → tokenize → protected-span mask               │   │
│  │  L1 syllable validity   (FST 17.974)                      │   │
│  │  L2 word/compound dict  (FST + freq)                      │   │
│  │  L3 candidate gen       (confusion-set | strip-diacritic) │   │
│  │  L4 Viterbi decode      (3-gram LM, mmap)                 │   │
│  │  L5 rule engine         (dấu câu, hoa, spacing)           │   │
│  │  L6 AI adapter          (opt-in, BYO Claude key)          │   │
│  │  → Vec<Diagnostic{ span, kind, candidates, confidence }>  │   │
│  └───────────────────────────────────────────────────────────┘   │
│                    │ Diagnostic[]                                 │
│                    ▼                                              │
│  ┌─ Tauri shell ─────────────────────────────────────────────┐   │
│  │  overlay window   transparent · no-decoration · NOACTIVATE│   │
│  │  settings window  React + TS                              │   │
│  │  store            personal dict · app profiles · stats     │   │
│  └───────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
          ▲                                          │
          │ selection / keystroke                    │ SendInput
          ▼                                          ▼
   ═══════════ Bất kỳ app nào: Zalo, Chrome, Word, Teams ═══════════
```

**Vì sao tách `writa-core` thành crate riêng, không phụ thuộc Windows:** để nó build được sang **WASM**. Một engine, bốn mặt trận — desktop app, VSCode extension, web demo cho marketing, và CLI cho CI. Đây là đòn bẩy lớn nhất của kiến trúc này.

---

## Cấu trúc repo

```
writa-companion/
├── PLAN.md                       ← tài liệu này
├── README.md  LICENSE (MIT)  THIRDPARTY.md  DECISIONS.md
├── Cargo.toml                    workspace
├── crates/
│   ├── writa-core/               engine — KHÔNG phụ thuộc OS
│   │   ├── src/
│   │   │   ├── lib.rs            pub fn check(text, opts) -> Vec<Diagnostic>
│   │   │   ├── normalize.rs      NFC + legacy encoding + tone placement
│   │   │   ├── token.rs          tokenizer + protected-span mask
│   │   │   ├── syllable.rs       L1 — FST validity
│   │   │   ├── dict.rs           L2 — word/compound + freq
│   │   │   ├── candidate.rs      L3 — confusion-set + strip-diacritic
│   │   │   ├── lm.rs             3-gram mmap + Viterbi beam decode
│   │   │   ├── rules.rs          L5 — dấu câu, hoa, spacing
│   │   │   ├── diacritic.rs      thêm dấu (dùng lm.rs)
│   │   │   └── lang/             trait LanguageEngine → mở rộng ngôn ngữ
│   │   └── tests/
│   ├── writa-win/                Windows integration (crate `windows`)
│   ├── writa-cli/                eval harness + benchmark + batch check
│   └── writa-data/               build.rs sinh artifacts từ data/raw
├── data/
│   ├── phonology/                bảng âm đầu · vần · ràng buộc thanh (TỰ VIẾT)
│   ├── confusion/                confusion-set + cặp từ hay nhầm (TỰ VIẾT)
│   ├── raw/                      corpus tải về (gitignore)
│   ├── build/                    *.fst, ngram.bin (artifact)
│   └── eval/                     testset.jsonl có nhãn
├── src-tauri/                    shell: tray, IPC, hotkey, autostart
└── ui/                           React + TS: overlay/ + settings/
```

Crate chính dùng: `windows` (Microsoft, MIT/Apache — khớp license), `fst` (Unlicense/MIT), `unicode-normalization`, `memmap2`, `serde`. Toàn bộ permissive.

---

## Engine — thiết kế từng lớp

### L0 · Normalize + tokenize + protected spans

Chạy trước mọi thứ, sai ở đây là sai tất cả:

- **NFC normalization.** Tiếng Việt tồn tại cả dạng dựng sẵn (`ế` = U+1EBF) và dạng tổ hợp (`e` + U+0302 + U+0301). Không normalize thì mọi lookup FST fail âm thầm. Dùng `unicode-normalization`.
- **Legacy encoding**: nhận diện VNI / TCVN3 / VIQR dán vào từ file cũ, convert sang Unicode.
- **Tone placement variant**: `hòa`/`hoà`, `quý`/`qúy` — **cả hai đều được coi là đúng** theo các quy chuẩn khác nhau. Tuyệt đối KHÔNG báo lỗi. Chỉ cung cấp rule "chuẩn hóa vị trí dấu" dạng opt-in trong settings.
- **Protected-span mask** — tính trước, engine bỏ qua hoàn toàn: URL, email, đường dẫn file/Windows path, `@mention`, `#hashtag`, chuỗi trong backtick, dòng thụt đầu ≥4 space, token có chữ+số lẫn nhau (mã SP), ALL-CAPS ≥2 ký tự, emoji, entry trong personal dict.

### L1 · Syllable validity — precision 100%, ~50µs

FST set 17.974 âm tiết. Không có trong set → **lỗi chắc chắn**. Đây là lớp duy nhất được phép auto-fix.

Build từ `data/phonology/`: âm đầu × vần × thanh, áp ràng buộc thanh–vần (vần đóng bằng `p/t/c/ch` chỉ nhận sắc/nặng; nhóm vần không nhận âm đầu), rồi khử trùng lặp. **Phải tự re-derive và verify con số** — 17.974 là số tham chiếu, không phải chân lý; cross-check bằng cách quét corpus xem có âm tiết thật nào rơi ngoài set (nếu có → bảng ngữ âm thiếu).

### L2 · Word / compound dictionary

Từ tiếng Việt đa phần đa âm tiết, viết cách nhau bằng space (`sử dụng`, `hợp tác xã`). FST cho unigram âm tiết attested (~7k) + FST cho compound 2–4 âm tiết, kèm log-frequency. Phục vụ segmentation và ngưỡng "âm tiết hợp lệ nhưng chưa từng thấy".

### L3 · Candidate generation — trái tim domain của app

Ba nguồn candidate, **không dùng Levenshtein mù**:

1. **Confusion-set** (bảng thủ công, `data/confusion/`): thanh hỏi↔ngã, âm đầu `s/x` `ch/tr` `r/d/gi` `l/n` `v/d`, âm cuối `n/ng` `t/c` `nh/n`, nguyên âm `i/y` `iê/ia` `ươ/ưa` `uô/ua`. Mục tiêu ~500–1000 cặp từ thật hay nhầm được curate tay.
2. **Strip-diacritic index**: map dạng không dấu → tất cả dạng có dấu (`se` → `se sẻ sẽ sè sé sẹ`). Dùng cho thêm dấu.
3. **Keyboard-adjacency + Telex/VNI typo**: fallback cho non-word không match confusion-set (`ddi`→`đi`, `tieengs`→`tiếng`).

### L4 · Viterbi decode với n-gram LM

3-gram LM cấp âm tiết, Kneser-Ney pruned, log-prob lượng tử hóa u16, memory-mapped (~10–30MB). Beam search chọn chuỗi candidate tốt nhất.

Quy tắc phán quyết:
- **Non-word** (L1 fail): luôn báo. Nếu đúng 1 candidate + tần suất cao → đủ điều kiện auto-fix.
- **Real-word** (L1 pass, LM thấp): chỉ báo khi `log P(candidate) − log P(observed) > θ`, `θ` cao. Đây là nguồn false positive chính → tuning bằng eval harness, không bằng cảm giác.

### L5 · Rule engine (deterministic)

Space đôi, space trước `, . ! ?`, thiếu space sau dấu phẩy, hoa đầu câu, hoa sau `.`, ngoặc/ngoặc kép không cân, `...` vs `…`, gạch nối vs gạch ngang, thiếu dấu câu cuối câu.

### L6 · AI adapter (Phase 6, opt-in, mặc định TẮT)

BYO Claude API key. Dùng cho ngữ pháp phức tạp + rewrite (`Writing Suggestions`, `AI Rewrite`). Chỉ gửi khi user **chủ động** bấm, không bao giờ gửi tự động. Hiển thị rõ ranh giới offline/online trong UI — đây là điểm bán hàng, không phải chi tiết kỹ thuật.

---

## Data pipeline — permissive license

Vì đã chốt MIT/Apache, **không được dùng `hunspell-vi` / Free Vietnamese Dictionary (GPLv3)**. Đường đi thay thế:

| Artifact | Nguồn | License |
|---|---|---|
| `syllables.fst` (17.974) | **Tự sinh** từ bảng ngữ âm ta tự viết | Của mình |
| `confusion/*.toml` | **Tự curate tay** | Của mình |
| `words.fst` + tần suất | Đếm thống kê từ Wikipedia tiếng Việt dump | CC BY-SA — attribution trong `THIRDPARTY.md` |
| `ngram.bin` | 3-gram từ viwiki + OSCAR/CC-100 vi | Kiểm tra kỹ từng nguồn trước khi dùng |
| `eval/testset.jsonl` | Synthetic (inject lỗi) + ~500 câu gán nhãn tay | Của mình |

**Lưu ý pháp lý:** thống kê tần suất và n-gram counts phái sinh nhìn chung là facts, nhưng để an toàn tuyệt đối thì (a) chỉ ship counts/probabilities, **không ship câu gốc**, (b) attribution đầy đủ trong `THIRDPARTY.md`, (c) verify license từng corpus **trước** khi đưa vào pipeline, không phải sau.

---

## Windows integration — chi tiết kỹ thuật

### Tier 1 · Hotkey trên selection (universal, ship trước)

1. `tauri-plugin-global-shortcut` bắt hotkey (mặc định `Ctrl+Alt+V`).
2. Đọc selection: UIA `GetFocusedElement` → `TextPattern::GetSelection()` → `GetText()`. Fallback: `SendInput(Ctrl+C)` → đọc clipboard → **restore clipboard cũ**.
3. Engine check → popup gợi ý.
4. Accept → ghi lại: `SendInput(KEYEVENTF_UNICODE)` hoặc set clipboard + `Ctrl+V` (nhanh hơn cho text dài, phải restore clipboard).

Tier này **không cần keyboard hook** → không bị antivirus nghi ngờ, chạy ở mọi app kể cả app vẽ text tùy biến. Đây là đường lùi an toàn nếu Tier 2 thất bại.

### Tier 2 · Real-time (chỉ làm sau khi P0 spike xác nhận khả thi)

- `SetWindowsHookExW(WH_KEYBOARD_LL)` — hook chạy trong process ta, **bắt buộc phải có message loop** trên thread hook.
- **Xử lý xung đột IME** — điểm chết của cả feature: Unikey/EVKey cũng dùng low-level hook, chúng **swallow** phím gốc rồi **inject** ký tự đã compose. Phím inject có `KBDLLHOOKSTRUCT.flags & LLKHF_INJECTED`. Chiến lược: **chỉ tin phím injected khi phát hiện có IME đang chạy**, coi đó là nguồn sự thật cho word buffer. Nếu spike cho thấy không đáng tin → chuyển sang **poll text từ UIA** khi caret đứng yên >300ms thay vì reconstruct từ keystroke.
- **Caret locator** — chain fallback, không có API nào chạy mọi nơi:
  1. `GetGUIThreadInfo` → `rcCaret` + `ClientToScreen` (Win32 edit, Notepad, Word)
  2. UIA `TextPattern2::GetCaretRange` → `GetBoundingRectangles` (Chrome, Electron, app modern)
  3. UIA `TextPattern::GetSelection` → bounding rect
  4. Cuối cùng: neo popup theo vị trí chuột / góc cửa sổ focus
- **Overlay window**: Tauri window `transparent: true`, `decorations: false`, `always_on_top: true`, `skip_taskbar: true`, và **bắt buộc `WS_EX_NOACTIVATE`** — overlay lấy focus là mất caret ở app đích, feature chết ngay. Pre-create + hide sẵn để hiện tức thì, chỉ reposition + update content.
- **Accept**: `Tab` → `SendInput(VK_BACK × n)` + `SendInput(KEYEVENTF_UNICODE, text)`. `Esc` → dismiss. Không dùng click làm interaction chính (click sẽ đổi focus).
- **Auto-fix an toàn**: chỉ áp cho lỗi L1 non-word + 1 candidate + tần suất cao, và **chỉ khi user vừa gõ space/dấu câu** (tức từ đã xong), kèm toast "đã sửa `nghành`→`ngành` · Ctrl+Z để hoàn tác".

### AppContext — biết khi nào phải im lặng

- Lấy exe name của foreground window → tra app profile.
- **Bỏ qua tuyệt đối**: UIA `IsPasswordPropertyId == true`, Win32 style `ES_PASSWORD`, blocklist mặc định (password manager, banking, terminal, RDP).
- Per-app profile: mạnh tay ở Zalo/Messenger/Word, chỉ-check-comment ở IDE, tắt hẳn ở terminal.

---

## Phase breakdown

### P0 · Spike & de-risk — 3–5 ngày · KHÔNG viết UI

Phase quan trọng nhất. Mọi rủi ro kỹ thuật của dự án nằm ở đây, phải trả lời trước khi đầu tư vào engine. Mỗi spike là một binary Rust nhỏ, throwaway.

| # | Spike | Câu hỏi phải trả lời |
|---|---|---|
| 1 | Tauri overlay | Hiện transparent, always-on-top, **không steal focus**, reposition tự do được không? |
| 2 | Đọc selection | UIA `GetSelection` chạy ở app nào? Test: Notepad, Word, Chrome (Gmail + textarea Facebook), Zalo PC, Teams, VSCode |
| 3 | Ghi text | `SendInput` UNICODE ghi được `ế`, `ữ` vào cả 6 app đó? Clipboard restore có sạch? |
| 4 | Caret position | Chain 4 fallback thực tế phủ được bao nhiêu %? |
| 5 | **IME coexistence** | **Hook thấy đúng gì khi Unikey/EVKey compose `tieengs`→`tiếng`?** Rủi ro cao nhất |
| 6 | Antivirus | Build unsigned có bị Defender/SmartScreen flag không? |

**Deliverable:** `SPIKE_RESULTS.md` — bảng compatibility matrix theo app × API. **Đây là go/no-go gate cho Tier 2.** Nếu spike 5 fail → Tier 2 chuyển sang cơ chế UIA-polling, hoặc app chỉ có Tier 1 (vẫn là sản phẩm dùng được).

### P1 · Engine core + data — 1.5–2 tuần

Không UI, không Windows. Chỉ `writa-core` + `writa-cli` + `cargo test`.

1. Viết `data/phonology/` (âm đầu, vần, ràng buộc thanh) → sinh + verify `syllables.fst`.
2. L0 normalize + tokenize + protected spans (test kỹ — lớp này rẻ mà sai thì hỏng hết).
3. L1 syllable validity + L2 dict từ corpus.
4. L3 confusion-set (bắt đầu từ hỏi/ngã + top 200 cặp hay nhầm).
5. L4 3-gram LM + Viterbi.
6. **Eval harness** — làm song song từ đầu, không để cuối: `writa-cli eval` in ra precision/recall/F0.5/FP-per-1000w/latency p50-p99.

**Exit gate:** trên testset — non-word recall >95%, FP rate <2/1000 từ, p99 latency <5ms/từ.

### P2 · Tier 1 shell — 1 tuần

Tray icon, settings window (React), global hotkey, popup gợi ý, personal dictionary, autostart.
**Kết thúc P2 là đã có app dùng được thật** — mốc quan trọng, ship internal build để tự dogfood hàng ngày.

### P3 · Thêm dấu tự động — 4–5 ngày

`diacritic.rs` + strip-diacritic index, dùng lại toàn bộ `lm.rs`. Hotkey riêng (`Ctrl+Alt+D`) cho selection không dấu. Eval riêng: accuracy per-syllable trên corpus held-out đã strip dấu (mục tiêu >95%).

### P4 · Tier 2 real-time — 2 tuần (phụ thuộc P0 gate)

Keyboard hook + word buffer + caret locator + overlay inline + accept/replace + auto-fix an toàn + app profile + password guard.

### P5 · Rules + profiles hoàn chỉnh — 1 tuần

L5 rule engine đầy đủ, biến thể vùng miền (Bắc/Trung/Nam), style vị trí dấu (`hòa`/`hoà`), per-app profile UI.

### P6 · AI layer + insights — 1 tuần

BYO Claude key (`claude-sonnet-5` cho grammar/rewrite). Insights dashboard: lỗi theo loại/thời gian, top lỗi cá nhân.

### P7 · Ship — 1 tuần

MSI/NSIS installer, **code signing** (xem Risks), auto-update, onboarding, `THIRDPARTY.md`, `LICENSE`, README có GIF demo.

---

## Feature roadmap

**Roadmap gốc, đã map vào phase:**

```
Typo Detection ────────── P1 (L1 + L3 keyboard-adjacency)
Vietnamese Spelling ───── P1 (L1 + L2 + L3 + L4)
Punctuation ───────────── P5 (L5)
Grammar ───────────────── P5 rule-based · P6 AI
Writing Suggestions ───── P6
AI Rewrite ────────────── P6
```

**Bổ sung — sắp theo tỉ lệ giá trị/chi phí:**

*Gần như miễn phí vì dùng lại hạ tầng đã có:*
- **Thêm dấu tự động** → đã đưa vào MVP (P3). Feature "wow" để demo.
- **CLI + pre-commit hook / GitHub Action** — `writa check *.md`. Chỉ là wrapper quanh `writa-cli`.
- **Web demo bằng WASM** — build `writa-core` sang WASM, deploy static. Marketing gần như free.
- **Text expansion / snippets** — hạ tầng hook + SendInput đã có sẵn từ P4.
- **Batch document check** — kéo thả `.txt/.md/.srt/.docx` → báo cáo lỗi.

*Đòn bẩy hệ sinh thái sẵn có:*
- **VSCode extension companion** — `writa-core` → WASM → extension gạch đỏ lỗi tiếng Việt trong comment/markdown/string. Kênh phân phối sẵn có, và là tier **đáng tin cậy nhất** (VSCode cho full text API, không cần hook).

*Tạo độ dính (retention):*
- **Insights dashboard** — "tuần này bạn sai hỏi/ngã 47 lần". Kiểu Grammarly weekly report.
- **Learning mode** — biến lỗi của chính user thành flashcard luyện hỏi/ngã. Rất mạnh với học sinh/sinh viên, không đối thủ nào có.
- **Personal dictionary học dần** — tên riêng, thuật ngữ, tên công ty.

*Mở đường B2B:*
- **Team dictionary** — thuật ngữ dùng chung, đồng bộ qua file/git.
- **Style guide doanh nghiệp** — văn phong hành chính vs thân mật, phát hiện teencode, từ lặp thừa (`hết sức vô cùng`), lạm dụng từ vay mượn.

*Chất lượng viết:*
- **Readability score cho tiếng Việt** — độ dài câu, mật độ từ Hán-Việt.
- **Read-aloud (TTS)** — đọc lại câu để tự nghe ra lỗi.

*Mở rộng ngôn ngữ:*
- `trait LanguageEngine` trong `writa-core/src/lang/` **thiết kế ngay từ P1**, dù MVP chỉ có tiếng Việt. Retrofit abstraction sau tốn gấp nhiều lần. Tiếng Anh (dictionary + LM tương tự), tiếng Nhật (cần morphological analyzer, phức tạp hơn hẳn).
- **macOS**: `CGEventTap` + Accessibility API, cần user cấp quyền + Apple Developer ID.

---

## Privacy & Security — không phải mục phụ

App này **về mặt kỹ thuật là một keylogger**. Nếu không xử lý đàng hoàng thì vừa mất niềm tin user vừa bị antivirus chặn. Đây là hạng mục sống còn:

1. **100% offline mặc định.** Zero network trong toàn bộ P1–P5. Chứng minh được bằng `netstat` — và nói rõ điều đó trong README.
2. **Không persist text.** Buffer chỉ giữ từ/câu hiện tại trong RAM, zero-out sau khi xử lý. Không log, không file tạm, không telemetry.
3. **Password guard nhiều lớp** — UIA `IsPassword`, Win32 `ES_PASSWORD`, blocklist app mặc định.
4. **Pause dễ thấy** — tray toggle + hotkey pause, icon đổi trạng thái rõ ràng.
5. **Open-source (MIT) là tính năng bảo mật** — user audit được. Đây chính là lợi thế lớn nhất trước một tool closed-source cùng loại.
6. **Statistics chỉ lưu counts**, không lưu nội dung, lưu local, xóa được.

---

## Risks & mitigation

| # | Rủi ro | Mức | Xử lý |
|---|---|---|---|
| 1 | **Xung đột IME (Unikey/EVKey)** — hai low-level hook tranh keystroke | **CAO** | P0 spike 5 quyết định trước mọi đầu tư. Plan A: dùng `LLKHF_INJECTED`. Plan B: UIA-polling khi caret đứng yên. Plan C: chỉ Tier 1 |
| 2 | **Antivirus / SmartScreen flag** — global hook + SendInput = signature keylogger | **CAO** | Code signing (OV cert + build reputation; EV nếu có ngân sách). Không obfuscate. Zero network → dễ giải trình. Submit false-positive report tới Defender. Open-source giúp trust. Tier 1 không cần hook nên miễn nhiễm |
| 3 | **False positive giết niềm tin** | **CAO** | Eval gate trong CI. Precision-first thresholds. Protected spans. "Ignore / thêm vào từ điển" 1 click |
| 4 | Overlay steal focus → mất caret app đích | TB | `WS_EX_NOACTIVATE`, không focus overlay bao giờ, interaction qua key không qua click |
| 5 | Caret position fail ở một số app | TB | Chain 4 fallback; cuối cùng neo theo chuột. P0 spike 4 đo được % phủ thật |
| 6 | Corpus license không rõ ràng | TB | Verify license **trước** khi vào pipeline. Chỉ ship counts không ship câu gốc. `THIRDPARTY.md` đầy đủ |
| 7 | Chất lượng LM không đủ cho lỗi real-word | TB | Eval sớm từ P1. Nếu 3-gram không đủ → lên 4-gram hoặc thêm ONNX classifier nhỏ **chỉ cho** hỏi/ngã (bài toán binary, model rất nhỏ) |
| 8 | Latency real-time | THẤP | Rust + FST + mmap. Benchmark trong CI |

---

## Verification

**Engine (tự động, chạy trong CI):**
```bash
cargo test -p writa-core                        # unit test từng lớp
cargo run -p writa-cli -- eval data/eval/testset.jsonl
#   → precision / recall / F0.5 / FP-per-1000w / p50-p99 latency
cargo run -p writa-cli -- eval-diacritic data/eval/diacritic.jsonl
cargo bench -p writa-core                       # gate latency
```

Testset gồm hai phần, **cả hai bắt buộc**:
- **Synthetic** — lấy câu đúng từ corpus held-out, inject lỗi theo phân bố thực (bỏ dấu, swap hỏi/ngã, swap s/x, typo phím kề).
- **Real** — ~500 câu thu từ comment/forum, gán nhãn tay.

Synthetic một mình sẽ cho số liệu lạc quan giả.

**Windows integration (manual, có checklist):**

`SPIKE_RESULTS.md` giữ compatibility matrix, test lại mỗi phase trên 10+ app: Notepad, WordPad, Word, Excel, Chrome (Gmail / textarea Facebook), Edge, Zalo PC, Teams, Slack, VSCode, Telegram. Mỗi app × {đọc selection, ghi text, caret position, real-time hook} → ✅/⚠️/❌.

**Dogfooding — nguồn tín hiệu chất lượng thật:**

Từ hết P2, dùng app hàng ngày. Số lần tự bật/tắt và số lỗi tự "ignore" là hai chỉ số trung thực nhất, không có eval nào thay được.

**Manual E2E:**
```bash
npm run tauri dev        # dev
npm run tauri build      # installer
```

---

## Định nghĩa thành công của MVP

| Chỉ tiêu | Mục tiêu |
|---|---|
| Phát hiện lỗi non-word (âm tiết bất hợp lệ) | recall > 95%, precision ~100% |
| False positive rate | **< 2 / 1000 từ** ← chỉ tiêu quan trọng nhất |
| Thêm dấu, accuracy per-syllable | > 95% |
| Latency p99 (Tier 2, mỗi từ) | < 5ms |
| Phủ app (đọc selection + ghi text) | ≥ 8/10 app trong checklist |
| RAM idle | < 80MB |
| Installer | < 20MB |
| Network traffic khi AI tắt | **0 byte** |

---

## Thứ tự thực thi

```
P0 spike (3-5d) ──► GATE: Tier 2 khả thi?
   │
   ├─ P1 engine + data (1.5-2w) ──► GATE: FP < 2/1000
   │     │
   │     ├─ P2 Tier 1 shell (1w) ──► ⭐ APP DÙNG ĐƯỢC, bắt đầu dogfood
   │     │     │
   │     │     ├─ P3 thêm dấu (4-5d) ──► ⭐ MVP HOÀN CHỈNH
   │     │     │     │
   │     │     │     ├─ P4 Tier 2 real-time (2w)
   │     │     │     │     ├─ P5 rules + profiles (1w)
   │     │     │     │     │     ├─ P6 AI + insights (1w)
   │     │     │     │     │     │     └─ P7 ship (1w)
```

MVP hoàn chỉnh (hết P3): **~4–5 tuần**. Full v1 (hết P7): **~9–11 tuần**.

---

## Tham khảo

- [All syllables in Vietnamese language — Lương Hiếu Thi](https://www.hieuthi.com/blog/2017/03/21/all-vietnamese-syllables.html) — cơ sở cho tập 17.974 âm tiết
- [Vietnamese phonology — Wikipedia](https://en.wikipedia.org/wiki/Vietnamese_phonology) — bảng âm đầu/vần/thanh
- [hunspell-vi](https://github.com/1ec5/hunspell-vi) — GPLv3, **không dùng**, chỉ tham khảo cách tiếp cận
- [UI Automation TextPattern Overview — Microsoft Learn](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-textpattern-overview) — API đọc text/caret
- [ViSoLex: Vietnamese Social Media Lexical Normalization](https://arxiv.org/html/2501.07020v1) — tham khảo cho lớp AI về sau
