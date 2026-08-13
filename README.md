# Writa

Sửa lỗi chính tả tiếng Việt ở **mọi ô nhập text trên máy** — Zalo, Chrome, Word,
Teams, Messenger. Chạy 100% offline, không gửi gì lên mạng.

> **Trạng thái: sẵn sàng cho bản phát hành đầu tiên (v0.1).**
>
> | | |
> |---|---|
> | `Ctrl+Alt+V` | Kiểm tra chính tả đoạn đang bôi đen |
> | `Ctrl+Alt+D` | Thêm dấu cho đoạn đang bôi đen |
> | `Tab` | Áp dụng gợi ý inline khi bật kiểm tra lúc gõ |
>
> Việc còn lại trước khi phát hành, và những chỗ đã biết là yếu, nằm ở
> [ROADMAP.md](ROADMAP.md). Toàn bộ phép đo — kể cả những lần giả thuyết của tôi bị
> chính số liệu bác bỏ — nằm ở [SPIKE_RESULTS.md](SPIKE_RESULTS.md).
> [PLAN.md](PLAN.md) là kế hoạch gốc, giữ nguyên để đối chiếu.

## Vì sao làm cái này

Không có công cụ nào sửa chính tả tiếng Việt xuyên app: Word chỉ trong Word,
extension chỉ trong browser, LanguageTool không hỗ trợ tiếng Việt, Grammarly không
biết tiếng Việt. Và lỗi phổ biến nhất của người Việt — **hỏi/ngã** — thì gần như
không tool nào xử lý tốt.

## Cách tiếp cận

Tiếng Việt có một tính chất mà tiếng Anh không có: **tập âm tiết ĐÓNG và sinh được**.
Chỉ ~18 nghìn âm tiết hợp lệ, dựng từ `âm đầu × vần × thanh` với vài ràng buộc chính tả.

Hệ quả: âm tiết không nằm trong tập là **sai chắc chắn**, phát hiện bằng một lần tra
cứu, precision 100%, không cần model. Lỗi `nghành`, `ngiên`, `quyêt` bắt được tức thì.

Phần khó là lỗi *real-word* — `chia sẽ`, `sữa lỗi`, `xử dụng` gồm toàn âm tiết hợp lệ.
Loại này cần sinh candidate theo **luật cấu trúc** (hỏi/ngã, s/x, ch/tr, r/d/gi…)
rồi phán quyết theo ngữ cảnh.

Điểm mấu chốt để làm được mà không báo oan: chỉ tần suất từ ghép là **không đủ**.
Thứ phân biệt `chia sẻ` (một từ ghép cố định) với `cát trắng` (kết hợp tự do) là
**độ chặt hai chiều** `min(P(b|a), P(a|b))`. Đo một chiều thôi thì từ chức năng như
`của`, `có` lọt qua, vì chúng đi sau rất nhiều từ với xác suất cao.

Cùng bộ máy đó cũng làm được **thêm dấu tự động**: `toi yeu tieng viet` →
`tôi yêu tiếng Việt`. Chỉ khác bước sinh candidate.

Và cùng chỉ mục bỏ dấu đó lại giải một bài toán tưởng như không liên quan: **phân biệt
từ tiếng Anh với từ tiếng Việt thiếu dấu**. Cả `deadline` lẫn `nghiep` đều là ASCII và
đều không phải âm tiết hợp lệ, nhưng `nghiep` ánh xạ tới `nghiệp` còn `deadline` không
ánh xạ tới đâu cả. Nhờ vậy Writa im lặng trước `meeting`, `push`, `check` — thứ người
Việt viết lẫn vào câu suốt ngày.

## Đã có gì

| | |
|---|---|
| `crates/writa-core/src/normalize.rs` | L0 — NFC, ký tự vô hình, hệ chữ Latin |
| `crates/writa-core/src/token.rs` | L0 — tách token + 11 loại vùng bảo vệ |
| `crates/writa-core/src/phonology.rs` | L1 — sinh tập âm tiết, 18.261 âm tiết |
| `crates/writa-core/src/dict.rs` | L2 — từ vựng suy ra từ corpus |
| `crates/writa-core/src/candidate.rs` | L3 — sinh candidate theo luật cấu trúc |
| `crates/writa-core/src/lm.rs` | L4 — Stupid Backoff + giải mã Viterbi |
| `crates/writa-core/src/rules.rs` | L5 — dấu câu, khoảng trắng, viết hoa |
| `crates/writa-core/src/ai.rs` | L6 — hợp đồng AI, **không có mạng** |
| `crates/writa-ai` | L6 — gọi Claude API; crate DUY NHẤT có mạng |
| `crates/writa-core/src/lib.rs` | `check()` — nối L0 → L5 |
| `crates/writa-core/src/diacritic.rs` | P3 — thêm dấu, dùng lại `lm.rs` |
| `crates/writa-win` | P0/P4 — hook bàn phím, đọc selection, ghi text, caret, overlay |
| `src-tauri` | P2/P4 — khay hệ thống, phím tắt, popup, gợi ý inline, tự cập nhật |
| `ui/` | Ba cửa sổ, Vite + TypeScript thuần (~11 KB JS) |
| `crates/writa-cli` | `check` · `explain` · `scan` · `eval` · `eval-realtime` · `eval-diacritic` … |
| `data/phonology/` | 27 âm đầu × 160 vần, dạng TSV người đọc/sửa được |
| `data/lexicon/` | Âm tiết chứng thực · từ ngoại chấp nhận · từ ghép |
| `data/confusion/rules.tsv` | Luật nhầm lẫn dạng máy đọc |
| `data/confusion/notes.md` | Sổ tay cặp nhầm lẫn tiếng Việt — đang curate |
| `scripts/extract_syllables.py` | Bóc tần suất âm tiết + câu văn xuôi từ dump Wikipedia |
| `scripts/build_lexicon.py` | Dựng từ vựng L2 từ corpus |
| `spikes/ime-probe` | Đo hành vi `WH_KEYBOARD_LL` khi UniKey compose tiếng Việt |

162 test pass, clippy sạch, `cargo fmt` sạch. Xem [.github/workflows/ci.yml](.github/workflows/ci.yml)
về những gì CI canh được — và những gì **không**, cùng lý do.

### Số đo hiện tại

Corpus: toàn bộ viwiki — 1,6 triệu bài viết, 231,7 triệu token. Từ vựng và mô hình
ngôn ngữ dựng từ 450 nghìn câu, **đo trên 50 nghìn câu held-out** (tách theo bài
viết, chưa từng thấy khi dựng).

| Chỉ tiêu | Tier 1 (vùng chọn) | Tier 2 (lúc gõ) | Ngưỡng MVP |
|---|---|---|---|
| **FP, token có dấu Việt** | **0,25 / 1000** | — | < 2,00 ✅ |
| **FP, token ASCII thuần** | **0,24 / 1000** | 0,23 / 1000 | < 2,00 ✅ |
| **FP, lỗi real-word** | **0,55 / 1000** | 0,89 / 1000 | < 2,00 ✅ |
| **Recall trên lỗi đã tiêm** | **92,5%** | **92,6%** | — |
| **Precision** | **99,6%** | — | — |
| **F0.5** | **0,982** | — | — |
| **Thêm dấu, văn xuôi** | **95,19%** | — | > 95% ✅ |
| Thêm dấu, tên riêng | 92,23% | — | — |
| Thêm dấu, gộp | 94,72% | — | > 95% ⚠️ |
| Latency p99 mỗi từ | — | **0,54 ms** | < 5 ms ✅ |
| **RAM, tiến trình engine** | **67 MB** | | < 80 MB ✅ |
| RAM, kể cả WebView2 | 198 MB | | ⚠️ xem dưới |
| Installer | **7,84 MB** | | < 20 MB ✅ |
| Dấu câu / viết hoa (lớp L5) | 18,25 / 1000 | *(không xét)* | ngân sách riêng |
| Độ phủ tập âm tiết trên 231 triệu token | 87,01% | — | — |

> **Về con số RAM.** Mốc "< 80 MB" trong PLAN.md được viết mà chưa tính tới runtime
> WebView2, vốn là một tiến trình Chromium dùng chung mà **mọi** app Tauri hay Electron
> đều phải trả. Tiến trình engine của Writa là 67 MB; 131 MB còn lại là WebView2. Bốn
> lượt trước tôi báo "RAM 79,6 MB" mà bỏ sót phần đó — con số ấy chưa bao giờ là toàn
> bộ.

Các con số được **tách riêng có chủ ý** — gộp lại là cách tự lừa mình, vì chúng có
nguyên nhân và lời giải khác nhau. Ba lớp có ba hướng sửa khác hẳn: nhóm ASCII cần
corpus lớn hơn, nhóm real-word cần ngưỡng, nhóm dấu câu là lớp tất định không liên
quan chính tả.

> **Chuyện đã xảy ra vì gộp chúng lại.** Bốn lượt đo trước, `scan` và `eval` đếm dấu câu
> **lẫn vào** nhóm lỗi từ. Nhóm ASCII vì thế đọc ra 24,03/1000 trong khi phần từ ngoại
> thật chỉ 0,24 — 18 điểm còn lại là dấu phẩy với dấu chấm. Nó cũng làm tôi tưởng số cũ
> trong tài liệu "không tái lập được" và ghi một cảnh báo sai vào đây: số cũ **luôn
> đúng**, chỉ phép so sánh là vô nghĩa vì lớp L5 chưa tồn tại khi chúng được đo. Sau khi
> tách, mọi số gốc tái lập chính xác — 0,25 · 99,9% · F0.5 0,979.
>
> Bài học: một chỉ số gộp nhiều nguyên nhân sẽ **nhảy khi đổi corpus và đứng yên khi đổi
> thuật toán** — đúng ngược với thứ cần biết.

Ngưỡng phán quyết real-word chọn bằng quét đường cong đánh đổi trên 35 nghìn lỗi
đã tiêm, không phải bằng cảm giác:

| margin | Recall | Precision | F0.5 | FP/1000 |
|---|---|---|---|---|
| 3 | 96,6% | 95,1% | 0,954 | 2,52 |
| 4,5 | 94,1% | 98,2% | 0,974 | 1,20 |
| **6** *(mặc định)* | **90,7%** | **99,9%** | **0,979** | **0,53** |
| 9 | 78,1% | 100% | 0,947 | 0,13 |
| 12 | 60,0% | 100% | 0,882 | 0,05 |

*(Cột precision trong bảng này là số cũ, nằm trong diện cảnh báo ở trên. Hình dạng
đường cong — precision đổi lấy recall — thì vẫn đúng.)*

### Lỗi cộng dồn

L3 ghép được **hai** luật cùng lúc, vì lỗi tiếng Việt hay đi theo cụm: `sẳng sàng` →
`sẵn sàng` cần đổi *cả* thanh (hỏi→ngã) *lẫn* âm cuối (ng→n), `gòi` → `rồi` cần đổi
*cả* âm đầu (g→r, đặc trưng miền Nam) *lẫn* nguyên âm (o→ô). Cùng một giọng nói sinh
ra cả hai lỗi, nên chúng xuất hiện cùng nhau là chuyện thường.

Candidate hai phép sửa phải mang thêm bằng chứng (`extra_edit_margin = 5`) vì nó a
priori kém khả dĩ hơn. Cái giá đo được của cả tính năng này:

| | Recall | Precision | F0.5 | FP real-word |
|---|---|---|---|---|
| Chỉ một phép sửa | 90,7% | 98,4% | 0,967 | 0,53 / 1000 |
| Thêm hai phép sửa | 90,7% | 98,4% | 0,967 | 0,55 / 1000 |
| **+ luật `ao↔au`** | **90,7%** | **98,3%** | **0,967** | **0,56 / 1000** |

Recall đứng yên **không phải vì tính năng vô dụng**, mà vì bộ test hiện tại tiêm mỗi
lần đúng một phép sửa — nó không chứa loại lỗi mà thay đổi này sinh ra để bắt. Muốn
biết lợi ích thật thì phải bổ sung nhóm lỗi hai-phép-sửa vào `make-eval`.

Phần đuôi các nhóm không hẳn là báo oan: lẫn trong đó có `vơí` (đúng là `với` —
đặt dấu sai vị trí), `chiéc` (đúng là `chiếc`), `chận` (đúng là `chặn`), `dà`
(đúng là `và`) — **lỗi chính tả thật của Wikipedia**, đúng loại lỗi Writa sinh ra
để bắt. Nên các con số trên là ước lượng **bi quan**.

## Cài đặt

Tải `Writa_*_x64-setup.exe` từ trang Releases rồi chạy. Writa nằm ở **khay hệ thống**,
không mở cửa sổ nào.

**Windows sẽ hiện "Windows protected your PC".** Bấm **More info → Run anyway**. Lý do
đầy đủ ở mục [Chữ ký](#chữ-ký-và-vì-sao-windows-vẫn-cảnh-báo) bên dưới — tóm tắt: đó là
vấn đề uy tín chữ ký, không phải vấn đề mã, và nó không mua được bằng tiền.

Gỡ như mọi app khác: Settings → Apps → Writa.

## Dựng từ mã nguồn

Cần: Rust ≥ 1.82 (MSVC toolchain), Node ≥ 18, Windows 10/11 có WebView2
(Windows 11 có sẵn).

```bash
npm install
npm run build                        # dựng frontend vào ui/dist
cargo build --release -p writa-app   # nhúng frontend vào exe
./target/release/writa-app.exe
```

Hoặc qua Tauri CLI:

| Lệnh | Việc |
|---|---|
| `npm run app` | Chạy thẳng |
| `npm run app:build` | Dựng installer NSIS, **không ký** — đây là lệnh CI dùng |
| `npm run release` | Dựng **và ký** bằng chứng chỉ ở `scripts/sign.ps1` |

Cả ba lệnh tự dừng Writa đang chạy trước (`scripts/prebuild.ps1`). Windows khoá file thực
thi đang chạy, và vì Writa được thiết kế để nằm ở khay hệ thống cả ngày, trạng thái bình
thường của máy phát triển chính là "app đang chạy" — nếu không dừng, `cargo` báo
`failed to remove file … Access is denied (os error 5)`, một thông báo không hề nhắc tới
nguyên nhân thật.

Installer ra ở `target/release/bundle/nsis/`, **7,84 MB**.

### Chữ ký, và vì sao Windows vẫn cảnh báo

Writa được ký bằng **chứng chỉ tự ký `CN=Shiroe Nguyễn`** (`scripts/sign.ps1`, tự tạo
chứng chỉ nếu chưa có; `npm run app:build` tự ký installer qua
`bundle.windows.certificateThumbprint`).

Chữ ký đó **được**:

- Chứng minh file do đúng máy dựng ra và chưa bị sửa sau đó — ai cũng kiểm được bằng
  `Get-AuthenticodeSignature`.
- Hết hỏi "Unknown publisher" trên máy đã cài chứng chỉ vào Trusted Root.
- Dựng sẵn đường ống ký, để đổi sang chứng chỉ thương mại chỉ là thay một dấu vân tay.

Và **không được**:

- **Không** làm SmartScreen im lặng. SmartScreen xét uy tín theo lượt tải, mà chứng chỉ
  không thuộc CA được tin cậy thì không có uy tín nào. Máy người lạ vẫn hiện *"Windows
  protected your PC"*, và đường chạy nằm sau **More info → Run anyway**.

Điều đó cũng **không mua được bằng tiền**: từ tháng 3/2024 Microsoft đã bỏ cơ chế cho
cert EV uy tín tức thì, nên app ký EV cũng phải tích luỹ uy tín y như ký OV. Xem
[SPIKE_RESULTS.md](SPIKE_RESULTS.md) mục Spike 6.

Về mã thì sạch: ESET quét installer + exe với đủ module `--unsafe --unwanted
--suspicious` cho **0 phát hiện**, và không chặn lúc chạy dù Writa cắm ba hook toàn máy.

> **Vì sao `custom-protocol` bật mặc định.** Tauri quyết định "dùng dev server hay
> asset nhúng" bằng feature `custom-protocol`, và theo quy ước gốc nó **không** bật
> mặc định — nên `cargo build --release` cho ra một binary trông như bản phát hành
> nhưng lại trỏ vào `http://localhost:5183`. Không có server đó thì cửa sổ chỉ hiện
> "can't reach this page", mà popup vốn ẩn nên triệu chứng duy nhất là *bấm phím tắt
> không có gì xảy ra*. Đúng lỗi này đã xảy ra một lần, nên nó được bật mặc định trong
> [src-tauri/Cargo.toml](src-tauri/Cargo.toml). Cái giá: sửa frontend phải chạy lại
> `npm run build` + `cargo build`, không có hot-reload.

### Khi thấy app "không làm gì"

```bash
./target/release/writa-app.exe --selftest
```

Bỏ qua phím tắt, ngữ cảnh app và việc đọc vùng chọn — nạp thẳng một câu có lỗi rồi
mở popup. Nếu popup hiện thì engine và giao diện đều sống, vấn đề nằm ở khâu đọc
vùng chọn. Nếu không hiện thì vấn đề ở giao diện.

Muốn biết chi tiết hơn, đặt `WRITA_DEBUG=1` — mỗi bước của luồng được ghi vào
`%TEMP%\writa-debug.log`. Mặc định tắt: file đó ghi tên app đang focus, và một công cụ
đọc được mọi ô nhập thì không được để lại dấu vết trên đĩa nếu user không chủ động bật.

App không mở cửa sổ nào lúc khởi động — nó nằm ở **khay hệ thống**. Bôi đen text ở
bất kỳ app nào rồi:

| Phím tắt | Việc |
|---|---|
| `Ctrl+Alt+V` | Kiểm tra chính tả đoạn đang bôi đen |
| `Ctrl+Alt+D` | Thêm dấu cho đoạn đang bôi đen |
| `Ctrl+Alt+Space` | Áp dụng gợi ý inline (khi bật realtime) |

### Kiểm tra ngay lúc gõ (Tier 2)

Mặc định **tắt**. Bật trong Cài đặt → *Kiểm tra ngay lúc gõ*. Khi bật, Writa gạch lỗi
ngay sau mỗi từ bạn gõ xong, ở mọi app. Bấm **`Tab`** để sửa, `Esc` hoặc bấm chuột để
bỏ qua.

`Tab` chỉ bị Writa chiếm **đúng trong khoảnh khắc gợi ý đang hiện** — hết gợi ý là nhả
ngay, `Tab` hoạt động bình thường. Đó là ngoại lệ duy nhất mà hook chặn phím, và nó
được thu hẹp bằng ba điều kiện; xem `hook::set_swallow_tab`. `Ctrl+Alt+Space` giữ lại
làm đường dự phòng cho app nào cần `Tab` cho việc khác.

Nó tắt sẵn không phải vì chưa chạy được, mà vì bật lên là **cắm một hook bàn phím toàn
máy**. Đó là thứ bạn phải chủ động đồng ý, không phải thứ bật sẵn rồi thông báo sau.
Tắt là *tháo hook thật sự*, không phải bỏ qua sự kiện — một công cụ có hình dạng
keylogger mà "tạm dừng" vẫn còn cắm hook thì lời hứa đó không kiểm chứng được.

Popup hiện ngay dưới con trỏ. `Enter` áp dụng, `Esc` đóng, bỏ tick từng chỗ không
muốn sửa, hoặc bấm **Chép** rồi tự dán nếu app đích không cho ghi trực tiếp.

Menu khay có **Kiểm tra nội dung trong clipboard** — đường không phụ thuộc cửa sổ
nào, tiện để thử nhanh.

## Chạy engine không cần app

```bash
cargo test -p writa-core
cargo run -p writa-cli --release -- check   "Tôi làm trong nghành này"
cargo run -p writa-cli --release -- restore "hom nay toi di hoc"
cargo run -p writa-cli --release -- explain "Tôi muốn chia sẽ điều này"
cargo run -p writa-cli --release -- count
cargo run -p writa-cli --release -- dict
```

### Dựng lại dữ liệu từ corpus

`data/lexicon/` đã được commit nên engine chạy được ngay. Chỉ cần các bước dưới khi
muốn dựng lại từ corpus mới hoặc sau khi sửa `data/phonology/`:

```bash
# 1. Tải dump (1,09 GB, không commit vào repo)
curl -L -C - --retry 5 \
  -o data/raw/viwiki-latest-pages-articles.xml.bz2 \
  https://dumps.wikimedia.org/viwiki/latest/viwiki-latest-pages-articles.xml.bz2

# 2. Bóc tần suất âm tiết + câu văn xuôi
py scripts/extract_syllables.py --limit-mb 300 \
    --sentences-out data/raw/sentences.txt     # nhanh, để thử
py scripts/extract_syllables.py \
    --sentences-out data/raw/sentences.txt     # toàn bộ, ~30 phút

# 3. Tách held-out để phép đo không bị vòng tròn
py -c "from pathlib import Path; l=Path('data/raw/sentences.txt').read_text(encoding='utf-8').splitlines(); c=int(len(l)*0.9); Path('data/raw/sentences-train.txt').write_text('\n'.join(l[:c])+'\n',encoding='utf-8'); Path('data/raw/sentences-heldout.txt').write_text('\n'.join(l[c:])+'\n',encoding='utf-8')"

# 4. Xuất tập âm tiết rồi dựng từ vựng L2 — CHỈ từ tập train
cargo run -p writa-cli --release -- dump data/build/syllables.txt
py scripts/build_lexicon.py --sentences data/raw/sentences-train.txt

# 5. Hai vòng kiểm chứng
cargo run -p writa-cli --release -- verify data/raw/syllable-freq.tsv
cargo run -p writa-cli --release -- scan   data/raw/sentences-heldout.txt
```

Bước 3 không phải hình thức: nếu dựng từ vựng và đo trên cùng tập câu thì mọi từ
trong text đo đã góp phần tạo ra từ điển, và con số false-positive sẽ tốt hơn thực
tế. Tách theo bài viết chứ không tách ngẫu nhiên theo câu, vì câu trong cùng bài
dùng lại cùng vốn từ.

Chạy spike IME (xem [SPIKE_RESULTS.md](SPIKE_RESULTS.md) để biết cách đọc kết quả):

```bash
cargo run -p ime-probe --release -- 40
```

## Quyền riêng tư

App này về mặt kỹ thuật là một keylogger, nên đây không phải mục phụ:

- **Zero network — kiểm chứng được bằng trình biên dịch, không phải bằng lời hứa.**
  Ranh giới offline/online cắt ở mức **crate**: `writa-core` không có dependency mạng
  nào, và có test đọc chính `Cargo.toml` của nó để canh điều đó. Toàn bộ HTTP nằm ở
  `writa-ai`, một crate riêng, chỉ chạy khi user chủ động bấm.
- **Không lưu text.** Buffer chỉ giữ từ/câu hiện tại trong RAM. Không log, không file tạm, không telemetry.
- **Chặn ô mật khẩu** nhiều lớp: UIA `IsPassword`, Win32 `ES_PASSWORD`, blocklist app.
- **Mã nguồn mở** — bạn tự audit được. Đó là lý do chính chọn MIT.

## Phát hành

```bash
npm run release                       # dựng + ký tại chỗ
git tag v0.1.0 && git push --tags     # CI dựng, ký, tạo bản nháp trên GitHub
```

`.github/workflows/release.yml` cần bốn secret — hai bắt buộc (khoá ký gói cập nhật),
hai tuỳ chọn (chứng chỉ ký mã). Bảng đầy đủ nằm ngay đầu file đó.

### Bốn secret của CI

| Secret | Bắt buộc | Lấy giá trị ở đâu |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | ✅ | nội dung `.keys/writa-update.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | ✅ | mật khẩu bạn đặt khi sinh khoá |
| `WINDOWS_CERT_PFX_BASE64` | ⬜ | `scripts/sign.ps1 -ExportPfx` |
| `WINDOWS_CERT_PASSWORD` | ⬜ | mật khẩu bạn đặt cho `.pfx` |

Đặt tại **GitHub → repo → Settings → Secrets and variables → Actions → New repository
secret**.

```powershell
# Sinh khoá ký gói cập nhật, và cập nhật luôn pubkey trong tauri.conf.json.
# Hai việc đó PHẢI đi cùng nhau — lệch nhau thì app từ chối mọi bản cập nhật, im lặng.
.\scripts\new-update-key.ps1

# Chép giá trị vào clipboard mà không in ra màn hình
Get-Content .keys\writa-update.key -Raw | Set-Clipboard

# Chứng chỉ ký mã cho CI (tuỳ chọn) — nhớ XOÁ hai file sinh ra sau khi dán xong
.\scripts\sign.ps1 -ExportPfx
```

> **`.keys/` không có đường khôi phục.** Mất nó là mất khả năng phát hành bản mới cho
> **những người đã cài** — họ sẽ kẹt ở phiên bản cũ vĩnh viễn, vì app từ chối mọi gói
> không khớp khoá công khai đã nhúng. Sao lưu ra ngoài repo.

## Giấy phép

MIT. Toàn bộ dữ liệu ngữ âm và confusion-set **tự xây**, không phái sinh từ
`hunspell-vi` hay Free Vietnamese Dictionary (GPLv3), nên license giữ được ở mức
permissive. Xem `THIRDPARTY.md` khi có dữ liệu bên thứ ba.

Corpus tần suất suy ra từ Wikipedia tiếng Việt (CC BY-SA). Chỉ **số đếm** được ship,
không có câu gốc nào.
