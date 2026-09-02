<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AdapterInfo, type WindowInfo } from "../../lib/api";

  let { notify }: { notify: (kind: "ok" | "error", text: string) => void } = $props();

  let windows: WindowInfo[] = $state([]);
  let adapters: AdapterInfo[] = $state([]);
  let loading = $state(true);

  // 新建窗口表单
  const draft = $state({ id: "", title: "" });
  const selected = $state<Record<string, boolean>>({});
  const busy = new Set<string>();

  async function refresh() {
    try {
      windows = await api.listWindows();
    } catch (e) {
      notify("error", `刷新窗口列表失败：${e}`);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    api
      .listAdapters()
      .then((a) => {
        adapters = a;
        // 默认勾选第一个适配器的全部 series（简化「分配适配器」）
        if (a.length > 0) {
          for (const s of a[0].series) selected[s] = true;
        }
      })
      .catch((e) => notify("error", `读取适配器失败：${e}`));
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });

  function toggleSeries(s: string) {
    selected[s] = !selected[s];
  }

  function selectedSeries(): string[] {
    return Object.keys(selected).filter((s) => selected[s]);
  }

  async function run(id: string, action: string, fn: () => Promise<void>) {
    if (busy.has(id)) return;
    busy.add(id);
    try {
      await fn();
      notify("ok", `${id}：${action}成功`);
    } catch (e) {
      notify("error", `${id}：${action}失败：${e}`);
    } finally {
      busy.delete(id);
      await refresh();
    }
  }

  async function create() {
    const series = selectedSeries();
    if (!draft.id.trim()) return notify("error", "窗口 id 不能为空");
    if (series.length === 0) return notify("error", "请至少勾选一个 series（分配适配器）");
    const id = draft.id.trim();
    await run(id, "新建", () =>
      api.createWindow(id, draft.title.trim() || id, series),
    );
    if (!busy.has(id)) {
      draft.id = "";
      draft.title = "";
      for (const s of Object.keys(selected)) selected[s] = false;
    }
  }

  function toggleVisible(w: WindowInfo) {
    void run(w.id, w.visible ? "隐藏" : "显示", () =>
      w.visible ? api.windowHide(w.id) : api.windowShow(w.id),
    );
  }

  /** 切换适配器：用当前「新建」勾选集合替换该窗口的 series。 */
  function switchSeries(w: WindowInfo) {
    const series = selectedSeries();
    if (series.length === 0) return notify("error", "请先勾选目标 series");
    void run(w.id, "切换 series", () => api.setWindowSeries(w.id, series));
  }

  function destroy(w: WindowInfo) {
    if (!window.confirm(`确定销毁窗口「${w.title}」（${w.id}）？`)) return;
    void run(w.id, "销毁", () => api.destroyWindow(w.id));
  }
</script>

{#if loading}
  <p class="dim">加载中…</p>
{:else}
  <section>
    <h2>窗口管理</h2>
    <p class="dim">
      管理窗口固定存在、不可销毁；监控窗口可新建 / 分配适配器 / 切换 series / 隐藏 / 销毁。
    </p>

    <div class="grid">
      <div class="panel">
        <h3>窗口列表</h3>
        {#if windows.length === 0}
          <p class="dim">暂无窗口。</p>
        {:else}
          <table>
            <thead>
              <tr>
                <th>窗口</th>
                <th>series</th>
                <th>可见</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each windows as w (w.id)}
                <tr>
                  <td>
                    <div class="win-title">
                      {w.title}
                      {#if w.is_manager}<span class="tag">管理</span>{/if}
                    </div>
                    <div class="win-id">{w.id}</div>
                  </td>
                  <td>
                    {#if w.series.length === 0}
                      <span class="dim">—</span>
                    {:else}
                      <div class="series-list">
                        {#each w.series as s}
                          <span class="series">{s}</span>
                        {/each}
                      </div>
                    {/if}
                  </td>
                  <td>
                    <span class={w.visible ? "ok" : "dim"}>{w.visible ? "显示" : "隐藏"}</span>
                  </td>
                  <td>
                    <div class="actions">
                      <button disabled={busy.has(w.id)} onclick={() => toggleVisible(w)}>
                        {w.visible ? "隐藏" : "显示"}
                      </button>
                      {#if !w.is_manager}
                        <button
                          disabled={busy.has(w.id)}
                          onclick={() => switchSeries(w)}
                          title="用右侧勾选的 series 替换该窗口内容"
                        >
                          切换 series
                        </button>
                        <button
                          class="danger"
                          disabled={busy.has(w.id)}
                          onclick={() => destroy(w)}
                        >
                          销毁
                        </button>
                      {/if}
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

      <div class="panel">
        <h3>新建窗口 / 分配适配器</h3>
        <label class="field">
          窗口 id（全小写 ASCII、连字符）
          <input
            placeholder="如 system-cpu"
            bind:value={draft.id}
            onkeydown={(e) => e.key === "Enter" && create()}
          />
        </label>
        <label class="field">
          标题（可选，默认 = id）
          <input placeholder="如 系统 CPU" bind:value={draft.title} />
        </label>

        <div class="adapter-group">
          {#if adapters.length === 0}
            <p class="dim">没有可用适配器。</p>
          {:else}
            {#each adapters as a (a.id)}
              <details class="adapter" open={a === adapters[0]}>
                <summary>
                  <span class="adapter-name">{a.name}</span>
                  <span class="adapter-id">{a.id}</span>
                </summary>
                {#if a.series.length === 0}
                  <p class="dim">该适配器未声明输出 series。</p>
                {:else}
                  {#each a.series as s}
                    <label class="checkbox">
                      <input
                        type="checkbox"
                        checked={!!selected[s]}
                        onchange={() => toggleSeries(s)}
                      />
                      <span class="mono">{s}</span>
                    </label>
                  {/each}
                {/if}
              </details>
            {/each}
          {/if}
        </div>

        <div class="form-actions">
          <button class="primary" onclick={() => create()} disabled={selectedSeries().length === 0}>
            新建窗口
          </button>
        </div>
      </div>
    </div>
  </section>
{/if}

<style>
  h2 {
    margin-top: 0;
  }
  .dim {
    color: var(--text-dim);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    align-items: start;
  }
  .panel {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 12px;
  }
  .win-title {
    font-weight: 600;
  }
  .win-id,
  .adapter-id {
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 11px;
  }
  .tag {
    margin-left: 4px;
    padding: 0 6px;
    border-radius: var(--radius);
    border: 1px solid var(--accent);
    color: var(--accent);
    font-size: 11px;
  }
  .series-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .series {
    padding: 1px 6px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-dim);
  }
  .ok {
    color: var(--ok);
  }
  .actions {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .actions .danger {
    border-color: var(--err);
    color: var(--err);
  }
  .field {
    display: block;
    margin-bottom: 8px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .field input {
    display: block;
    width: 100%;
    margin-top: 4px;
  }
  .adapter-group {
    margin-top: 8px;
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .adapter {
    margin-bottom: 6px;
  }
  .adapter summary {
    cursor: pointer;
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .checkbox {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 2px 0 2px 16px;
    cursor: pointer;
  }
  .mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .form-actions {
    margin-top: 10px;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
</style>
