# Writa — Lộ trình

> Cập nhật 2026-08-13. Mọi hạng mục ở đây xuất phát từ một **khoảng trống đã đo được**
> hoặc một **rủi ro đã xác định**, không phải từ danh sách mong muốn. Mục nào chưa có số
> đo thì ghi rõ là chưa.
>
> [PLAN.md](PLAN.md) là kế hoạch gốc, giữ nguyên để đối chiếu.
> [SPIKE_RESULTS.md](SPIKE_RESULTS.md) chứa toàn bộ phép đo và các lần giả thuyết bị bác.

---

## v0.1 — bản phát hành đầu tiên

**Trạng thái: sẵn sàng, trừ ba việc cần tay người.**

Những gì đã có và đã đo:

| | Kết quả |
|---|---|
| Tier 1 — bôi đen + phím tắt | ✅ Zalo, VS Code, Google Chat đã xác nhận tay |
| Tier 2 — kiểm tra lúc gõ | ✅ mặc định tắt, bật trong Cài đặt |
| Thêm dấu | ✅ `Ctrl+Alt+D` |
| Chính tả: FP / recall / F0.5 | 0,25–0,55 / 1000 · 92,5% · **0,982** |
| Thêm dấu, văn xuôi | **95,19%** (mốc PLAN: > 95%) |
| Độ trễ mỗi từ, p99 | **0,54 ms** (mốc: < 5 ms) |
| RAM tiến trình engine | **67 MB** (mốc: < 80 MB) |
| Installer | **7,84 MB** (mốc: < 20 MB) |
| Antivirus | ESET 0 phát hiện, không chặn lúc chạy |
| Tự cập nhật | ✅ có chữ ký, không bao giờ tự cài |

### Còn chặn v0.1

- [x] **`git init` + đẩy lên GitHub** — `xShiroeNguyenx/writa-companion`, public.
- [x] **Đặt secret cho CI** — cả 4 secret đã có; `latest.json` xuất hiện trong bản phát
      hành, tức chữ ký gói cập nhật hoạt động.
- [ ] **Thử cài thật** installer một lần: cài, chạy, gỡ.
- [ ] **Phát hành lại v0.1.0 / v0.1.1** với endpoint đúng — xem mục dưới.

### Endpoint cập nhật từng trỏ sai repo

Hai bản phát hành đầu tiên ra đời với `endpoints` trỏ `ShiroeNguyen/writa`, trong khi repo
thật là `xShiroeNguyenx/writa-companion`. Triệu chứng duy nhất: một dòng "Could not fetch
a valid release JSON from the remote" trong Cài đặt. Không log, không mã lỗi, và
`latest.json` trên GitHub thì hoàn toàn hợp lệ — nên nhìn từ phía server mọi thứ trông
như đã đúng.

Điều làm nó đắt hơn một lỗi cấu hình thường: **endpoint là hằng số lúc dựng**, nó nằm
trong file thực thi ở máy user. Endpoint sai thì không bản cập nhật nào sửa được nó — cơ
chế dùng để vá lỗi lại chính là cơ chế bị lỗi. Ai đã cài phải tự đi tải bản mới, đúng cái
việc mà tính năng này tồn tại để họ không phải làm.

Nên nó thành một **cổng chặn trong CI**, không phải một dòng ghi chú: `release.yml` so
`endpoints` với `${{ github.repository }}` và so tag với `version`, thất bại thì không
dựng. Hai phép so đó rẻ đến mức không đáng để lệ thuộc vào việc con người nhớ.

### Thử tính năng tự cập nhật

Chỉ thử được **sau khi** đã có hai bản phát hành công khai, vì endpoint và khoá công khai
được nhúng vào file thực thi lúc dựng — không có cách nào chuyển hướng nó lúc chạy.

1. Phát hành `v0.1.0`, **bấm Publish** (nháp không tính là "latest" — xem ghi chú trong
   `release.yml`), tải installer về cài.
2. `.\scripts\bump-version.ps1 0.1.1` → commit → tag `v0.1.1` → push tag.
3. Publish bản `v0.1.1`.
4. Mở Cài đặt của bản đang cài → **Kiểm tra ngay**. Hoặc chỉ mở app rồi đợi 90 giây
   (`STARTUP_DELAY_SECS`) nếu muốn thử luôn cả đường kiểm tra tự động.

Bốn chỗ hay làm nó im lặng, xếp theo tần suất: bản phát hành còn ở dạng nháp; endpoint
trỏ sai repo; version trong `tauri.conf.json` không khớp tag; `latest.json` thiếu trong
bản phát hành (nghĩa là `TAURI_SIGNING_PRIVATE_KEY` chưa đặt, vì `tauri-action` chỉ sinh
nó khi có chữ ký). Cổng CI ở trên bắt được chỗ thứ hai và thứ ba.

### Nên làm trước khi mời người ngoài dùng

- [ ] **Đo Windows Defender.** Máy phát triển dùng ESET nên Defender bị vô hiệu hoá và
      chưa đo được — mà Defender là cấu hình của đa số người dùng.
- [ ] **Đường clipboard chưa chạy lần nào.** Sandbox tự động chặn `OpenClipboard`. Đó là
      đường lùi khi UIA không đọc được vùng chọn, tức là ở khá nhiều app.
- [ ] **Điền nốt ma trận tương thích** — còn 9/13 app trống. Chạy
      `cargo run -p writa-win --bin win-probe --release` ở từng app.

---

## v0.2 — thu hẹp những chỗ đã biết là yếu

Xếp theo **giá trị đo được / công sức**, không theo độ thú vị.

### Tên riêng khi thêm dấu — 92,23%

Đây là điểm nghẽn duy nhất khiến số **gộp** 94,72% chưa đạt 95%, dù văn xuôi đã 95,19%.
Mô hình n-gram cấp âm tiết về bản chất không suy được `Nguyễn` hay `Nguyên`.

Hướng đi: `build_lexicon.py` xuất thêm **tần suất của dạng viết hoa**, rồi
`diacritic::options_for` xếp hạng theo bảng đó khi token viết hoa. Dữ liệu đã có sẵn
trong corpus, chỉ chưa ai đếm riêng.

### `cũng cố` → `củng cố` bị bỏ sót

`cũng` là phó từ cực phổ biến nên khi mô hình lùi về unigram nó vẫn đủ điểm cạnh tranh,
dù bigram `củng cố` phổ biến hơn hẳn `cũng cố`. Đây là điểm yếu của Stupid Backoff, không
phải của ngưỡng — hạ ngưỡng sẽ kéo theo một loạt báo oan khác.

Hướng đi: thử Kneser–Ney, thứ PLAN.md từng cân nhắc rồi gạt đi vì "chỉ cần xếp hạng".
Trường hợp này cho thấy nhận định đó chưa đủ.

### Hai lỗi nằm cạnh nhau cho đề xuất sai

`sữa lổi` → engine đề xuất `lổi → nổi` (11,64) thay vì `lỗi` (11,18), vì cả `sữa lỗi` lẫn
`sửa lổi` đều không phải tổ hợp có thật nên mô hình mất điểm tựa.

Hướng đi: **giải mã liên kết** cả cụm bằng Viterbi — bộ máy đã có sẵn cho phần thêm dấu.
Nó nới rộng vùng tìm kiếm rất nhiều nên phải đo lại false-positive trước; công cụ đo đã
có (`writa-cli eval-realtime`).

### Thứ tự chuỗi hook có thể đảo

Windows gọi hook cài **sau cùng trước tiên**. Writa chạy được vì nó cài hook sau UniKey.
Nếu user khởi động lại UniKey **sau** khi Writa đang chạy thì thứ tự đảo, Writa chỉ còn
thấy luồng bơm, và bộ đệm ra mảnh vụn.

Nhận ra được — thấy loạt `VK_PACKET` mà không có phím vật lý đi trước — nhưng chưa xử lý.
Cách sửa có thể là cài lại hook khi đổi cửa sổ để luôn đứng đầu chuỗi.

### Bộ eval chưa đo được nhóm lỗi hai-phép-sửa

`make-eval` tiêm mỗi lần đúng một phép sửa, nên recall của tính năng candidate hai phép
sửa (`sẳng sàng` → `sẵn sàng`) **không xuất hiện trong bất kỳ con số nào**. Cần bổ sung
nhóm đó vào bộ sinh test.

### Cổng chất lượng trong CI

Hiện các số chất lượng là **đo tay**, vì corpus 1 GB không commit. Lưu sẵn một tập
held-out nhỏ (vài nghìn câu, vài trăm KB) sẽ biến FP và recall thành cổng CI thật sự —
đúng điều PLAN.md yêu cầu ngay từ P1.

---

## v0.3 — mở rộng khi nền đã chắc

### Corpus không phải Wikipedia

Đây là giới hạn xuyên suốt, và nó bóp méo mọi con số theo cùng một hướng. Writa chạy
trong ô chat, còn mô hình học từ bách khoa toàn thư. Hệ quả đã thấy: `deadline`,
`meeting`, `push` từng bị báo là lỗi chính tả vì Wikipedia gần như không có chúng.

Cần một corpus chat/diễn đàn tiếng Việt có license rõ ràng. Đây cũng là điều kiện tiên
quyết để đo được rủi ro thật của tuỳ chọn "báo âm tiết chưa từng thấy", vốn hiện chỉ đo
được một cách vòng tròn.

### Từ điển cá nhân học dần

Hiện phải thêm tay. Học từ hành vi — user bỏ qua một từ ba lần thì thôi báo nó — vừa
đúng thói quen vừa gần như không tốn gì để làm.

### Per-app profile

Khung đã có (`DEFAULT_BLOCKLIST`, blocklist của user). Còn thiếu phần điều chỉnh theo
app: mạnh tay ở Zalo và Word, chỉ xét comment trong IDE, im lặng hẳn ở terminal.

### Biến thể vùng miền

`data/confusion/rules.tsv` đã đánh dấu luật nào là đặc trưng Bắc (`l/n`) hay Nam
(`v/d`, `g/r`, âm cuối `n/ng`). Cho user chọn vùng miền sẽ bớt nhiễu đáng kể cho người
mà những cặp đó là phát âm bình thường chứ không phải lỗi.

---

## Xa hơn — theo PLAN.md, chưa lên lịch

- **VSCode extension** qua WASM. `writa-core` không phụ thuộc OS chính là để dành cho
  việc này, và đây là kênh phân phối đáng tin nhất: VSCode cho API text đầy đủ nên không
  cần hook nào cả.
- **Web demo** bằng WASM — gần như miễn phí khi đã có bản WASM, và là cách trình bày tốt
  nhất cho người chưa muốn cài gì.
- **Lớp AI** (`writa-ai` đã gọi được Claude API): ngữ pháp phức tạp và viết lại. Mặc định
  tắt, chỉ chạy khi user chủ động bấm, và ranh giới offline/online phải hiện rõ trong UI.
- **Insights** — thống kê lỗi của chính user theo thời gian. Chỉ lưu số đếm, không lưu
  nội dung.
- **macOS** — `CGEventTap` + Accessibility API. Cần Apple Developer ID và người dùng cấp
  quyền thủ công.

---

## Việc bảo trì cần để mắt

- **`data/lexicon/trigrams.tsv` nặng 21,2 MB** và được commit. Mỗi lần dựng lại lexicon là
  thêm một blob 21 MB vĩnh viễn vào lịch sử git.

  Tôi từng khuyên cân nhắc Git LFS ở đây; **khuyên vậy là sai**. LFS làm mọi lượt checkout
  của CI phải tải lại 25 MB object, mà hạn mức miễn phí của GitHub là 1 GB băng thông mỗi
  tháng — tức khoảng 40 lượt CI là hết. Git thường xử lý một file 21 MB hoàn toàn ổn
  (GitHub chỉ cảnh báo từ 50 MB). Cách đúng đơn giản hơn: **dựng lại lexicon thưa thôi**,
  và khi dựng thì gộp vào một commit.
- **Khoá ký gói cập nhật** (`.keys/`) không có bản sao lưu ở đâu ngoài máy này. Mất nó là
  mất khả năng phát hành bản mới cho những ai đã cài.
- **Chứng chỉ tự ký hết hạn 2031-08-13.** Chữ ký cũ vẫn hợp lệ nhờ đóng dấu thời gian,
  nhưng bản dựng mới sau ngày đó cần chứng chỉ mới.
