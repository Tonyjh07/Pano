<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, type MonitorInfo, type SampleEvent, type WindowContent } from "../lib/api";
  import { EVENT_WINDOW_COMPONENT, EVENT_WINDOW_DECORATIONS, onSample } from "../lib/events";
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

  // 无边框状态（M2.4）：false = 无边框窗口，内容区（data-pano-drag）可拖拽移动
  let decorations = $state(true);
  let unlistenDecor: (() => void) | null = null;

  // 窗口控制状态
  let pinned = $state(false);
  let fullscreen = $state(false);
  let monitors: MonitorInfo[] = $state([]);

  // 小分辨率自适应（M2.4）：窗口高度 < COMPACT_H → 折叠头部控件、压缩内容区 padding。
  // 阈值高于 SysDashboard 的紧凑阈值（120），保证 h<100 场景头部折叠后渲染器必命中紧凑布局。
  const COMPACT_H = 140;
  let compact = $state(false);
  let unlistenResize: (() => void) | null = null;
  let unlistenScale: (() => void) | null = null;

  /**
   * 监听窗口高度变化（Tauri 窗口尺寸 API；非 Tauri 环境回落常规模式）。
   * 注意：`innerSize()` / `onResized` / `onScaleChanged` 的 payload 均为
   * **物理像素**，而紧凑布局的 CSS 阈值为逻辑像素——高 DPI（缩放 ≠ 100%）
   * 下必须按 scaleFactor 换算，否则小屏紧凑判定错位（M2.4 修复）。
   * scaleFactor 随窗口跨显示器拖动变化，用可变闭包并在 onScaleChanged 时
   * 同步刷新，避免陈旧换算。
   */
  async function watchHeight() {
    const w = getCurrentWindow();
    let factor = 1;
    try {
      const updateFrom = (physicalHeight: number) => {
        compact = physicalHeight / factor < COMPACT_H;
      };
      factor = await w.scaleFactor();
      updateFrom((await w.innerSize()).height);
      unlistenResize = await w.onResized(({ payload }) => {
        updateFrom(payload.height);
      });
      unlistenScale = await w.onScaleChanged(({ payload }) => {
        factor = payload.scaleFactor;
        updateFrom(payload.size.height);
      });
    } catch {
      /* 尺寸不可用 → 常规模式 */
    }
  }

  /** 载入本窗口的内容（组件 + series）并订阅样本（切换组件后重载）。 */
  async function load(label: string) {
    unlisten?.();
    const c = await api.windowContent(label);
    content = c;
    decorations = c.decorations;
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
      await watchHeight();
      // 「窗口管理」切换组件事件：重载本窗口内容
      unlistenContent = await listen<WindowContent>(EVENT_WINDOW_COMPONENT, () => {
        void load(label).catch((e) => (error = String(e)));
      });
      // 无边框切换事件（M2.4，管理页开关 → 命令层定向 emit）
      unlistenDecor = await listen<boolean>(EVENT_WINDOW_DECORATIONS, (ev) => {
        decorations = ev.payload;
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
    unlistenDecor?.();
    unlistenResize?.();
    unlistenScale?.();
  });

  /**
   * 无边框窗口内容区拖拽移动（M2.4）：mousedown 落在 `data-pano-drag` 区域
   * 且不在交互控件上 → `startDragging`。有边框窗口由系统标题栏负责拖动。
   * 组件只声明 `data-pano-drag` 标记（纯 HTML 属性），平台调用收敛在窗口壳。
   */
  async function onMouseDown(e: MouseEvent) {
    if (decorations) return;
    const target = e.target as HTMLElement | null;
    if (!target || !target.closest("[data-pano-drag]")) return;
    if (target.closest("button, select, input, a, textarea")) return;
    e.preventDefault();
    try {
      await getCurrentWindow().startDragging();
    } catch {
      /* 平台不支持 / 环境异常：忽略 */
    }
  }

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
  <div class="window" role="presentation" onmousedown={onMouseDown}>
    <header class="win-header" class:compact>
      <span class="component-name" data-pano-drag>{content.component || "（无组件）"}</span>
      {#if !compact}
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
      {/if}
    </header>
    <main class:compact>
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
  /* 小分辨率：折叠头部 → 单行极简条，把高度让给仪表 */
  .win-header.compact {
    padding: 2px 6px;
    border-bottom: none;
  }
  .win-header.compact .component-name {
    font-size: 10px;
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
    /* 关键：flex 子项默认 min-height:auto，内容会把 main 撑开，导致
       ResizeObserver 测到的是内容高度（~500px）而非可视高度——小屏
       紧凑布局永不触发（用户实测 400×100 仍显示 2×2 的根因）。
       允许收缩后 main 高度 = 窗口可视高度，SysDashboard 的紧凑判定才真实。 */
    min-height: 0;
    overflow: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  /* 小分辨率：内容区去掉留白，交给紧凑仪表布局 */
  main.compact {
    overflow: hidden;
    padding: 2px;
    gap: 2px;
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
