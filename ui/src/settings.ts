// Cửa sổ cài đặt.
//
// Lưu ngay khi đổi thay vì có nút "Lưu": mọi thiết lập ở đây đều đảo ngược được
// bằng một cú bấm, nên bước xác nhận chỉ tổ thêm việc. Ngoại lệ duy nhất là phím
// tắt — đăng ký có thể thất bại vì app khác đã chiếm, nên backend trả về thiết lập
// **thật sự đang chạy** và UI vẽ lại theo đó.

import { listen } from "@tauri-apps/api/event";
import {
  checkUpdate,
  engineInfo,
  getSettings,
  installUpdate,
  saveSettings,
  type EngineInfo,
  type Settings,
  type UpdateInfo,
} from "./api";
import "./style.css";

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const pauseBtn = el<HTMLButtonElement>("toggle-pause");
const toastBox = el<HTMLDivElement>("toast");

/** Mọi thiết lập bật/tắt, để một vòng lặp lo cả đọc lẫn ghi. */
// `detectRealWord` KHÔNG có ở đây một cách có chủ ý.
//
// Nó vốn là núm để **đo** phần đóng góp của lớp real-word vào false-positive
// (`writa-cli scan --no-realword`), và để nó lọt vào UI là một sai lầm: user tắt nó
// thì `chia sẽ`, `sữa lỗi`, `xử dụng` — nhóm lỗi người Việt mắc nhiều nhất — biến mất
// khỏi tầm nhìn, còn app thì trông như hỏng chứ không như đã bị tắt bớt. Chuyện này đã
// xảy ra thật. "Độ nhạy" đã phủ hết nhu cầu chính đáng, kể cả mức thận trọng nhất.
const BOOL_KEYS = [
  "realtime",
  "autoFix",
  "flagUnattested",
  "checkPunctuation",
  "checkCapitalization",
  "typographicStyle",
  "autostart",
  "autoUpdate",
] as const;
type BoolKey = (typeof BOOL_KEYS)[number];

const checks: Record<BoolKey, HTMLInputElement> = {
  realtime: el<HTMLInputElement>("realtime"),
  autoFix: el<HTMLInputElement>("auto-fix"),
  flagUnattested: el<HTMLInputElement>("flag-unattested"),
  checkPunctuation: el<HTMLInputElement>("check-punctuation"),
  checkCapitalization: el<HTMLInputElement>("check-capitalization"),
  typographicStyle: el<HTMLInputElement>("typographic-style"),
  autostart: el<HTMLInputElement>("autostart"),
  autoUpdate: el<HTMLInputElement>("auto-update"),
};

/** Ô nhập phím tắt → tên trường trong Settings. */
const HOTKEYS = {
  hotkeyCheck: el<HTMLInputElement>("hotkey-check"),
  hotkeyDiacritic: el<HTMLInputElement>("hotkey-diacritic"),
  hotkeyAccept: el<HTMLInputElement>("hotkey-accept"),
} satisfies Record<string, HTMLInputElement>;
type HotkeyKey = keyof typeof HOTKEYS;
const marginSel = el<HTMLSelectElement>("margin");
const marginNote = el<HTMLElement>("margin-note");

/**
 * Chú thích cho từng mức độ nhạy.
 *
 * Đây là số đo thật trên 35 nghìn lỗi đã tiêm và 50 nghìn câu held-out, không phải
 * ước lượng. Hiện thẳng cho user vì "độ nhạy" là một đánh đổi chứ không phải một
 * núm "tốt hơn" — họ có quyền biết mình đang đánh đổi cái gì.
 */
const MARGIN_NOTE: Record<string, string> = {
  "9": "Bắt 78% lỗi nhầm từ, gần như không bao giờ báo oan (0,13 lần / 1000 từ).",
  "6": "Bắt 90,7% lỗi nhầm từ, báo oan 0,53 lần / 1000 từ. Mặc định.",
  "4.5": "Bắt 94,1% lỗi nhầm từ, báo oan 1,20 lần / 1000 từ.",
  "3": "Bắt 96,6% lỗi nhầm từ, báo oan 2,52 lần / 1000 từ.",
};

let current: Settings | null = null;

function toast(message: string) {
  toastBox.textContent = message;
  toastBox.classList.add("show");
  setTimeout(() => toastBox.classList.remove("show"), 1800);
}

function paint(s: Settings) {
  current = s;
  for (const key of BOOL_KEYS) {
    checks[key].checked = s[key];
  }
  for (const key of Object.keys(HOTKEYS) as HotkeyKey[]) {
    HOTKEYS[key].value = s[key];
  }
  // Tự-sửa và phím áp dụng chỉ có nghĩa khi realtime đang bật.
  checks.autoFix.disabled = !s.realtime;
  HOTKEYS.hotkeyAccept.disabled = !s.realtime;

  // Ngưỡng có thể đến từ file cấu hình sửa tay, không chắc trùng mức nào.
  const asText = String(s.realWordMargin);
  if (!(asText in MARGIN_NOTE)) {
    const custom = new Option(`Tuỳ chỉnh (${asText})`, asText);
    marginSel.append(custom);
  }
  marginSel.value = asText;
  marginNote.textContent = MARGIN_NOTE[asText] ?? `Ngưỡng ${asText}.`;

  pauseBtn.textContent = s.paused ? "Bật lại" : "Tạm dừng";
  pauseBtn.classList.toggle("primary", s.paused);

  renderChips("dict-chips", s.personalDict, "Chưa có từ nào.", (w) => {
    s.personalDict = s.personalDict.filter((x) => x !== w);
    void push(s);
  });
  renderChips("block-chips", s.blocklist, "Chưa thêm app nào.", (w) => {
    s.blocklist = s.blocklist.filter((x) => x !== w);
    void push(s);
  });
}

function renderChips(
  id: string,
  items: string[],
  empty: string,
  onRemove: (item: string) => void,
) {
  const box = el<HTMLDivElement>(id);
  if (items.length === 0) {
    box.replaceChildren(
      Object.assign(document.createElement("span"), {
        className: "none",
        textContent: empty,
      }),
    );
    return;
  }
  box.replaceChildren(
    ...items.map((item) => {
      const chip = document.createElement("span");
      chip.className = "chip";
      chip.append(document.createTextNode(item));
      const x = document.createElement("button");
      x.textContent = "✕";
      x.title = `Bỏ “${item}”`;
      x.addEventListener("click", () => onRemove(item));
      chip.append(x);
      return chip;
    }),
  );
}

/** Gửi thiết lập xuống backend và vẽ lại theo thứ **thật sự** đang chạy. */
async function push(next: Settings) {
  const effective = await saveSettings(next);
  const hotkeyRefused = (Object.keys(HOTKEYS) as HotkeyKey[]).some(
    (k) => effective[k] !== next[k],
  );
  paint(effective);
  if (hotkeyRefused) {
    toast("Phím tắt đó đang bị app khác chiếm — giữ nguyên phím cũ.");
  }
}

function collect(): Settings {
  const s = { ...current! };
  for (const key of BOOL_KEYS) {
    s[key] = checks[key].checked;
  }
  for (const key of Object.keys(HOTKEYS) as HotkeyKey[]) {
    s[key] = HOTKEYS[key].value.trim();
  }
  s.realWordMargin = Number(marginSel.value);
  return s;
}

for (const box of Object.values(checks)) {
  box.addEventListener("change", () => void push(collect()));
}
marginSel.addEventListener("change", () => void push(collect()));

// Phím tắt chỉ gửi đi khi user rời ô — gửi từng ký tự thì mỗi lần gõ dở lại là một
// lần đăng ký hỏng.
for (const input of Object.values(HOTKEYS)) {
  input.addEventListener("change", () => void push(collect()));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") input.blur();
  });
}

pauseBtn.addEventListener("click", () => {
  const s = collect();
  s.paused = !s.paused;
  void push(s).then(() => toast(s.paused ? "Đã tạm dừng." : "Đang hoạt động."));
});

function wireAdd(inputId: string, buttonId: string, field: "personalDict" | "blocklist") {
  const input = el<HTMLInputElement>(inputId);
  const add = () => {
    const value = input.value.trim().toLowerCase();
    if (!value || !current) return;
    if (current[field].includes(value)) {
      input.value = "";
      return;
    }
    const s = collect();
    s[field] = [...s[field], value].sort();
    input.value = "";
    void push(s);
  };
  el<HTMLButtonElement>(buttonId).addEventListener("click", add);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      add();
    }
  });
}

wireAdd("dict-input", "dict-add", "personalDict");
wireAdd("block-input", "block-add", "blocklist");

function paintEngine(e: EngineInfo) {
  const n = (v: number) => v.toLocaleString("vi-VN");
  el<HTMLDivElement>("stats").replaceChildren(
    ...(
      [
        [n(e.syllables), "âm tiết hợp lệ"],
        [n(e.attested), "có trong corpus"],
        [n(e.compounds), "từ ghép"],
        [n(e.trigrams), "cụm 3 âm tiết"],
        [n(e.acceptedForeign), "từ vay mượn"],
        [e.version, "phiên bản"],
      ] as const
    ).map(([value, label]) => {
      const d = document.createElement("div");
      d.append(
        Object.assign(document.createElement("b"), { textContent: value }),
        Object.assign(document.createElement("small"), { textContent: label }),
      );
      return d;
    }),
  );
  el<HTMLSpanElement>("default-blocklist").textContent = e.defaultBlocklist.join(", ") + ".";
}

// Thiết lập cũng đổi được từ nơi khác: menu khay hệ thống, hay nút "Bỏ qua từ này"
// trong popup. Cửa sổ này có thể đang mở lúc đó, nên phải nghe để không hiển thị số
// liệu đã cũ.
void listen("writa://settings", () => void getSettings().then(paint));

// ------------------------------------------------------------------ cập nhật

const updateStatus = el<HTMLElement>("update-status");
const updateBtn = el<HTMLButtonElement>("check-update");

/// Bản mới đã tìm thấy, chờ user bấm cài. Tải và cài **không bao giờ** tự chạy.
let available: UpdateInfo | null = null;

async function lookForUpdate(manual: boolean) {
  updateBtn.disabled = true;
  updateStatus.textContent = "Đang kiểm tra…";
  try {
    available = await checkUpdate();
    if (available) {
      updateStatus.textContent = `Có bản ${available.version} (đang dùng ${available.current})`;
      updateBtn.textContent = "Cài và khởi động lại";
      updateBtn.classList.add("primary");
    } else {
      updateStatus.textContent = "Đang dùng bản mới nhất.";
      updateBtn.textContent = "Kiểm tra ngay";
      updateBtn.classList.remove("primary");
    }
  } catch (e) {
    // Chỉ nói khi user chủ động bấm. Lượt kiểm tra nền thất bại — mất mạng, chưa có
    // trang phát hành — không phải chuyện đáng làm phiền ai.
    available = null;
    updateStatus.textContent = manual ? `Không kiểm tra được: ${e}` : "—";
    updateBtn.textContent = "Kiểm tra ngay";
    updateBtn.classList.remove("primary");
  } finally {
    updateBtn.disabled = false;
  }
}

updateBtn.addEventListener("click", async () => {
  if (!available) {
    await lookForUpdate(true);
    return;
  }
  updateBtn.disabled = true;
  updateStatus.textContent = "Đang tải và cài…";
  try {
    await installUpdate(); // app khởi động lại nếu thành công
  } catch (e) {
    updateStatus.textContent = `${e}`;
    updateBtn.disabled = false;
  }
});

// Lượt kiểm tra nền tìm thấy bản mới trong lúc cửa sổ này đang mở.
void listen<UpdateInfo>("writa://update", (e) => {
  available = e.payload;
  updateStatus.textContent = `Có bản ${e.payload.version} (đang dùng ${e.payload.current})`;
  updateBtn.textContent = "Cài và khởi động lại";
  updateBtn.classList.add("primary");
});

async function boot() {
  const [s, e] = await Promise.all([getSettings(), engineInfo()]);
  paintEngine(e);
  paint(s);
  updateStatus.textContent = `Đang dùng ${e.version}`;
  if (s.autoUpdate) void lookForUpdate(false);
}

void boot();
