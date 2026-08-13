// Bong bóng gợi ý của Tier 2 — hiện ngay dưới chỗ đang gõ.
//
// # Vì sao không có nút bấm nào
//
// Cửa sổ này mang `WS_EX_NOACTIVATE`: nó không bao giờ lấy focus, nên nó **không
// nhận được phím và không nhận được cả cú bấm chuột có ý nghĩa**. Đó là chủ ý —
// user đang gõ dở, cướp focus là làm mất caret ở app đích và giết luôn tính năng.
//
// Hệ quả: mọi thao tác đi qua phím tắt toàn cục và hook bàn phím ở phía Rust. Vai
// trò của file này chỉ còn là *hiển thị*, và báo lại chiều rộng nội dung để Rust đặt
// kích thước cho khít.

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";

interface Suggestion {
  from: string;
  to: string;
  /// Phím tắt để áp dụng, đã định dạng sẵn để hiển thị.
  hotkey: string;
  /// Lỗi chắc chắn sai (âm tiết không tồn tại) hay chỉ là nghi ngờ.
  certain: boolean;
}

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const bubble = el<HTMLDivElement>("bubble");
const fromBox = el<HTMLSpanElement>("from");
const toBox = el<HTMLSpanElement>("to");
const keyBox = el<HTMLElement>("key");

async function render(s: Suggestion) {
  fromBox.textContent = s.from;
  toBox.textContent = s.to;
  keyBox.textContent = s.hotkey;
  bubble.classList.toggle("likely", !s.certain);

  // Đợi trình duyệt bố cục xong rồi mới đo, nếu không `scrollWidth` là của lần vẽ
  // trước — bong bóng sẽ luôn khít với gợi ý *trước đó*, lệch một nhịp.
  await new Promise(requestAnimationFrame);
  await invoke("fit_inline", {
    width: Math.ceil(bubble.scrollWidth) + 2,
    height: Math.ceil(bubble.scrollHeight) + 2,
  });
}

void listen<Suggestion>("writa://inline", (e) => void render(e.payload));
