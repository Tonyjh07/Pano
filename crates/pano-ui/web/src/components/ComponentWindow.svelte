<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, type MonitorInfo, type SampleEvent, type WindowContent } from "../lib/api";
  import { onSample } from "../lib/events";
  import { resolveRenderer } from "./renderers";

  // 窗口壳（ui.md §4，M2.2）：负责数据管道（窗口内容查询、series 订阅 / 缓冲），
  // 渲染按组件类型 id 分派给对应渲染器（renderers.ts）。
  let content: WindowContent | null = $state(null);
  /** 本窗口消费的 series（由绑定的组件目录解析）。 */
  let seriesList: string[] = $state([]);
  /** series → 样本缓冲（最多 500 点，曲线画最近 300；$state 深响应）。 */
  let samplesBySeries = $state<Record<string, SampleEvent[]>>({});
  /** series → 最新值。 */
  let latest = $state<Record<string, SampleEvent>>({});
  let error: string | null = $state(null);
  let unlisten: (() => void) | null = null;
  let unlistenContent: (() => void) | null = null;

  // 窗口控制状态
  let pinned = $state(false);
  let fullscreen = $state(false);
  let monitors: MonitorInfo[] = $state([]);

  /** 载入本窗口的内容（组件 + series）并订阅样本（切换组件后重载）。 */
  async function load(label: string) {
    unlisten?.();
    const c = await api.windowContent(label);
    content = c;
    seriesList = c.series;
    samplesBySeries = {};
    latest = {};
    for (const s of seriesList) {
      const hist = await api.seriesHistory(s, 300);
      samplesBySeries[s] = hist;
      if (hist.length > 0) latest[s] = hist[hist.length - 1];
    }
    // 事件订阅（ui.md §6）：按本窗口 series 过滤；100ms 合并窗口
    unlisten = await onSample(
      (ev) => {
        latest[ev.series] = ev;
        const buf = samplesBySeries[ev.series] ?? [];
        buf.push(ev);
        if (buf.length > 500) buf.shift();
        samplesBySeries[ev.series] = buf;
      },
      seriesList,
      100,
    );
  }

  onMount(async () => {
    const label = getCurrentWindow().label;
    try {
      // 「窗口管理」切换组件事件：重载本窗口内容
      unlistenContent = await listen<WindowContent>("pano://window-component", () => {
        void load(label).catch((e) => (error = String(e)));
      });
      await load(label);
      monitors = await api.monitors().catch(() => []);
    } catch (e) {
      error = String(e);
    }
  });

  onDestroy(() => {
    unlisten?.();
    unlistenContent?.();
  });

  async function setPin(v: boolean) {
    pinned = v;
    await api.windowSetAlwaysOnTop(getCurrentWindow().label, v).catch((e) => (error = String(e)));
  }

  async function setFullscreen(v: boolean) {
    fullscreen = v;
    await api.windowSetFullscreen(getCurrentWindow().label, v).catch((e) => (error = String(e)));
  }

  async function setMonitor(id: string) {
    await api.windowSetMonitor(getCurrentWindow().label, id).catch((e) => (error = String(e)));
  }
</script>

{#if error}
  <div class="error-box">错误：{error}</div>
{:else if !content}
  <p class="dim load">加载中…</p>
{:else}
  <div class="window">
    <header class="win-header">
      <span class="component-name">{content.component || "（无组件）"}</span>
      <div class="controls">
        <button class:active={pinned} onclick={() => setPin(!pinned)} title="置顶">置顶</button>
        <button class:active={fullscreen} onclick={() => setFullscreen(!fullscreen)} title="全屏">
          全屏
        </button>
        {#if monitors.length > 0}
          <select
            value=""
            onchange={(e) => setMonitor((e.currentTarget as HTMLSelectElement).value)}
          >
            <option value="" disabled>绑定显示器…</option>
            {#each monitors as m}
              <option value={m.id}>
                {m.is_primary ? "主显示器" : m.name || m.id}
              </option>
            {/each}
          </select>
        {/if}
      </div>
    </header>
    <main>
      {#if seriesList.length === 0}
        <p class="dim">该组件未声明任何 series（数据源未启用？见管理窗口）。</p>
      {:else}
        {@const Renderer = resolveRenderer(content.component)}
        <Renderer seriesList={seriesList} samples={samplesBySeries} latest={latest} />
      {/if}
    </main>
  </div>
{/if}

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .win-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .component-name {
    color: var(--text-dim);
    font-size: 12px;
  }
  .controls {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  button.active {
    border-color: var(--accent);
    color: var(--accent);
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .dim {
    color: var(--text-dim);
  }
  .load {
    padding: 16px;
  }
  .error-box {
    margin: 16px;
    padding: 12px;
    border: 1px solid var(--err);
    border-radius: var(--radius);
    color: var(--err);
  }
</style>
