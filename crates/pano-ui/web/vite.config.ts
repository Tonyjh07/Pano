import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri 官方推荐的 Vite 配置（多窗口 SPA：所有窗口加载同一入口，
// 前端按 getCurrentWindow().label 渲染管理窗口或组件窗口）。
export default defineConfig({
  plugins: [svelte()],
  // 防止 Vite 清屏干扰 Tauri 的进程输出
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Tauri 使用 Chromium WebView（Windows）／ WebKit（macOS/Linux）
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
