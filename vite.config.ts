import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

// Frontend nằm ở `ui/`, còn package.json ở gốc — để `npm run tauri` tìm thấy
// `src-tauri/` mà không phải dò ngược thư mục.
const ui = (file: string) => fileURLToPath(new URL(`./ui/${file}`, import.meta.url));

export default defineConfig({
  root: ui(""),
  // Tauri log build của mình ở đây, đừng để Vite xoá màn hình mất.
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // WebView2 trên Windows 10/11 luôn là Chromium khá mới; nhắm thẳng vào đó thay
    // vì hạ chuẩn xuống ES2015 rồi phình bundle.
    target: "chrome110",
    rollupOptions: {
      // Đường dẫn tuyệt đối: input tương đối được Rollup giải theo cwd chứ không
      // theo `root`, và cwd khác nhau tuỳ chỗ gọi (npm run vs tauri CLI).
      input: {
        settings: ui("index.html"),
        popup: ui("popup.html"),
        inline: ui("inline.html"),
      },
    },
  },
});
