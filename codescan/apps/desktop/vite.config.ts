import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

/** 与 tauri.conf.json 版本同源，构建期注入 __APP_VERSION__（顶部展示）。 */
const tauriVersion = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
).version as string;

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(tauriVersion),
  },
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // WKWebView 会跨重启复用模块 HTTP 缓存；dev 下禁用，避免 UI 显示旧版本。
    headers: {
      "Cache-Control": "no-store",
    },
  },
  build: {
    target: "es2022",
    sourcemap: false,
  },
});
