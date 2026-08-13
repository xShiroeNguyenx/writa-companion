// Hợp đồng dữ liệu giữa UI và Rust.
//
// Mọi kiểu ở đây phải khớp với `src-tauri/src/model.rs`. Rust dùng
// `#[serde(rename_all = "camelCase")]` nên tên trường viết camelCase ở cả hai phía.

import { invoke } from "@tauri-apps/api/core";

/** Loại thay đổi. Quyết định màu sắc và việc có được tự sửa hay không. */
export type ChangeKind =
  | "invalid" // L1 — âm tiết không tồn tại. Chắc chắn sai.
  | "unattested" // L2 — hợp lệ nhưng chưa thấy trong corpus. Nghi vấn.
  | "confused" // L4 — lỗi real-word. Chỉ gợi ý.
  | "punctuation" // L5 — dấu câu, khoảng trắng.
  | "capitalization" // L5 — viết hoa đầu câu.
  | "diacritic"; // P3 — thêm dấu.

export interface Change {
  id: number;
  /** Vị trí byte trong `Review.original`. */
  start: number;
  end: number;
  kind: ChangeKind;
  /** Đoạn trong bản gốc. */
  from: string;
  /** Phương án thay thế, tốt nhất trước. Luôn có ít nhất một. */
  options: string[];
  /** Sai chắc chắn (không cần ngữ cảnh phán quyết). */
  certain: boolean;
  /** Ngữ cảnh hai bên, đã cắt ngắn, để user nhận ra chỗ nào trong câu. */
  before: string;
  after: string;
}

export type Mode = "check" | "diacritic";

export interface Review {
  mode: Mode;
  /** Tên exe của app đang focus, hoặc `"clipboard"`. */
  app: string;
  original: string;
  changes: Change[];
  /** Lời nhắn thay cho danh sách lỗi — "chưa bôi đen đoạn nào", v.v. */
  notice: string | null;
}

/** Một thay đổi user đồng ý áp dụng. */
export interface Decision {
  id: number;
  replacement: string;
}

export interface Settings {
  paused: boolean;
  hotkeyCheck: string;
  hotkeyDiacritic: string;
  hotkeyAccept: string;
  realtime: boolean;
  autoFix: boolean;
  realWordMargin: number;
  flagUnattested: boolean;
  checkPunctuation: boolean;
  checkCapitalization: boolean;
  typographicStyle: boolean;
  autostart: boolean;
  autoUpdate: boolean;
  personalDict: string[];
  blocklist: string[];
}

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
  date: string | null;
}

export const checkUpdate = () => invoke<UpdateInfo | null>("check_update");
export const installUpdate = () => invoke<void>("install_update");

export interface EngineInfo {
  version: string;
  /** Âm tiết sinh ra từ bảng ngữ âm. */
  syllables: number;
  /** Trong số đó, bao nhiêu thực sự xuất hiện trong corpus. */
  attested: number;
  acceptedForeign: number;
  compounds: number;
  trigrams: number;
  /** Danh sách app Writa không bao giờ đụng tới, đến từ code. */
  defaultBlocklist: string[];
}

/** Kết quả một lần ghi ngược vào app đích. */
export interface ApplyOutcome {
  ok: boolean;
  /** Text cuối cùng — luôn có, kể cả khi ghi thất bại, để user chép tay. */
  text: string;
  /** Lý do thất bại, tiếng Việt, để hiện thẳng cho user. */
  error: string | null;
}

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) =>
  invoke<Settings>("save_settings", { settings });
export const engineInfo = () => invoke<EngineInfo>("engine_info");

export const getReview = () => invoke<Review | null>("get_review");
export const applyReview = (decisions: Decision[]) =>
  invoke<ApplyOutcome>("apply_review", { decisions });
export const dismissReview = () => invoke<void>("dismiss_review");
export const ignoreWord = (word: string) => invoke<void>("ignore_word", { word });
export const copyToClipboard = (text: string) =>
  invoke<void>("copy_to_clipboard", { text });

/**
 * Báo chiều cao popup cần, để Rust đặt kích thước rồi **mới** hiện cửa sổ.
 *
 * Không tự gọi `setSize` từ đây vì chỉ Rust biết caret ở đâu, và việc đặt kích thước
 * với việc định vị phải xảy ra cùng lúc — đổi cao xong mới kẹp lại vào màn hình thì
 * popup nhảy một nhịp trước mắt user.
 */
export const fitPopup = (height: number) => invoke<void>("fit_popup", { height });

export interface Segment {
  text: string;
  /** Đoạn này là phần đã thay, để bản xem trước tô sáng được. */
  changed: boolean;
}

/**
 * Dựng bản xem trước từ các thay đổi user đang chọn.
 *
 * Span của engine là **vị trí byte** trong bản gốc, còn chuỗi JavaScript là UTF-16
 * — `"tôi".length` là 3 nhưng chiếm 4 byte. Cắt bằng `slice()` sẽ lệch ngay từ dấu
 * tiếng Việt đầu tiên, nên phải đi qua `TextEncoder`.
 */
export function previewSegments(
  original: string,
  changes: Change[],
  picks: Map<number, string>,
): Segment[] {
  const bytes = new TextEncoder().encode(original);
  const decoder = new TextDecoder();
  const chosen = changes.filter((c) => picks.has(c.id)).sort((a, b) => a.start - b.start);

  const out: Segment[] = [];
  let at = 0;
  for (const c of chosen) {
    if (c.start > at) {
      out.push({ text: decoder.decode(bytes.subarray(at, c.start)), changed: false });
    }
    out.push({ text: picks.get(c.id)!, changed: true });
    at = c.end;
  }
  if (at < bytes.length) {
    out.push({ text: decoder.decode(bytes.subarray(at)), changed: false });
  }
  return out;
}

export const previewText = (
  original: string,
  changes: Change[],
  picks: Map<number, string>,
) => previewSegments(original, changes, picks).map((s) => s.text).join("");
