# Thành phần bên thứ ba

Writa phát hành theo giấy phép **MIT** (xem [LICENSE](LICENSE)). Tài liệu này liệt kê
mọi thứ đến từ nơi khác, và điều kiện đi kèm.

---

## Dữ liệu

### Wikipedia tiếng Việt — CC BY-SA 4.0

Nguồn: <https://dumps.wikimedia.org/viwiki/>
Bản dùng: `viwiki-latest-pages-articles`, tải 2026-08-11.

`data/lexicon/*.tsv` được **suy ra bằng thống kê** từ dump này: tần suất âm tiết, số câu
chứa mỗi âm tiết, tần suất cặp và bộ ba âm tiết.

**Chỉ số đếm được ship. Không câu gốc nào nằm trong repo hay trong file cài đặt.** Đó là
lý do `data/raw/` và `data/eval/` đều bị loại khỏi git — bộ test tiêm lỗi chứa nguyên văn
câu Wikipedia nên nó được tái tạo tại chỗ chứ không phân phối.

Ranh giới pháp lý ở đây: tần suất và n-gram counts là **dữ kiện thống kê** phái sinh, nói
chung không mang tính biểu đạt được bảo hộ. Ta vẫn ghi attribution đầy đủ vì đó là việc
đúng phải làm, không phải vì bắt buộc phải làm.

### Dữ liệu tự xây — MIT, cùng giấy phép dự án

| Tệp | Nội dung |
|---|---|
| `data/phonology/onsets.tsv` · `rimes.tsv` | 27 âm đầu × 160 vần, viết tay từ mô tả ngữ âm tiếng Việt |
| `data/confusion/rules.tsv` · `notes.md` | Luật nhầm lẫn chính tả, curate tay |

Đây là chỗ **cố ý** không dùng nguồn có sẵn. `hunspell-vi` và Free Vietnamese Dictionary
đều là **GPLv3**; phái sinh từ chúng sẽ kéo cả dự án sang GPL. Toàn bộ tập âm tiết vì thế
được *sinh ra* từ bảng ngữ âm tự viết, không copy từ từ điển nào.

---

## Thư viện Rust

Mọi dependency trực tiếp đều là **MIT hoặc Apache-2.0** (phần lớn là cả hai), tương thích
với MIT.

| Crate | Giấy phép | Dùng làm gì |
|---|---|---|
| [`tauri`](https://github.com/tauri-apps/tauri) + `tauri-plugin-{global-shortcut,autostart,updater,process}` | MIT / Apache-2.0 | Vỏ ứng dụng, khay hệ thống, phím tắt, tự cập nhật |
| [`windows`](https://github.com/microsoft/windows-rs) | MIT / Apache-2.0 | Toàn bộ Win32: hook bàn phím, UIA, SendInput, clipboard |
| [`serde`](https://serde.rs) · `serde_json` | MIT / Apache-2.0 | Cấu hình, IPC |
| [`unicode-normalization`](https://github.com/unicode-rs/unicode-normalization) | MIT / Apache-2.0 | NFC — nền của mọi thao tác chữ tiếng Việt |
| [`ureq`](https://github.com/algesten/ureq) | MIT / Apache-2.0 | HTTP, **chỉ trong `writa-ai`** |

Danh sách đầy đủ kể cả dependency gián tiếp nằm trong `Cargo.lock`. Sinh lại bảng kiểm
giấy phép bằng `cargo license` hoặc `cargo deny check licenses`.

### Ranh giới mạng

`writa-core` **không có dependency mạng nào**, và điều đó được canh bằng một test đọc
chính `Cargo.toml` của nó ([`ai.rs`](crates/writa-core/src/ai.rs),
`core_has_no_network_dependency`). CI chạy test đó như một cổng riêng. Mọi HTTP nằm trong
`writa-ai`, một crate tách biệt chỉ chạy khi user chủ động bấm.

---

## Thư viện JavaScript

| Gói | Giấy phép |
|---|---|
| [`@tauri-apps/api`](https://github.com/tauri-apps/tauri) · `@tauri-apps/cli` | MIT / Apache-2.0 |
| [`vite`](https://vitejs.dev) | MIT |
| [`typescript`](https://www.typescriptlang.org) | Apache-2.0 |
| `@types/node` | MIT |

Frontend không dùng framework UI nào — không React, không thư viện component. Xem
[SPIKE_RESULTS.md](SPIKE_RESULTS.md) về lý do.

---

## Runtime yêu cầu ở máy người dùng

**Microsoft Edge WebView2** — có sẵn trên Windows 11 và trên Windows 10 đã cập nhật.
Writa không đóng gói kèm; installer sẽ nhắc cài nếu thiếu. Đây là thành phần của
Microsoft, phân phối theo điều khoản riêng của họ.

---

## Công cụ chỉ dùng lúc phát triển

Không đi vào bản phát hành: `rustfmt`, `clippy`, `cargo`, `npm`, và các script Python
trong `scripts/` (chỉ dùng thư viện chuẩn).
