<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";

  let preview = "加载中…";
  let schemaVersion: number | null = null;

  onMount(async () => {
    schemaVersion = await api.configSchemaVersion().catch(() => null);
    preview = await api.configPreview().catch((e) => `读取失败：${e}`);
  });
</script>

<section>
  <h2>全局设置</h2>
  <p class="dim">配置 schema 版本：{schemaVersion ?? "—"}</p>
  <p class="dim">
    主题切换 / 默认采样 / 缓冲容量 / 窗口布局重置：归 M3 完善（当前为骨架）。
  </p>
  <h3>当前 pano.toml</h3>
  <pre>{preview}</pre>
</section>

<style>
  h2 {
    margin-top: 0;
  }
  .dim {
    color: var(--text-dim);
  }
  pre {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 12px;
    font-family: var(--mono);
    font-size: 12px;
    overflow: auto;
    max-height: 60vh;
    user-select: text;
  }
</style>
