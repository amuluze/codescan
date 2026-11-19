import { readFileSync } from "node:fs";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

/** 与 vite.config.ts 一致：__APP_VERSION__ 来自 tauri.conf.json（顶部展示）。 */
const tauriVersion = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
).version as string;

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(tauriVersion),
  },
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./vitest.setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
