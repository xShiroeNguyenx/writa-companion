// Popup gợi ý — cửa sổ user thật sự nhìn thấy khi bấm hotkey.
//
// # Vì sao popup này LẤY focus
//
// PLAN.md nói overlay phải có `WS_EX_NOACTIVATE` để không cướp caret. Điều đó đúng
// cho overlay inline của Tier 2, nơi user đang gõ dở. Nhưng Tier 1 khác hẳn: user
// đã bôi đen xong rồi mới bấm phím tắt, và việc tiếp theo họ cần làm là **đọc và
// chọn**. Một cửa sổ không nhận được phím hay chuột thì không làm được việc đó.
//
// Lấy focus cũng chính là thứ khiến bước ghi ngược hợp lệ: Windows chỉ cho
// `SetForegroundWindow` khi process gọi đang là foreground. Xem `writa_win::context::focus`.

import { listen } from "@tauri-apps/api/event";
import {
  applyReview,
  copyToClipboard,
  dismissReview,
  fitPopup,
  getReview,
  ignoreWord,
  previewSegments,
  previewText,
  type Change,
  type Review,
} from "./api";
import "./style.css";

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const listBox = el<HTMLDivElement>("list");
const previewBox = el<HTMLDivElement>("preview");
const errorBox = el<HTMLDivElement>("error");
const titleBox = el<HTMLSpanElement>("title");
const appBox = el<HTMLSpanElement>("app");
const applyBtn = el<HTMLButtonElement>("apply");
const copyBtn = el<HTMLButtonElement>("copy");
const closeBtn = el<HTMLButtonElement>("close");

let review: Review | null = null;
/** id → chuỗi thay thế. Vắng mặt nghĩa là user đã bỏ chọn chỗ đó. */
let picks = new Map<number, string>();

const KIND_LABEL: Record<Change["kind"], string> = {
  invalid: "không phải âm tiết tiếng Việt",
  unattested: "chưa từng thấy trong corpus",
  confused: "có thể nhầm từ",
  punctuation: "dấu câu / khoảng trắng",
  capitalization: "viết hoa",
  diacritic: "thêm dấu",
};

/** Loại nào là lỗi chính tả của một *từ* — chỉ những loại này mới bỏ qua được. */
const isWordKind = (k: Change["kind"]) =>
  k === "invalid" || k === "unattested" || k === "confused";

async function load() {
  review = await getReview();
  picks = new Map();
  errorBox.hidden = true;
  copyBtn.textContent = "Chép";

  if (!review) {
    titleBox.textContent = "Writa";
    appBox.textContent = "";
    message("Không có gì để xem lại.");
    await fit();
    return;
  }

  titleBox.textContent = review.mode === "diacritic" ? "Thêm dấu" : "Kiểm tra chính tả";
  appBox.textContent = review.app;

  // Mặc định chọn hết: user bấm phím tắt là đã muốn sửa, bắt tick từng cái thì phím
  // tắt chẳng nhanh hơn tự sửa tay. Chỗ nào engine không nghĩ ra cách sửa thì không
  // có gì để chọn.
  for (const c of review.changes) {
    if (c.options.length > 0) picks.set(c.id, c.options[0]);
  }

  render();
  await fit();
}

function message(text: string) {
  listBox.replaceChildren(
    Object.assign(document.createElement("div"), {
      className: "empty",
      textContent: text,
    }),
  );
  previewBox.hidden = true;
  applyBtn.disabled = true;
  copyBtn.disabled = true;
  applyBtn.textContent = "Áp dụng";
}

function render() {
  if (!review) return;

  if (review.notice) {
    message(review.notice);
    return;
  }
  if (review.changes.length === 0) {
    message(
      review.mode === "diacritic"
        ? "Đoạn này đã có dấu đầy đủ."
        : "Không tìm thấy lỗi nào.",
    );
    return;
  }

  listBox.replaceChildren(...review.changes.map(renderItem));
  renderPreview();
  copyBtn.disabled = false;
  applyBtn.disabled = picks.size === 0;
  applyBtn.textContent =
    picks.size === review.changes.length
      ? `Áp dụng ${picks.size} sửa`
      : `Áp dụng ${picks.size}/${review.changes.length}`;
}

function renderItem(c: Change): HTMLElement {
  const row = document.createElement("div");
  row.className = "item";
  if (!c.certain) row.classList.add("likely");
  if (!picks.has(c.id)) row.classList.add("off");

  const from = document.createElement("span");
  from.className = "from";
  from.textContent = display(c.from);

  const mid = document.createElement("div");
  let head: HTMLElement;

  if (c.options.length === 0) {
    // Bắt được lỗi mà không nghĩ ra cách sửa. Vẫn phải hiện — giấu đi thì user tưởng
    // câu mình đúng.
    const box = document.createElement("input");
    box.type = "checkbox";
    box.disabled = true;
    head = box;

    const note = document.createElement("span");
    note.className = "arrow";
    note.textContent = "· chưa có gợi ý, sửa tay giúp";
    mid.append(from, note);
  } else {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = picks.has(c.id);
    box.title = KIND_LABEL[c.kind];
    head = box;

    const arrow = document.createElement("span");
    arrow.className = "arrow";
    arrow.textContent = "→";

    const pick = document.createElement("select");
    for (const opt of c.options) {
      pick.append(new Option(display(opt), opt));
    }
    pick.value = picks.get(c.id) ?? c.options[0];

    box.addEventListener("change", () => {
      if (box.checked) picks.set(c.id, pick.value);
      else picks.delete(c.id);
      render();
    });
    pick.addEventListener("change", () => {
      picks.set(c.id, pick.value);
      render();
    });

    mid.append(from, arrow, pick);
  }

  const ctx = document.createElement("div");
  ctx.className = "ctx";
  ctx.append(
    document.createTextNode("…" + c.before),
    Object.assign(document.createElement("mark"), { textContent: display(c.from) }),
    document.createTextNode(c.after + "…"),
  );

  row.append(head, mid);

  if (isWordKind(c.kind)) {
    const skip = document.createElement("button");
    skip.className = "ignore";
    skip.textContent = "Bỏ qua từ này";
    skip.title = `Thêm “${c.from}” vào từ điển cá nhân — Writa sẽ không báo nữa`;
    skip.addEventListener("click", async () => {
      await ignoreWord(c.from);
      if (!review) return;
      const dropped = review.changes.filter((x) => x.from === c.from);
      review.changes = review.changes.filter((x) => x.from !== c.from);
      for (const d of dropped) picks.delete(d.id);
      render();
      await fit();
    });
    row.append(skip);
  } else {
    row.append(document.createElement("span"));
  }

  row.append(ctx);
  return row;
}

/** Khoảng trắng và xuống dòng phải nhìn thấy được, nếu không lỗi dấu câu vô hình. */
function display(s: string) {
  return s.replace(/\n/g, "⏎").replace(/ /g, "·");
}

function renderPreview() {
  if (!review) return;
  previewBox.hidden = false;
  previewBox.replaceChildren(
    ...previewSegments(review.original, review.changes, picks).map((seg) => {
      if (!seg.changed) return document.createTextNode(seg.text);
      const ins = document.createElement("ins");
      ins.textContent = seg.text;
      return ins;
    }),
  );
}

/** Báo chiều cao cần cho Rust — nó đặt kích thước, định vị, rồi mới hiện cửa sổ. */
async function fit() {
  await new Promise(requestAnimationFrame);
  const h = Math.min(Math.max(document.body.scrollHeight, 110), 560);
  await fitPopup(h);
}

function fail(text: string) {
  errorBox.hidden = false;
  errorBox.textContent = text;
  void fit();
}

applyBtn.addEventListener("click", async () => {
  if (!review || picks.size === 0) return;
  applyBtn.disabled = true;
  const outcome = await applyReview(
    [...picks].map(([id, replacement]) => ({ id, replacement })),
  );
  if (outcome.ok) return; // backend đã ẩn cửa sổ
  applyBtn.disabled = false;
  fail(
    (outcome.error ?? "Không ghi được vào app đích.") +
      " Bấm “Chép” rồi dán tay bằng Ctrl+V.",
  );
});

copyBtn.addEventListener("click", async () => {
  if (!review) return;
  await copyToClipboard(previewText(review.original, review.changes, picks));
  copyBtn.textContent = "Đã chép";
  setTimeout(() => (copyBtn.textContent = "Chép"), 1200);
});

closeBtn.addEventListener("click", () => void dismissReview());

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    void dismissReview();
  } else if (e.key === "Enter" && !e.isComposing) {
    // Enter trong <select> đang mở là để chọn phương án, không phải để áp dụng.
    if ((e.target as HTMLElement).tagName === "SELECT") return;
    e.preventDefault();
    if (!applyBtn.disabled) applyBtn.click();
  }
});

// Bấm phím tắt lần nữa: nạp nội dung mới. Đây cũng là tín hiệu duy nhất báo cửa sổ
// sắp hiện — Rust chỉ hiện popup sau khi `fit()` báo lại chiều cao.
void listen("writa://review", () => void load());

void load();
