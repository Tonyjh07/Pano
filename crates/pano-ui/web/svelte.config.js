import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  // vitePreprocess：<script lang="ts"> 与样式预处理
  preprocess: vitePreprocess(),
};
