import { fileURLToPath } from "node:url";
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
    // Svelte 5 根导出在 Node（vitest）默认解析到 server 版（index-server.js），
    // 其中 `mount` 不可用（lifecycle_function_unavailable）。测试环境下把根
    // `svelte` 别名到 client 版（用绝对路径避开 package exports 的导出限制），
    // 使 @testing-library/svelte 的 render 能真正挂载组件。
    alias: [
      {
        find: /^svelte$/,
        replacement: fileURLToPath(
          new URL("./node_modules/svelte/src/index-client.js", import.meta.url),
        ),
      },
    ],
  },
});
