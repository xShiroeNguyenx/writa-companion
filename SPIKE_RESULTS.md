# P0 — Kết quả spike

Bảng compatibility matrix của Writa. **File này là go/no-go gate cho Tier 2** (real-time).
Cập nhật lại mỗi phase; đừng để nó cũ.

Trạng thái: `⬜ chưa chạy` · `✅ được` · `⚠️ được nhưng có điều kiện` · `❌ không được`

---

## Spike 5 — IME coexistence ✅ ĐÃ TRẢ LỜI (2026-08-12) — **GO**

> Phần chi tiết bên dưới giữ nguyên làm bối cảnh. Đây là kết luận.

Đo trên UniKey Telex, 163 sự kiện keydown thật, gõ câu
`nghành công nghệ thông tin là nghành sẻ bị đàu thải sớm thôi`.

**Giả thuyết ban đầu sai.** Plan A giả định "bộ gõ nuốt hết phím gốc, chỉ tin phím
bơm". Thực tế UniKey để **phần lớn** phím gốc đi thẳng vào ô nhập, và chỉ nuốt đúng
**một** phím mỗi lần ghép: phím kích hoạt.

```
[13612ms] DOWN 'f'                       ← vật lý; hook thấy, ô nhập KHÔNG nhận
[13612ms] DOWN <BACKSPACE>  INJECTED     ← cùng mili-giây
[13614ms] DOWN <BACKSPACE>  INJECTED
[13615ms] DOWN <BACKSPACE>  INJECTED
[13616ms] DOWN 'à' INJECTED → 'n' → 'h'  ← qua VK_PACKET
```

Vì thế cả 6 chiến lược của probe đều trượt: "chỉ injected" chỉ được mấy mảnh vụn
(`ànhôngẹệôn…`), còn "tất cả + backspace" thì thừa đúng một ký tự mỗi lần ghép
(`coông`, `nghaành`, `ngheẹệ`).

**Mô hình đúng: tin cả hai luồng, trừ đi phím bị nuốt.** Phím bị nuốt là phím vật lý
sinh-ký-tự cuối cùng ngay trước một loạt sự kiện bơm. Kiểm lại trên toàn bộ 163 sự
kiện:

| Chiến lược | Kết quả |
|---|---|
| Tất cả + xử lý backspace | `nghaành coông ngheẹệ … thoôi` ❌ |
| Chỉ INJECTED | `ànhôngẹệôn…` ❌ |
| **Trừ phím bị nuốt (cửa sổ ≤ 1 ms)** | **khớp tuyệt đối** ✅ |
| Trừ phím bị nuốt (2 / 5 / 20 / 60 ms) | khớp tuyệt đối ✅ |

Cửa sổ 1 ms tới 60 ms cho **cùng một** kết quả, nên ranh giới không mong manh.
Chốt ở 20 ms: rộng gấp hai chục lần độ trễ đo được, vẫn hẹp hơn nhịp gõ nhanh nhất
của người (~60 ms/ký tự ở 200 từ/phút).

**Đã sửa vào code:** `buffer.rs` bỏ hẳn chế độ "chỉ tin phím bơm"; `hook.rs` chèn một
`Backspace` bù khi phát hiện phím bị nuốt. Hai test dùng **đúng chuỗi sự kiện thật**
lấy từ `ime-probe.log` (`nghanhf` → `nghành`, và `daudf` → `đàu` với hai lần ghép
trong một từ).

**Rủi ro còn lại — thứ tự chuỗi hook.** Windows gọi hook cài **sau cùng trước tiên**.
Cách này chạy được vì Writa cài hook sau UniKey (UniKey khởi động cùng máy), nên ta
thấy phím trước khi UniKey nuốt. Nếu user khởi động lại UniKey **sau** Writa thì thứ
tự đảo, ta chỉ còn thấy luồng bơm, và bộ đệm sẽ ra mảnh vụn. Nhận ra được — thấy loạt
`VK_PACKET` mà không có phím vật lý đi trước — nhưng chưa xử lý.

---

## Spike 5 — bối cảnh và cách chạy lại

Đây là spike duy nhất có thể **đổi hình dạng sản phẩm**. Bốn spike còn lại đều có
đường lùi rõ ràng; spike này thì không.

### Cách chạy

```bash
cargo run -p ime-probe --release -- 40
```

Trong 40 giây đó:
1. Mở Notepad.
2. Đảm bảo **UniKey đang bật**, kiểu gõ Telex.
3. Gõ vài từ tiếng Việt, ví dụ `tieengs Vieejt`, `xin chaof`.
4. **Ghi lại chính xác text hiện ra trong Notepad** — đây là ground truth để so.
5. Nhấn `F12` nếu muốn dừng sớm.

Chương trình in ra 6 chiến lược reconstruct + ghi `ime-probe.log`.

### Kết quả

| Hạng mục | Giá trị |
|---|---|
| Text gõ thật (ground truth) | *chưa điền* |
| Số event keydown | |
| Trong đó INJECTED | |
| Số VK_PACKET | |
| Số Backspace (injected) | |
| `dwExtraInfo` khác 0 gặp được | |
| **Chiến lược khớp ground truth** | |

### Phán quyết

| Nếu… | Thì |
|---|---|
| Chiến lược **F** (chỉ INJECTED + xử lý BS) khớp | ✅ **PLAN A** — tin phím injected làm nguồn sự thật cho word buffer. Tier 2 làm như PLAN.md. |
| `VK_PACKET = 0` và không có event INJECTED | ⚠️ **PLAN B** — UniKey ghi qua WM_CHAR/TSF, hook không thấy. Chuyển sang poll text từ UIA khi caret đứng yên >300ms. |
| Không chiến lược nào khớp | ⚠️ **PLAN B**, hoặc **PLAN C** nếu UIA cũng không đủ (chỉ làm Tier 1 hotkey) |
| `dwExtraInfo` có giá trị khác 0 lặp đều | 🎯 Đó là chữ ký UniKey — dùng nó lọc chính xác hơn cờ INJECTED |

**Kết luận:** ⬜ *chưa chạy*

---

## Spike 1 — Tauri overlay 🟡 một phần, và câu hỏi đã đổi

P2 dựng xong popup nên phần lớn hạng mục này trả lời được luôn — **trừ hạng mục
quan trọng nhất**, vì Tier 1 hoá ra không cần nó.

| Hạng mục | Kết quả |
|---|---|
| Frameless + bo góc + always-on-top | ✅ Tauri 2 `decorations:false`, `alwaysOnTop:true` |
| Reposition khi đang hiện | ✅ `set_position` theo toạ độ vật lý, kẹp theo màn hình chứa caret |
| Tạo sẵn rồi ẩn, hiện lại tức thì | ✅ ba cửa sổ tạo lúc khởi động, `visible:false` |
| **Không steal focus (`WS_EX_NOACTIVATE`)** | ✅ **đã đo** — overlay inline của Tier 2, xem bên dưới |
| Transparent thật (nền trong suốt) | ✅ overlay inline dùng `transparent:true` |

### `WS_EX_NOACTIVATE` — đo được (2026-08-12)

Cần **hai** mảnh, thiếu một là hỏng:

- `WS_EX_NOACTIVATE` + `WS_EX_TOOLWINDOW` trên ex-style (cái sau giữ overlay khỏi Alt+Tab).
- `ShowWindow(SW_SHOWNOACTIVATE)`. `Window::show()` của Tauri gọi `SW_SHOW`, và
  `SW_SHOW` **vẫn activate** dù có cờ trên — nên phải đi đường Win32 riêng.

Đo end-to-end: bơm `Toi lam trong nghanh ` vào một app đang focus, overlay hiện sau
**0 ms** ở `pos=3512,61 size=224x31`, và luồng bơm phím tiếp tục vào app đích không
gián đoạn — tức là focus không hề chuyển.

```
nghanh → nganh   [Ctrl+Alt+Space]
```

Hệ quả kiến trúc: overlay **không nhận được phím**. Vì vậy mọi thao tác của Tier 2 đi
qua phím tắt toàn cục (`Ctrl+Alt+Space` để áp dụng) và qua hook (Esc / click / mũi
tên để bỏ qua), không qua bàn phím của cửa sổ đó.

### Vì sao Tier 1 cố tình để popup lấy focus

PLAN.md coi `WS_EX_NOACTIVATE` là bắt buộc. Điều đó đúng cho **overlay inline của
Tier 2**, nơi user đang gõ dở và mất caret là chết. Nhưng Tier 1 khác hẳn: user đã
bôi đen xong mới bấm phím tắt, và việc tiếp theo họ cần làm là *đọc danh sách và
chọn*. Một cửa sổ không nhận được phím thì không có `Enter` để áp dụng, không có
`Esc` để đóng, không chọn được phương án khác trong dropdown.

Và có một hệ quả ngược đời nhưng có lợi: Windows chỉ cho `SetForegroundWindow` khi
process gọi **đang** là foreground. Popup lấy focus chính là thứ khiến bước trả
focus về app đích lúc ghi ngược trở nên hợp lệ.

Nên `WS_EX_NOACTIVATE` chuyển thành câu hỏi của **P4**, không phải của P2.

---

## Rủi ro mới do P2 sinh ra ⬜ CHƯA ĐO

Cách ghi ngược của Tier 1 dựa trên một giả định chưa được kiểm chứng trên app thật:

> **Vùng chọn ở app đích vẫn còn sau khi cửa sổ đó mất focus rồi được focus lại.**

Nếu giả định sai, `Ctrl+V` sẽ **chèn thêm** thay vì ghi đè — tức là nhân đôi đoạn
text của user. Đây là kiểu hỏng tệ nhất trong cả app: nó phá dữ liệu chứ không chỉ
báo sai.

Đã dựng hai lớp chắn, nhưng không lớp nào thay được phép đo:

1. Trước khi ghi, đọc lại vùng chọn qua UIA và so với bản đã kiểm tra. Khác thì
   **không ghi** và bảo user dùng nút *Chép*. Chỉ chặn được app mà UIA đọc được.
2. Nút *Chép* luôn có mặt như đường lùi thủ công.

| App | Giữ vùng chọn khi mất focus? | Ghi đè đúng? |
|---|---|---|
| Notepad | ⬜ | ⬜ |
| MS Word | ⬜ | ⬜ |
| Chrome — textarea | ⬜ | ⬜ |
| Zalo PC | ⬜ | ⬜ |
| MS Teams | ⬜ | ⬜ |
| VS Code | ⬜ | ⬜ |

**Cách đo:** bôi đen một câu có lỗi, bấm `Ctrl+Alt+V`, bấm *Áp dụng*, xem text bị
thay hay bị nhân đôi.

---

## Spike 2/3/4 — Compatibility matrix theo app ⬜

Ba spike này đo trên cùng một tập app nên gộp một bảng.

- **Đọc selection**: UIA `GetFocusedElement` → `TextPattern::GetSelection()` → `GetText()`
- **Ghi text**: `SendInput` với `KEYEVENTF_UNICODE` (thử ký tự `ế`, `ữ`)
- **Caret position**: chain 4 fallback — `GetGUIThreadInfo` → `TextPattern2::GetCaretRange` → `TextPattern::GetSelection` → neo theo chuột

**Cách đo:** `cargo run -p writa-win --bin win-probe --release` (thêm `--write` để đo cả
việc ghi text). Có 5 giây để chuyển sang app cần đo và bôi đen một đoạn.

| App | Ngữ cảnh | Đọc selection | Ghi text | Caret | Ghi chú |
|---|---|---|---|---|---|
| **Chrome — Google Sheets** | ✅ | ✅ clipboard | ⬜ | ✅ `UiaSelection` (1057,959) h=18 | UIA đọc được `IsPassword`; app tự vẽ mà vẫn ra caret chính xác |
| **Zalo PC** | ✅ | ✅ | ✅ | ✅ | user xác nhận tay 2026-08-12: Tier 1 *Áp dụng* thay đè đúng |
| **VS Code** | ✅ | ✅ | ✅ | ✅ | user xác nhận tay 2026-08-12 |
| **Chrome — Google Chat** | ✅ | ✅ | ✅ | ✅ | user xác nhận tay 2026-08-12 |
| Notepad | ⬜ | ⬜ | ⬜ | ⬜ | |
| MS Word | ⬜ | ⬜ | ⬜ | ⬜ | |
| Excel | ⬜ | ⬜ | ⬜ | ⬜ | |
| Chrome — Gmail | ⬜ | ⬜ | ⬜ | ⬜ | |
| Chrome — textarea Facebook | ⬜ | ⬜ | ⬜ | ⬜ | |
| Edge | ⬜ | ⬜ | ⬜ | ⬜ | |
| MS Teams | ⬜ | ⬜ | ⬜ | ⬜ | |
| Slack | ⬜ | ⬜ | ⬜ | ⬜ | |
| Telegram | ⬜ | ⬜ | ⬜ | ⬜ | |

### Ghi ngược đã được xác nhận (2026-08-12) — rủi ro lớn nhất của Tier 1 đã đóng

Giả định "vùng chọn vẫn còn sau khi app đích mất focus rồi được focus lại" — thứ mà nếu
sai thì `Ctrl+V` **chèn thêm** chứ không ghi đè, tức là nhân đôi text của user — **đã
được kiểm bằng tay và đúng** ở Zalo PC, VS Code và Google Chat trong Chrome. Ba app đó
phủ ba tầng render khác nhau (Electron, Electron, Chromium web), nên độ tin cậy khá tốt.

Tier 2 `Tab` cũng đã xác nhận: thay đè đúng, không sinh từ mới. Đây là chỗ mà lỗi
"phím phụ còn bị giữ" từng làm hỏng — xem mục về `release_modifiers` bên dưới.

**Chrome là ca khó nhất và nó chạy được cả bốn.** Chrome tự vẽ ô nhập nên không lộ
`ES_PASSWORD` và không có control Edit chuẩn — nếu app này qua được thì phần lớn app
modern (Electron, Teams, Slack, VS Code) nhiều khả năng cũng qua, vì chúng dùng cùng
tầng UIA. Notepad và Word thì đi đường `GetGUIThreadInfo` còn rẻ hơn.

**Ngưỡng MVP:** ≥ 8/12 app cho *đọc selection* và *ghi text*.
Caret position được phép phủ thấp hơn vì có fallback neo theo chuột.

---

## Spike 6 — Antivirus / SmartScreen 🟡 đã đo phần lớn (2026-08-12)

Installer: `npm run app:build` → NSIS, **3,60 MB** (mục tiêu < 20 MB ✅). Ruột đã kiểm
bằng `7z l`: đúng một `writa-app.exe` (16,04 MB, nén còn 3,7 MB nhờ lexicon TSV) cộng
plugin NSIS. Định dạng NSIS-3 Unicode.

| Hạng mục | Kết quả |
|---|---|
| Dựng được installer | ✅ 3,60 MB |
| Chữ ký số (exe + installer) | ❌ `NotSigned` |
| **ESET quét tĩnh** | ✅ **0 phát hiện** / 2 file, 21 object |
| **ESET real-time chặn lúc chạy?** | ✅ **không** — xem dưới |
| Windows Defender | ⬜ không đo được ở đây: ESET chiếm chỗ nên Defender tự tắt |
| SmartScreen | ❌ **sẽ chặn** — xem dưới |

### Antivirus: qua được, và đây là phép đo thật

Máy đo có **ESET Security** làm AV chủ động (`productState 0x41000` → real-time bật,
định nghĩa cập nhật 2026-08-12). Quét bằng scanner dòng lệnh của chính ESET, bật hết các
module heuristic:

```
ecls.exe --unsafe --unwanted --suspicious --clean-mode=none  <installer> <exe>
→ Detected: files - 0, objects 0     (exit code 0)
```

Quan trọng hơn phép quét tĩnh: **app đã chạy nhiều giờ trên máy này với ESET real-time
bật** — cắm `WH_KEYBOARD_LL`, `WH_MOUSE_LL`, `SetWinEventHook`, và bơm phím bằng
`SendInput` — và không hề bị chặn hay cách ly. Đó là phép thử hành vi, chỉ là thụ động.

**Chưa đo được:** Windows Defender. Nó bị vô hiệu hoá vì ESET đã đăng ký làm AV chính,
nên `MpCmdRun -Scan` trả `0x80004005`. Cần một máy chỉ có Defender.

### SmartScreen: chắc chắn sẽ chặn, không phải phỏng đoán

Gán Mark-of-the-Web như thể vừa tải về (`Zone.Identifier`, `ZoneId=3`) và đối chiếu với
đúng hai thứ SmartScreen dựa vào:

| SmartScreen xét | Writa hiện tại |
|---|---|
| Chữ ký số / nhà phát hành | không có |
| Uy tín theo số lượt tải | bằng không (chưa phát hành) |
| Mark-of-the-Web | có, ngay khi user tải từ web |

Ba điều đó cộng lại là điều kiện đủ để Windows hiện **"Windows protected your PC"**, và
đường chạy tiếp bị giấu sau *More info → Run anyway*. Với một app có hình dạng keylogger
thì hộp thoại đó là lý do bỏ cài rất mạnh.

### Ngân sách code-signing — tra lại 2026-08-12, và hai giả định cũ đều SAI

Bảng đầu tiên ở đây (viết theo ký ức) sai hai chỗ quan trọng. Bản đã tra:

| Lựa chọn | Chi phí xấp xỉ | Thực tế |
|---|---|---|
| Không ký | 0 | "Unknown publisher" + SmartScreen, mãi mãi |
| **Azure Artifact Signing** *(tên mới của Trusted Signing, đổi 2026)* | ~10 USD/tháng | ❌ **Việt Nam không đủ điều kiện.** Doanh nghiệp: US/Canada/EU/UK. Cá nhân tự doanh: **chỉ US và Canada** |
| Cert OV | ~200–400 USD/năm | Hết "Unknown publisher". Uy tín SmartScreen **phải tích luỹ dần** |
| Cert EV | ~300–700 USD/năm + token | **Không** còn lợi thế SmartScreen nào so với OV |

**Sai lầm thứ nhất:** tôi từng ghi Azure Trusted Signing là "đường hợp lý nhất". Nó
không mở cho Việt Nam — cá nhân tự doanh chỉ US/Canada, doanh nghiệp thì US/Canada/EU/UK.

**Sai lầm thứ hai, quan trọng hơn:** tôi từng ghi cert EV cho "uy tín ngay lập tức". Đó
là hành vi **cũ**. Microsoft đã bỏ nó từ **tháng 3/2024**; nay app ký EV cũng phải tích
luỹ uy tín từ telemetry tải về y như app ký OV. Trả thêm tiền cho EV chỉ để tránh
SmartScreen không còn hợp lý — EV giờ chỉ cần cho driver kernel-mode hoặc khi khách
doanh nghiệp yêu cầu.

### Hệ quả cho chiến lược phát hành

Không có mức chi nào mua được việc "user đầu tiên không thấy cảnh báo". Nên kế hoạch
phải **tính cảnh báo đó vào trong**, không phải cố mua nó đi:

1. Ký bằng **OV** — đủ để hết "Unknown publisher", và bắt đầu đồng hồ tích luỹ uy tín.
2. **Ký liên tục bằng đúng một cert.** Đổi cert là đặt lại uy tín về 0.
3. Nói trước trong README và trong trang release: cảnh báo này là gì, vì sao có, cách
   đi qua. Một app có hình dạng keylogger mà im lặng về chuyện này thì mất niềm tin gấp
   đôi.
4. Cân nhắc **Microsoft Store** làm đường phát hành thứ hai — app từ Store không đi qua
   SmartScreen, nên nó là con đường duy nhất user mới không gặp cảnh báo nào.

Nguồn: [Microsoft Learn — code signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options),
[Microsoft Learn — SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation),
[ToDesktop — EV certs no longer grant immediate reputation](https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore).

**Chưa làm:** thử cài thật (`Program Files`, Start Menu, mục uninstall) — đó là thay đổi
hệ thống nên chờ user đồng ý.

---

## Đã xong ngoài spike

| Việc | Trạng thái |
|---|---|
| viwiki dump 1.09 GB | ✅ tải xong, verified byte-exact, magic `BZh` |
| Bảng ngữ âm `data/phonology/` | ✅ 27 âm đầu × 160 vần |
| L1 sinh tập âm tiết | ✅ 18.261 âm tiết |
| L0 chuẩn hoá + tách token + vùng bảo vệ | ✅ 10 loại vùng bảo vệ |
| `check()` nối L0 → L1 | ✅ 35 test pass, clippy sạch |
| Verify đối chiếu corpus | ✅ độ phủ **91,86%** token; **0,18%** token có dấu Việt bị loại |
| Âm tiết thực sự dùng trong corpus | ✅ 6.760 / 18.261 (khớp dự đoán ~7.000 của PLAN.md) |
| **False-positive rate** (token có dấu Việt) | ✅ **0,66 / 1000 từ** — dưới ngưỡng 2,00 |
| Confusion-set khởi tạo | ✅ `data/confusion/notes.md` — chờ Nguyễn Khánh review các dòng ⚠️ |

### L2 — từ vựng suy ra từ corpus

| Việc | Trạng thái |
|---|---|
| `dict.rs` + `build_lexicon.py` | ✅ 49 test pass |
| Corpus đầy đủ | ✅ 1,6 triệu bài · 231,7 triệu token · bỏ 628.654 trang ngoài bài viết |
| Âm tiết đã chứng thực | ✅ 9.352 (từ 18.261 sinh ra) |
| Từ ngoại / tên riêng / viết tắt được chấp nhận | ✅ 5.258 |
| Từ ghép 2 âm tiết | ✅ 154.092 |
| L0 chặn chữ ngoài hệ Latin | ✅ Hy Lạp, Kirin, Hán |

Diễn biến false-positive qua từng lớp:

| | có dấu Việt | ASCII thuần | ghi chú |
|---|---|---|---|
| L1 một mình | 0,66 | 20,52 | mẫu 120k câu |
| + L2 từ vựng corpus | 0,39 | 8,18 | mẫu 120k câu |
| + L0 chặn chữ ngoài Latin | 0,38 | 7,28 | mẫu 120k câu |
| + corpus đầy đủ, **đo held-out** | **0,25** | **6,40** | 50k câu chưa từng thấy |

**Phép đo held-out:** từ vựng dựng từ 450 nghìn câu, đo trên 50 nghìn câu tách
riêng **theo bài viết** (không tách ngẫu nhiên theo câu, vì câu trong cùng bài dùng
lại cùng vốn từ). Đo trên tập train cho 0,30 / 5,65 — chênh rất ít, nghĩa là danh
sách từ vay mượn tổng quát hoá được chứ không học vẹt.

**Phần đuôi nhóm "có dấu Việt" không hẳn là báo oan.** Lẫn trong đó có `vơí` (đúng
là `với` — dấu đặt sai vị trí) và `chiéc` (đúng là `chiếc` — thiếu dấu mũ): lỗi
chính tả **thật** của Wikipedia, đúng loại lỗi Writa sinh ra để bắt. Nghĩa là con
số 0,25 còn là ước lượng **bi quan** cho false-positive thật.

**Kích thước cần theo dõi:** `compounds.tsv` giờ 2,2 MB. Với native thì không vấn
đề gì, nhưng đây là lúc để mắt tới việc chuyển sang FST + mmap như PLAN.md — sẽ làm
khi L4 lên và biết yêu cầu kích thước thật.

### L3 — sinh candidate + phát hiện lỗi real-word

| Việc | Trạng thái |
|---|---|
| `phonology::decompose` / `compose` | ✅ phân tích khoan dung, dựng lại nghiêm ngặt |
| `candidate.rs` + `data/confusion/rules.tsv` | ✅ 76 test pass |
| Candidate kèm theo mỗi lỗi L1 | ✅ `nghành` → `ngành`, `ngiên` → `nghiên` |
| Phục hồi Telex khi bộ gõ tắt | ✅ `chinhs` → `chính`, `tieengs` → `tiếng` |
| **Phát hiện lỗi real-word** | ✅ `chia sẽ` → `sẻ`, `xử dụng` → `sử` |

Diễn biến false-positive của lớp real-word — bốn lượt đo, mỗi lượt sửa một lỗ:

| | real-word / 1000 từ |
|---|---|
| Tần suất bigram thô | 3,46 |
| + loại biến thể i/y đều đúng | 3,21 |
| + cổng collocation một chiều `P(b\|a)` | 0,69 |
| + cổng collocation **hai chiều** `min(P(b\|a), P(a\|b))` | **0,20** |

**Vì sao cần cả hai chiều.** Tần suất bigram thô báo `cát → các`, `dùng → vùng`,
`hộ → họ` — toàn cặp hai từ đều đúng — vì tổ hợp thay thế *có tần suất cao*, mà cao
chỉ vì cả hai từ đều phổ biến. Cổng một chiều bịt phần lớn chỗ đó, nhưng **từ chức
năng** (`của`, `có`) vẫn lọt: chúng đi sau rất nhiều từ với xác suất cao, nên
`P(của|X)` lớn với hầu hết `X`. Chiều ngược lại phơi bày ngay — `P(X|của)` bé tí vì
`của` đứng cạnh hàng nghìn từ. Từ ghép cố định thật thì chặt cả hai chiều.

Để tính được tỉ số này phải thêm **cột tần suất đếm trên chính tập câu đã dựng
`compounds.tsv`** vào `syllables.tsv`. Cột tần suất trên trọn dump không dùng được:
tử số và mẫu số khác mẫu thì tỉ số vô nghĩa.

**Một sai lầm đáng ghi lại:** tôi định làm phát hiện real-word bằng tần suất bigram
của L2, tức đi trước L4 mà PLAN.md xếp cho việc này. Lượt đo đầu cho 3,46/1000 với
precision kém, và bài học là tần suất bigram thô **không phải mô hình ngôn ngữ** —
`observed = 0` không nghĩa "không thể" mà chỉ nghĩa "không nằm trong top 154k". Cổng
độ chặt hai chiều đưa được về 0,20/1000, nhưng nó vẫn là xấp xỉ; L4 với n-gram có
backoff mới là lời giải đúng.

### L4 — mô hình ngôn ngữ và giải mã theo ngữ cảnh

| Việc | Trạng thái |
|---|---|
| `lm.rs` — Stupid Backoff, giải mã Viterbi bậc hai | ✅ |
| Trigram từ corpus | ✅ 249.972 (tiền tố bigram phổ biến, ≥6 lần) |
| Thay heuristic L3 bằng chấm điểm LM | ✅ |
| Eval harness: `make-eval` + `eval` | ✅ 35.034 lỗi đã tiêm |

**Chọn Stupid Backoff thay Kneser-Ney (PLAN.md ghi KN).** Ta chỉ cần *xếp hạng*, không
cần xác suất chuẩn hoá — và với corpus đủ lớn Stupid Backoff xếp hạng ngang ngửa các
phương pháp làm mượt phức tạp, đúng kết luận của bài báo gốc. Nó cũng không cần bảng
chiết khấu và trọng số backoff, tức không cần thêm dữ liệu nào ngoài số đếm đã có; ít
tham số hơn nghĩa là ít chỗ sai âm thầm hơn.

**Ngưỡng chọn bằng đường cong đánh đổi, không bằng cảm giác:**

| margin | Recall | Precision | F0.5 | FP/1000 |
|---|---|---|---|---|
| 3 | 96,6% | 95,1% | 0,954 | 2,52 |
| 4,5 | 94,1% | 98,2% | 0,974 | 1,20 |
| **6** *(mặc định)* | **90,7%** | **99,9%** | **0,979** | **0,53** |
| 9 | 78,1% | 100% | 0,947 | 0,13 |
| 12 | 60,0% | 100% | 0,882 | 0,05 |

`6` tối đa hoá F0.5 — độ đo ưu tiên precision gấp đôi recall, đúng hướng lệch dự án.

**Hai lỗi của tôi khi làm L4:**

1. **Lối tắt tốc độ dùng "hoặc" thay vì "và".** Bỏ qua vị trí khi từ tạo từ ghép quen
   với *một trong hai* bên — nhưng `sẽ điều` là tổ hợp có thật (`sẽ điều hành`), nên
   `chia sẽ điều này` bị bỏ qua dù bên trái đã rõ là sai. Phải là **cả hai** bên.
2. **Không loại từ ngoại khỏi phán quyết real-word.** Engine đi "sửa" từ tiếng Anh
   thành âm tiết Việt: `bit → bít` (292 lần trên 50 nghìn câu), `net → nét`,
   `hit → hít`, `bus → bú`. Chúng lọt vào vì L2 chấp nhận chúng như từ vay mượn, nên
   `classify` đúng khi im lặng — nhưng im lặng ở đó không có nghĩa là mời lớp real-word
   vào sửa. Đã thêm điều kiện: chỉ xét âm tiết tiếng Việt hợp lệ (1,11 → 0,53).

### L5 — dấu câu, khoảng trắng, viết hoa

| Việc | Trạng thái |
|---|---|
| `rules.rs` — khoảng trắng đôi, khoảng trắng trước dấu câu, thiếu space sau dấu | ✅ |
| Ngoặc / nháy kép không cân | ✅ |
| Viết hoa đầu câu | ✅ mặc định **tắt** |
| Kiểu chữ `...` → `…` | ✅ mặc định **tắt** |

Hai luật cuối tắt mặc định có lý do: trong chat và tin nhắn, **không viết hoa là chuẩn
mực chứ không phải lỗi** — bật mặc định thì Writa thành phiền toái ở đúng nơi người ta
gõ nhiều nhất. `...` vs `…` là sở thích văn phong, không phải lỗi.

Mọi luật đều tôn trọng vùng bảo vệ của L0: `1.000`, `3,14`, `10:30`, URL, đường dẫn,
code trong backtick đều không bị đụng tới.

### L6 — lớp AI

| Việc | Trạng thái |
|---|---|
| `writa-core/src/ai.rs` — hợp đồng thuần, **không có mạng** | ✅ |
| `crates/writa-ai` — gọi Claude API qua HTTP thô | ✅ |
| Test canh "core không có dependency mạng" | ✅ |

**Ranh giới cắt ở mức crate, không phải mức hàm.** Writa quảng cáo "100% offline mặc
định"; lời hứa đó chỉ đáng tin nếu *kiểm chứng được*. Nên `writa-core` không có
dependency mạng nào, và có test đọc chính `Cargo.toml` của crate để canh — thêm nhầm
một HTTP client là đỏ ngay. Cách này còn giữ được khả năng build sang WASM cho VSCode
extension và web demo.

Prompt nói rõ **engine offline đã lo những gì** (chính tả âm tiết, hỏi/ngã, dấu câu).
Thiếu phần đó thì AI báo lại đúng những lỗi L1–L5 vừa bắt, và user thấy cùng một lỗi
hai lần với hai cách diễn đạt khác nhau — trông như engine tự mâu thuẫn.

Dùng **structured output** nên không phải bóc tách văn xuôi và không có nhánh "parse
thất bại thì đoán". Không gửi `temperature`/`top_p` (các model hiện tại từ chối với
lỗi 400). Xét `stop_reason` **trước** khi đọc `content` — khi model từ chối thì
`content` rỗng, code đọc thẳng `content[0]` sẽ vỡ.

Rust chưa có SDK Anthropic chính thức nên gọi HTTP thô theo đúng tài liệu.

### P3 — thêm dấu tự động

| Việc | Trạng thái |
|---|---|
| `diacritic.rs` — chỉ mục bỏ dấu + Viterbi dùng lại của L4 | ✅ 12 test |
| `writa-cli restore` / `eval-diacritic` | ✅ |

**Chi phí biên gần bằng không, đúng như PLAN.md dự đoán.** Sửa lỗi real-word và thêm
dấu là *cùng một bài toán*: sinh lựa chọn cho từng vị trí, giải mã bằng LM. Toàn bộ
module này là bước sinh lựa chọn cộng việc ghép lại text — không có mô hình mới nào.

**Số đo trên 19.782 câu held-out (613.696 từ):**

| | Độ chính xác |
|---|---|
| Gộp chung | 93,97% |
| **Chữ thường (văn xuôi)** | **94,47%** |
| Viết hoa (tên riêng) | 91,30% |
| Đúng trọn câu | 34,11% |

**Chưa đạt mục tiêu >95% của PLAN.md.** Nói thẳng vậy. Tách theo kiểu viết cho thấy
tên riêng kéo con số xuống — mô hình n-gram cấp âm tiết về nguyên tắc không biết
`Lão Hạc` là tên tác phẩm.

**Một giả thuyết của tôi bị dữ liệu bác bỏ.** Mẫu sai chữ thường bị `tải trọng → tại
trong` lặp đi lặp lại chi phối, nên tôi đoán từ ghép đó bị tỉa mất khỏi `compounds.tsv`.
Tra ra thì cả hai đều có, và `tại trong` (445 lần) còn **phổ biến hơn** `tải trọng`
(92 lần) — vì nó nằm trong "tồn tại trong", "hiện tại trong". Đây không phải lỗi tỉa
dữ liệu mà là giới hạn thật của n-gram cấp âm tiết: một từ ghép kỹ thuật hiếm không
thắng nổi một chuỗi từ chức năng tình cờ.

Muốn đóng khoảng cách còn lại thì cần 4-gram, hoặc LM cấp **từ** thay vì cấp âm tiết,
hoặc một danh sách tên riêng. Cả ba đều là việc lớn hơn hẳn, không phải tinh chỉnh.

Một phần "sai" cũng là **gold sai**: `Điẹn → Điện`, `tiêng → tiếng` — engine đúng,
Wikipedia sai. Nên 94,47% là ước lượng bi quan.

### L3 — ghép hai luật (2026-08-12, do dogfooding)

Gõ thử `sẳng sàng gòi đó` trong Chrome: engine im lặng. Nguyên nhân không phải ngưỡng
mà là **sinh candidate chỉ đổi một thành phần âm tiết**. Cả hai lỗi đều cần hai:

| Sai | Đúng | Cần đổi |
|---|---|---|
| `sẳng` | `sẵn` | thanh (hỏi→ngã) **+** âm cuối (ng→n) |
| `gòi` | `rồi` | âm đầu (g→r) **+** nguyên âm (o→ô) |

Cả bốn luật đều đã nằm trong `rules.tsv` từ đầu — chỉ là chưa bao giờ được ghép lại.
(`onset g r` là luật mới, miền Nam phát âm /r/ thành [ɣ].)

`MAX_EDITS = 2`, kèm chi phí `extra_edit_margin = 5` cho candidate hai phép sửa. Giá
đo được của cả tính năng, trên 35 nghìn lỗi đã tiêm + 50 nghìn câu held-out:

| | Recall | Precision¹ | F0.5 | FP real-word |
|---|---|---|---|---|
| Chỉ một phép sửa | 90,7% | 98,4% | 0,967 | 0,53 / 1000 |
| **Thêm hai phép sửa** | **90,7%** | **98,4%** | **0,967** | **0,55 / 1000** |

¹ đã bỏ nhóm B (từ vay mượn ASCII) — xem ghi chú về chỉ số bên dưới.

**Hai điều phải nói rõ, vì cả hai đều là giới hạn của phép đo chứ không phải kết quả:**

1. **Recall đứng yên không chứng minh tính năng vô dụng.** Bộ test tiêm mỗi lần đúng
   một phép sửa, nên nó *không chứa* loại lỗi mà thay đổi này sinh ra để bắt. Cần bổ
   sung nhóm lỗi hai-phép-sửa vào `make-eval` mới đo được lợi ích.

2. **Ngưỡng 5 chọn theo precision, không theo ví dụ.** Ở ngưỡng 3, `gòi → rồi` được
   bắt trong mọi ngữ cảnh; ở ngưỡng 5 chỉ bắt khi ngữ cảnh mạnh (`xong gòi nha bạn`
   chênh 16,67 ✅, `sẳng sàng gòi đó` chênh 10,89 — thiếu 0,11 ❌). Hạ xuống 3 tốn 0,1
   điểm precision để đổi lấy một lợi ích **chưa đo được**. Chỉnh ngưỡng cho vừa một ví
   dụ là đúng cái bẫy mà kỷ luật đo đạc của dự án dựng lên để tránh.

### Chỉ số eval đang trộn hai vấn đề — đã tách (2026-08-12)

`writa-cli eval` tính precision bằng cách đếm **mọi** báo lỗi không nằm đúng vị trí đã
tiêm. Nhưng phần lớn số đó là từ vay mượn tần suất thấp (`subroutine`, `ketchup`,
`gerrard`) — nhóm B, thứ `scan` vốn theo dõi riêng vì lời giải của nó là *corpus lớn
hơn* chứ không phải chỉnh ngưỡng.

Hệ quả: một chỉ số nhảy khi đổi corpus và đứng yên khi đổi thuật toán — đúng ngược với
thứ cần biết. Nay in cả hai: `Precision (toàn bộ)` 66,5% và `Precision (bỏ nhóm B)`
98,4%. F0.5 tính trên cái thứ hai.

**Số cũ ĐÃ tái lập được — nguyên nhân là công cụ đo, không phải engine (giải 2026-08-12).**

Tôi từng ghi ở đây rằng FP theo token "không tái lập được" (0,88 / 24,03 so với 0,25 /
6,40 trong bảng cũ) và để nó thành một câu hỏi mở suốt bốn lượt. Nguyên nhân: `scan` và
`eval` đếm **dấu câu lẫn vào nhóm lỗi từ**, còn số cũ thì được đo **trước khi lớp L5 tồn
tại**.

Đối chiếu số học:

| Nhóm | Số cũ | Đo lại (gộp) | Chênh | Riêng phần dấu câu |
|---|---|---|---|---|
| A — token có dấu Việt | 0,25 | 0,88 | **0,63** | **0,62** |
| B — token ASCII thuần | 6,40 | 24,03 | **17,63** | **17,61** |

Khớp tới hai chữ số thập phân. Sau khi tách dấu câu thành nhóm D riêng, mọi số gốc tái
lập **chính xác**: nhóm A = **0,25**, precision = **99,9%**, F0.5 = **0,979**.

Cả hai con số đều luôn đúng với thứ chúng đo. Chỉ phép so sánh là vô nghĩa — và tôi đã
để một cảnh báo sai trong README bốn lượt vì không nghi ngờ công cụ đo trước khi nghi ngờ
dữ liệu.

### Bẫy build của Tauri — bản release trỏ vào dev server (2026-08-12)

Triệu chứng user báo: *"gõ gì sai cỡ nào cũng không có hiện tượng gì xảy ra hết"*.

Nguyên nhân: `tauri/build.rs` đặt `dev = !custom_protocol`. Feature `custom-protocol`
theo quy ước gốc **không bật mặc định** — Tauri CLI mới thêm nó vào lúc `tauri build`.
Nên `cargo build --release -p writa-app` cho ra một binary tối ưu, có icon, chạy được,
**nhưng trỏ vào `http://localhost:5183`**. Không có Vite dev server thì WebView hiện
"Hmmm… can't reach this page".

Vì popup mặc định ẩn và chỉ hiện *sau khi* JavaScript gọi ngược `fit_popup`, JS chết
đồng nghĩa popup không bao giờ hiện. Không thông báo, không lỗi, không log — đúng triệu
chứng "app không hoạt động".

**Vì sao mất một vòng mới tìm ra.** Ba dấu hiệu đầu đều nói app khoẻ: process sống,
tray icon hiện, cả hai phím tắt đã chiếm chỗ toàn máy (kiểm bằng `RegisterHotKey` từ
process khác). Tất cả đều đúng — chúng chỉ không chạm tới tầng WebView.

**Cách gỡ.** Ba bước, mỗi bước loại bỏ một nửa không gian nghi vấn:

| Bước | Công cụ | Kết luận |
|---|---|---|
| 1 | `--selftest` (bỏ qua phím tắt + đọc vùng chọn) | Popup vẫn không hiện → lỗi ở UI, không ở khâu capture |
| 2 | `WRITA_DEBUG=1` ghi log từng bước | `get_review`/`fit_popup` chưa bao giờ được gọi → JS không chạy |
| 3 | `PrintWindow(PW_RENDERFULLCONTENT)` chụp cửa sổ | Đọc được dòng "can't reach this page" → sai địa chỉ tải |

Bước 3 cần đúng API đó: `CopyFromScreen`/BitBlt **không** bắt được nội dung WebView2 vì
Chromium vẽ qua DirectComposition — ảnh chụp lần đầu chỉ ra hình nền desktop và suýt
dẫn tới kết luận sai là "cửa sổ không hiện".

**Đã sửa:** `custom-protocol` thành feature mặc định. Đánh đổi mất hot-reload, đổi lấy
việc mọi cách build đều cho ra app chạy được. Hai công cụ chẩn đoán (`--selftest`,
`WRITA_DEBUG`) giữ lại.

**Còn lại:** clipboard trong môi trường shell tự động không mở được (`OpenClipboard`
trả `E_ACCESSDENIED`, kể cả `Set-Clipboard` của PowerShell cũng hỏng, và không cửa sổ
nào đang giữ). Nên đường lùi đọc-vùng-chọn-bằng-clipboard và đường ghi-bằng-dán
**chưa được kiểm thử lần nào**. Đã thêm cơ chế thử lại cho `OpenClipboard` (8 lần ×
12 ms) vì tranh chấp chớp nhoáng là chuyện có thật, nhưng bản thân cơ chế đó cũng chưa
kiểm được.

### P2 — vỏ ứng dụng (Tier 1)

| Việc | Trạng thái |
|---|---|
| Khay hệ thống, icon đổi màu khi tạm dừng | ✅ |
| Phím tắt toàn cục, chuỗi lùi 3 bậc khi bị chiếm | ✅ `Ctrl+Alt+V` / `Ctrl+Alt+D` |
| Popup gợi ý, định vị theo caret, kẹp theo màn hình | ✅ |
| Cửa sổ cài đặt | ✅ Vite + TypeScript thuần, ~10 KB JS |
| Từ điển cá nhân | ✅ thêm từ popup hoặc từ cài đặt |
| Chặn app do user chỉ định (cộng vào danh sách cứng) | ✅ |
| Khởi động cùng Windows | ✅ đọc trạng thái **thật** từ hệ thống, không tin file cấu hình |
| Đường clipboard không phụ thuộc cửa sổ | ✅ menu khay |
| Ghi ngược vào app đích | 🟡 code xong, **chưa đo trên app thật** — xem mục rủi ro ở trên |

Ba quyết định đáng ghi lại:

1. **Không dùng React.** Hai cửa sổ nhỏ, không có state phức tạp, và popup thì mở
   ra đóng vào liên tục nên thời gian parse bundle là thứ user cảm nhận trực tiếp.
   TypeScript thuần cho ~10 KB JS thay vì ~150 KB.

2. **Popup tự đo chiều cao rồi mới hiện.** JS gọi ngược lệnh `fit_popup`; Rust đặt
   kích thước, định vị theo caret, **rồi mới** `show()`. Hiện trước khi đo thì mỗi
   lần bấm phím tắt user thấy một khung sai kích thước nhấp nháy.

3. **`save_settings` trả về thứ thật sự đang chạy, không phải thứ user gửi lên.**
   Phím tắt có thể bị app khác chiếm, autostart có thể bị chính sách hệ thống chặn.
   UI vẽ lại theo giá trị trả về nên nó không bao giờ hiển thị một thiết lập không
   tồn tại.

### P4 — Tier 2 real-time

| Việc | Trạng thái |
|---|---|
| `buffer.rs` — bộ đệm từ đang gõ | ✅ 11 test, không cần bàn phím thật |
| `hook.rs` — bàn phím + chuột + đổi cửa sổ | ✅ hạ tầng đã đo, xem bên dưới |
| Câu hỏi bộ gõ (spike 5) | ✅ **GO** — mô hình cũ sai, đã sửa |
| Overlay inline `WS_EX_NOACTIVATE` | ✅ đo end-to-end |
| `realtime.rs` — nối hook → bộ đệm → engine → overlay | ✅ đo end-to-end |
| Tự sửa lỗi chắc chắn (opt-in) | ✅ code xong, **chưa đo** |
| Áp dụng gợi ý bằng `Ctrl+Alt+Space` | ✅ code xong, **chưa đo** |

#### Luồng Tier 2 đã đo (2026-08-12)

Bật realtime, bơm `Toi lam trong nghanh ` vào app đang focus:

```
realtime: BAT, app_allowed=true
realtime:   exe=chrome.exe password=false blocklisted=false uia_password=false
realtime: het tu, dem = "Toi"      → xet ["Toi"] -> None
realtime: het tu, dem = "lam"      → xet ["Toi","lam"] -> None
realtime: het tu, dem = "trong"    → None
realtime: het tu, dem = "nghanh"   → xet [...] -> Some(("nganh", true))
realtime: goi y "nghanh" -> "nganh", emit Ok(())
fit_inline: 224x31                 → overlay hiện sau 0 ms
```

Ba chi tiết thiết kế đáng ghi lại:

1. **Kiểm cả cụm, báo một từ.** Xét một âm tiết đơn lẻ chỉ bắt được `nghành`; lỗi
   *real-word* (`chia sẽ`) thì vô hình vì `sẽ` là từ đúng. Nên engine nhận 5 từ gần
   nhất làm ngữ cảnh, và ta chỉ báo lỗi rơi đúng vào từ vừa gõ xong.

2. **Hỏi lại `IsPassword` ở mỗi từ, không chỉ khi đổi cửa sổ.** Bấm vào ô mật khẩu
   *trong cùng một app* (Chrome chẳng hạn) không bắn `EVENT_SYSTEM_FOREGROUND`. Vì
   thế click chuột được báo lên là `FocusChanged` chứ không phải `CaretMoved` — với bộ
   đệm hai cái giống nhau, nhưng cái đầu buộc tính lại quyền.

3. **Bịt hook khi tự bơm phím.** Writa sửa lỗi bằng `SendInput`, và phím đó mang đúng
   cờ `INJECTED` như phím bộ gõ. Không bịt thì bộ đệm nghe lại chính mình, *và* cơ chế
   bù-phím-bị-nuốt xoá oan một ký tự.

#### Xung đột giả tạo giữa hai chỉ tiêu MVP, và cách gỡ (2026-08-12)

Nới ngưỡng cắt tỉa lexicon (từ ghép ≥ 8 → ≥ 3, trigram ≥ 6 → ≥ 2) cải thiện mọi chỉ
tiêu chất lượng — nhưng làm RAM tiến trình chính nhảy từ **80 MB lên 194 MB**, trong khi
dữ liệu thô chỉ tăng 16 MB. Hai mục tiêu MVP bỗng chống nhau.

Chỗ thừa nằm ở `HashMap`: mỗi ô phải giữ khoá, giá trị, mã băm và chỗ trống của bảng —
tốn gấp nhiều lần bản thân dữ liệu, mà dữ liệu thì **đã nằm sẵn trong file thực thi**
dưới dạng văn bản.

Thay bằng **tìm nhị phân trên chính chuỗi đã nhúng**: không dựng bảng nào, chỉ cấp phát
một `u32` mỗi dòng để biết dòng bắt đầu ở đâu.

| | Trước | Sau |
|---|---|---|
| Lexicon | 250k từ ghép · 250k trigram | **632k · 1,5 triệu** |
| RAM tiến trình chính | 194 MB | **67 MB** |
| Recall (Tier 1) | 90,7% | **92,5%** |
| F0.5 | 0,979 | **0,982** |
| Thêm dấu, văn xuôi | 94,47% | **95,19%** |
| `check_with` p99 mỗi từ | 0,20 ms | 0,54 ms |

Latency tăng 2,7 lần nhưng vẫn dưới mốc 5 ms **mười lần**, nên nó không phải chi phí user
cảm nhận được. Công cụ đo hàng loạt thì chậm hơn hẳn (eval thêm dấu 23s → 126s) — đó là
chi phí của người phát triển, không phải của người dùng, và là đánh đổi đúng chiều.

Cộng thêm: cửa sổ cài đặt chuyển sang **tạo theo nhu cầu** thay vì giữ sẵn. Nó được mở
vài lần mỗi tháng, còn Writa thì chạy cả ngày.

**Đính chính về con số RAM.** Bốn lượt trước tôi báo "RAM 79,6 MB" mà chỉ đo tiến trình
chính, bỏ sót tiến trình WebView2 (~131 MB). Tổng thật luôn là ~200 MB. Mốc "< 80 MB"
trong PLAN.md viết khi chưa tính tới runtime WebView2 — thứ mà mọi app Tauri/Electron
đều phải trả.

#### Độ trễ mỗi từ — và một lớp chắn tốn 200 ms (2026-08-12)

Đo bằng `hook-probe`, 30 vòng, cụm 5 từ có một lỗi real-word:

| Việc | p50 | p99 |
|---|---|---|
| `context::current` (Win32) | 0,10 ms | 0,25 ms |
| **`is_password_element` (UIA)** | **3,16 ms** | **200,52 ms** |
| `caret::locate` (chỉ khi có gợi ý) | 3,56 ms | 5,79 ms |
| `check_with` (engine, 5 từ) | 0,13 ms | **0,20 ms** |

**Engine nhanh gấp 25 lần mốc PLAN.md** (0,20 ms so với 5 ms). Nút cổ chai không nằm ở
tiếng Việt mà ở UIA.

Bản trước gọi `is_password_element()` cho **mỗi từ** làm lớp chắn cuối. Nó tốn gấp một
nghìn lần chính việc kiểm tra chính tả, và vì thread tiêu thụ chạy tuần tự, một lần
200 ms làm mọi phím sau đó dồn lại — gợi ý đến sau khi user đã gõ xong câu.

**Đã bỏ.** Cái nó bảo vệ thì ba lối khác đã bịt, và mỗi lối đều gọi `app_allowed` (vốn
có hỏi UIA) — chỉ là hỏi khi focus **thật sự có thể đã đổi**:

| Lối đổi phần tử focus | Đã bịt bằng |
|---|---|
| Đổi cửa sổ | `EVENT_SYSTEM_FOREGROUND` → `FocusChanged` |
| Click chuột | `WH_MOUSE_LL` → báo lên là `FocusChanged` |
| Tab / Enter trong form | xử lý riêng đầu `on_event` |

Sau khi bỏ, mỗi từ hoàn thành chỉ còn engine — **p99 ≈ 0,2 ms**.

#### Ngưỡng của Tier 1 quá chặt với Tier 2 (2026-08-12)

`chia sẽ` — ví dụ chính tả tiêu biểu nhất của tiếng Việt, và là ví dụ mở đầu README —
**không được Tier 2 báo**. Đo ra thì nó thiếu 0,44:

```
chia sẽ         →  sẻ  chênh 5.56   (ngưỡng 6 → im lặng)
muốn chia sẽ    →  sẻ  ✅            (thêm một từ ngữ cảnh là vượt)
```

Đây là một lỗ hổng **có hệ thống**, không phải một ca lẻ. `DEFAULT_REAL_WORD_MARGIN = 6`
được chọn bằng cách đo trên **câu đầy đủ** — đúng tình huống của Tier 1. Tier 2 thì
không bao giờ có cả câu: tối đa 5 từ đã gõ xong, và ngữ cảnh đó bị vứt sạch mỗi lần user
di con trỏ. Ít ngữ cảnh hơn nghĩa là ít bằng chứng hơn cho cùng một lỗi, nên cùng một
ngưỡng lại chặt hơn hẳn.

**Không đoán ngưỡng mới.** Thêm `writa-cli eval-realtime` — phép đo mô phỏng *đúng hình
dạng* Tier 2 (cửa sổ 5 từ, xét 2 vị trí cuối, một gợi ý mỗi bước) rồi quét lại:

| margin | Báo oan / 1000 từ | Recall |
|---|---|---|
| 6 *(của Tier 1)* | 0,48 | 88,7% |
| **5** *(chốt)* | **0,91** | **91,8%** |
| 4,5 | 1,21 | 93,0% |
| 4 | 1,71 | 94,0% |
| 3,5 | 2,25 | 95,1% ← vượt ngân sách MVP 2,0 |

Chốt **nới 1,0** (6 → 5): đổi gấp đôi báo oan lấy 3,1 điểm recall, vẫn dưới **một nửa**
ngân sách, và bắt được `chia sẽ`. Không đi xa hơn vì hướng lệch của dự án là precision
trước — 4,5 mua thêm 1,2 điểm recall bằng 33% báo oan nữa.

Cài dưới dạng **độ nới** chứ không phải hằng số tuyệt đối, nên lựa chọn "Độ nhạy" của
user vẫn có tác dụng ở cả hai tier, và có sàn 3,0 để mức nhạy nhất cũng không thành bừa.

#### Lỗi: một núm đo lường lọt vào cài đặt người dùng (2026-08-12)

User báo Tier 2 "vẫn không được" sau hai vòng sửa. Log chẩn đoán cho thấy engine chạy
đúng, bắt đúng từ, và trả `Clear` — trong khi cùng chuỗi đó chạy qua CLI thì bắt được.

Nguyên nhân nằm trong `settings.json`:

```json
"detectRealWord": false
```

`CheckOptions::detect_real_word` tồn tại để `writa-cli scan --no-realword` **đo** phần
đóng góp riêng của lớp real-word vào false-positive. Nó bị đưa lên UI thành một ô tick,
và khi tắt thì `chia sẽ`, `sữa lỗi`, `xử dụng` — nhóm lỗi người Việt mắc nhiều nhất —
biến mất hoàn toàn. Chỉ còn lớp âm tiết, nên `nghành` vẫn chạy còn mọi thứ khác thì
không: app **trông như hỏng** chứ không như đã bị tắt bớt.

**Đã sửa:** bỏ hẳn trường đó khỏi `Settings`; `check_options()` luôn đặt `true`. File
cũ tự lành vì serde bỏ qua khoá lạ. Ai muốn Writa nói ít hơn thì dùng "Độ nhạy", vốn
giữ được precision thay vì bỏ cả lớp.

**Bài học rộng hơn:** một tham số sinh ra để *đo* không phải một tuỳ chọn *người dùng*.
Đưa nó lên UI là mời user vô hiệu hoá phần giá trị nhất của sản phẩm mà không hiểu cái
giá.

**Hai thứ thêm vào để lần sau chẩn đoán mất vài giây thay vì cả lượt:**

- `config::load` ghi lại **thiết lập hiệu lực** khi bật `WRITA_DEBUG`. Một lượt gỡ rối
  đã mất chỉ vì không ai thấy được app đang chạy với cấu hình nào.
- `sanitize()` chuẩn hoá chuỗi phím tắt. File của user có `"Ctrl + Alt + Space"` (có
  khoảng trắng) — cùng một phím tắt nhưng khác chuỗi, đủ để UI báo "phím tắt bị chiếm"
  ngay sau khi nó vừa đăng ký xong.

#### Chất lượng khi hai lỗi nằm cạnh nhau

Cùng lần đo trên, user gõ `nay tôi sữa lổi chính tẻ` — **hai từ sai liền nhau**. Mô
hình ngôn ngữ mất điểm tựa, vì cả `sữa lỗi` lẫn `sửa lổi` đều không phải tổ hợp có thật:

```
lổi   điểm gốc -28.07    nổi  +11.64   ← thắng
                         lỗi  +11.18   ← đúng, nhưng thua 0,46
```

Writa vẫn báo, nhưng **đề xuất sai**. Khi từ bên cạnh đúng thì nó đúng ngay:
`nay tôi sửa lổi` → `lỗi` ✅, `nay tôi sữa lỗi` → `sửa` ✅.

Đây là giới hạn thật của cách giải mã từng-vị-trí-một. Lời giải đúng là giải mã **liên
kết** cả cụm bằng Viterbi (đã có sẵn cho phần thêm dấu), nhưng nó nới rộng vùng tìm kiếm
rất nhiều nên phải đo lại false-positive trước khi đổi. Chưa làm.

#### Lỗi: gợi ý sống chưa tới một giây (2026-08-12, do dogfooding)

Gõ `nay sữa lỗi chính tã chia sẽ` → **không thấy gợi ý nào**, dù engine bắt được cả
`sữa`, `tã` và `sẽ` khi chạy qua CLI.

Nguyên nhân là chính bản "xét lại từ trước" ở trên. Cửa sổ xét lại nhìn **hai** từ
cuối, nên:

| Gõ xong | Cụm xét | Kết quả bản lỗi |
|---|---|---|
| `nay sữa` | `[nay, sữa]` | im lặng — `sữa` chưa có ngữ cảnh phải |
| `nay sữa lỗi` | xét `lỗi`, `sữa` | **hiện `sữa → sửa`** ✅ |
| `… chính` | xét `chính`, `lỗi` | `sữa` rơi ngoài tầm nhìn → **xoá gợi ý** ❌ |

Code hiểu "lần này không thấy lỗi" thành "xoá gợi ý". Nên gợi ý tồn tại đúng một nhịp
gõ — gõ liền tay thì không ai kịp thấy.

**Sửa:** gợi ý chỉ hết hạn bằng tín hiệu **thật** — gõ quá xa (`MAX_TRAIL`), di con
trỏ, đổi cửa sổ, xoá ngược qua nó, hoặc bấm nhận. Không thấy lỗi ở lượt xét mới thì
*giữ nguyên* thứ đang hiện.

Cùng lúc sửa một lỗi họ hàng: khi gợi ý hết hạn, code thoát sớm và **bỏ mất phím đó**
khỏi bộ đệm, làm từ đang gõ hụt một ký tự.

**Đã biến thành test.** Cả hai lỗi đều là lỗi *quyết định*, không phải lỗi Win32, và cả
hai chỉ lộ ra khi gõ một câu dài trên máy thật. Nên phần quyết định được tách thành hàm
thuần `decide(context, pending, settings) -> Outcome`, và có test gõ lại **đúng câu
user đã gõ**, từng từ một, khẳng định gợi ý sống qua các bước 4–5.

#### Lỗi: "sửa xong lại mọc thêm từ mới" (2026-08-12, do dogfooding)

Bấm `Ctrl+Alt+Space` thì bản sửa **thêm vào** thay vì thay từ cũ.

Nguyên nhân: phím tắt bắn ngay lúc user **nhấn**, nên `Ctrl` và `Alt` vẫn còn đang
giữ. `SendInput(VK_BACK)` gửi vào giữa trạng thái đó tới app đích dưới dạng
`Ctrl+Alt+Backspace` — tổ hợp mà hầu hết app bỏ qua. Phần xoá không xảy ra, phần gõ
thì có, nên bản sửa mọc thành từ mới.

Trường hợp tệ hơn cùng gốc: `paste_text` tự gửi `Ctrl+V`, mà `Alt` còn giữ thì thành
`Ctrl+Alt+V` — **đúng phím tắt Tier 1 của chính Writa**, tức là tự gọi lại mình.

Đã sửa: `writer::release_modifiers()` nhả những phím phụ *đang thật sự xuống*
(`GetAsyncKeyState`) trước khi bơm, gọi từ `type_text`, `backspace` và `paste_text`.
Không dựng lại sau — user sắp nhả tay, còn keydown dựng lại thì có thể kẹt phím nếu ta
chết giữa đường.

Đo bằng `hook-probe` (không gõ chữ vào đâu cả, chỉ giữ phím rồi kiểm):

```
đang giữ:    Ctrl=true   Alt=true
sau khi nhả: Ctrl=false  Alt=false
```

**Tab tránh được lỗi này từ gốc**, vì không có phím phụ nào bị giữ — thêm một lý do
để nó là đường chính.

#### Tab làm phím nhận gợi ý — ngoại lệ duy nhất được chặn phím

`hook.rs` không chặn phím: một hook nuốt phím mà lỗi thì làm hỏng việc gõ của cả máy.
Tab là ngoại lệ, thu hẹp ba lớp:

1. Chỉ chặn khi **đang có gợi ý hiện** (`hook::set_swallow_tab`, bật ở `show`, tắt ở
   `hide`). Không có gợi ý thì Tab đi qua y như trước.
2. Chỉ chặn phím **vật lý**; Tab do `SendInput` bơm vẫn đi qua.
3. Chặn cả keydown lẫn keyup của cùng lần bấm, để app đích không nhận một keyup lẻ.

`Ctrl+Alt+Space` giữ lại làm đường dự phòng cho app cần Tab cho việc khác.

#### Tier 2 bắt được gì — đo từng loại lỗi (2026-08-12)

Realtime chỉ có trong tay những từ **đã gõ xong**, nên phải đo đúng dạng đó: cụm kết
thúc ngay tại từ có lỗi.

| Gõ xong | Bắt được | Loại |
|---|---|---|
| `… trong nghành` | `ngành` ✅ | L1 âm tiết không tồn tại |
| `đang ngiên` | `nghiên` ✅ | L1 |
| `đã quyêt` | `quyết` ✅ | L1 |
| `… chia sẽ` | `sẻ` ✅ | L4 real-word |
| `Cách xử dụng` | `sử` ✅ | L4 |
| `Tôi đã sẳng` | `sẵn` ✅ | L4, **hai phép sửa** |
| `tôi bị đàu` | `đào` ✅ | L4, luật `ao↔au` mới |
| `toi di hoc` | `tôi`, `học` ✅ | thiếu dấu — bắt được **khi có ngữ cảnh** |
| `nay sữa` → `nay sữa lỗi` | `sửa` ✅ *(muộn một từ)* | cần ngữ cảnh bên phải |
| `kết quả suất` → `… suất sắc` | `xuất` ✅ *(muộn một từ)* | cần ngữ cảnh bên phải |
| `anh ấy giành` | im lặng ✅ | `giành`/`dành` đều đúng — im lặng là đúng |
| `quyển truyện kể chuyện` | im lặng ✅ | văn bản đúng |
| `toi` (một từ, không ngữ cảnh) | im lặng | không đủ căn cứ |
| `cần cũng cố` | im lặng ❌ | **bỏ sót** — xem dưới |
| `xin chào ,` | im lặng — *theo thiết kế* | dấu câu là việc của Tier 1 |

**Hai giới hạn có thật, nói cho rõ:**

1. **`cũng cố` → `củng cố` không bắt được.** `cũng` là phó từ cực phổ biến nên khi mô
   hình lùi về unigram nó vẫn đủ điểm cạnh tranh, dù bigram `củng cố` phổ biến hơn hẳn
   `cũng cố`. Đây là điểm yếu của backoff, không phải của ngưỡng — hạ ngưỡng sẽ kéo
   theo cả một loạt báo oan khác.

2. **Không thêm dấu cho từ đứng một mình.** `toi` không ngữ cảnh thì im lặng; `toi di
   hoc` thì bắt được cả `tôi` lẫn `học`. Realtime không chạy `diacritic::restore` —
   thêm dấu vẫn là việc của `Ctrl+Alt+D` trên vùng chọn.

**Dấu câu bị loại có chủ ý.** Cụm ngữ cảnh của Tier 2 ghép các từ bằng space, nên nó
không phản ánh dấu câu thật user gõ. Phán quyết dấu câu trên một dữ liệu đã bị bóp méo
thì tệ hơn là không phán quyết.

#### Lỗi phụ tìm được khi đo

BOM UTF-8 trong `settings.json` làm `serde_json` hỏng, và `config::load` **im lặng**
quay về mặc định — mọi thiết lập bốc hơi không lời giải thích. Notepad và PowerShell
5.1 đều ghi BOM, nên user sửa tay file cấu hình là gặp. Đã cắt BOM trước khi parse, và
ghi một dòng log khi file có mà parse không được.

#### Hạ tầng hook đã đo xong (2026-08-12)

`hook-probe` tự bơm phím bằng `SendInput` rồi so những gì hook thấy với những gì đã
gửi. Vì `SendInput` cũng đặt cờ `INJECTED`, phím của chính ta đi **đúng con đường mà
bộ gõ dùng** — nên phép đo này kiểm được toàn bộ cơ chế, chỉ trừ hành vi của UniKey.

```
Đã gửi: "tiếng Việt " rồi 1 backspace   →   Hook thấy 12 sự kiện
Char('t') Injected packet=true  …  Char('ế') Injected packet=true  …
Backspace Injected packet=false
Bộ đệm ghép được: ["tiếng", "Việt"]
```

| Hạng mục | Kết quả |
|---|---|
| Cài hook + message loop nhận sự kiện | ✅ |
| Đọc đúng cờ `LLKHF_INJECTED` | ✅ |
| Giải mã `VK_PACKET` (mã UTF-16 nằm ở `scanCode`) | ✅ |
| Ký tự tiếng Việt có dấu (`ế`, `ệ`) qua được nguyên vẹn | ✅ |
| Nhận diện Backspace | ✅ |
| `buffer.rs` ghép lại đúng chuỗi đã gửi | ✅ `["tiếng", "Việt"]` |
| **`SendInput` không bị chặn ở phiên làm việc này** | ✅ (đường ghi ngược của Tier 1 dùng chung API này) |

**Câu hỏi còn lại đã hẹp hơn nhiều.** Trước đây là "hook có dùng được với bộ gõ
không". Giờ đã biết: *khi text đến qua `SendInput` + `KEYEVENTF_UNICODE` thì mọi thứ
chạy đúng*. Chỉ còn đúng một điều chưa biết — **UniKey/EVKey có ghi text bằng đường
đó không**, hay chúng dùng TSF/`WM_CHAR`, đường mà low-level hook không nhìn thấy.

Đó là spike 5, và nó cần người ngồi gõ: `cargo run -p ime-probe --release -- 40`.

Phần **thuật toán** của Tier 2 tách hẳn khỏi Win32 nên test được mà không cần gõ thật.
Bốn thứ làm hỏng bộ đệm, và cách xử lý:

1. **Bộ gõ tiếng Việt** — chỉ tin phím `Injected` khi có bộ gõ chạy. Nghe nhầm nguồn
   thì buffer ra `tieengs` thay vì `tiếng`.
2. **Backspace** — lùi buffer; bộ gõ dùng backspace rất nhiều khi ghép.
3. **Con trỏ nhảy chỗ** — vứt buffer.
4. **Đổi cửa sổ** — vứt buffer.

Nguyên tắc: **nghi ngờ thì vứt**. Buffer sai vừa cho đề xuất sai vừa **thay nhầm chỗ**
— tệ hơn nhiều so với bỏ lỡ một từ. Có test riêng cho chuyện Writa không nghe lại phím
của chính nó khi tự sửa (nếu không thì thành vòng lặp phản hồi).

### Phát hiện đã thay đổi thiết kế

Năm lần đo đối chiếu corpus đã tìm ra năm lỗi thật, không lần nào là lỗi suy đoán:

1. **Bug nhập chữ trong `phonology.rs`.** Luật cũ *chặn* khi chữ cuối âm đầu trùng
   chữ đầu vần, nhưng hiện tượng thật là **nhập chữ**: `gi`+`ì` → `gì`,
   `qu`+`uynh` → `quynh`. Ba token tần suất cao bị loại oan: `gì` (5.462),
   `quỳnh` (1.242), `gìn` (615). Cùng luật sửa lại giải thích luôn `quyên`,
   `giêng`, `giếng`.

2. **Cleaner corpus clean theo từng dòng.** Template và thẻ `<ref>` của Wikipedia
   trải nhiều dòng nên không pattern nào khớp — `quot`, `lt`, `gt`, `ref`,
   `title`, `publisher` lọt vào và đẩy phép đo FP lên 59,64/1000 toàn rác. Sửa
   thành clean trọn trang.

3. **Dump XML escape wikitext.** `<ref>` nằm trong file dưới dạng `&lt;ref&gt;`.
   Thay entity bằng dấu cách phá luôn cấu trúc thẻ, khiến `ref` thành token bị
   báo nhiều nhất (48.215). Phải **decode** entity, và decode **hai lần** vì
   wikitext viết `&nbsp;` thì trong XML thành `&amp;nbsp;`.

4. **`build_lexicon.py` quét regex chữ-thường trên dòng còn chữ hoa.** Bỏ chữ cái
   đầu của mọi từ viết hoa, sinh ra lexicon đầy mảnh vụn: `Wikipedia`→`ikipedia`,
   `Paris`→`aris`, `Giáo`→`iáo`, `Ông`→`ng`. Phát hiện được vì soát mắt danh sách
   trước khi tin nó. Lỗi này **nguy hiểm hơn báo oan**: nó làm engine IM LẶNG
   trước lỗi thật, và im lặng thì không ai báo cáo.

5. **Dump `pages-articles` không chỉ có bài viết.** Nó chứa cả Bản mẫu:, Thể loại:,
   Thảo luận:, Thành viên: — 36% số trang. Lọc `<ns>0</ns>` để chỉ giữ bài viết.

Một giả thuyết của tôi thì **sai**, và ghi lại ở đây vì nó cũng là dữ kiện: tôi cho
rằng `ng`, `ko`, `dc`, `n` trong lexicon là teencode từ trang thảo luận, và viết
test chặn chúng. Sau khi lọc namespace chúng vẫn còn — vì đến từ bài viết thật:
`ng` nằm trong bài về **ngữ âm tiếng Việt** liệt kê "âm cuối được viết bằng p, t,
c, ch, m, n, ng"; `ko` từ "KO GmbH" và mã ngôn ngữ Hàn; `n` từ biến số toán học.
Test đó đã bỏ thay vì giữ một khẳng định không đúng.

### Cách L2 phân biệt từ vay mượn với lỗi gõ tay

Cả hai đều "không phải âm tiết tiếng Việt" dưới mắt L1. Thứ tách chúng ra là **độ
lan toả** — dùng *số câu chứa* thay vì tần suất thô, vì tần suất thô bị một bài dài
lặp lại một từ làm lệch.

Bằng chứng cách này chạy được: `vectơ` xuất hiện 929 lần, còn `thuớc` — một lỗi
chính tả **thật của Wikipedia** (đúng là `thước`) — chỉ 11 lần. Hai đầu phân bố.

Toàn bộ danh sách suy ra từ corpus nên không phái sinh từ `hunspell-vi` hay từ điển
GPL nào — license MIT giữ nguyên.

### Còn lại gì

Group ASCII còn 7,28/1000, toàn bộ là từ vay mượn tần suất thấp hơn ngưỡng lan toả
(`ester`, `codon`, `hemoglobin`, `taxon`, `platin`, `rugby`). Nhóm có dấu Việt còn
`véctơ` (25), `mácma` (17), `elíp` (16) — cũng chỉ sát dưới ngưỡng.

Cách xử lý đúng **không phải hạ ngưỡng** mà là chạy trên corpus lớn hơn: mẫu hiện
tại chỉ 120 nghìn câu, nên cùng một ngưỡng lan toả sẽ tự nhận thêm các từ này khi
có 500 nghìn câu. Hạ ngưỡng thì đồng thời nhận cả lỗi gõ tay.
