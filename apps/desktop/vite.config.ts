import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
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
