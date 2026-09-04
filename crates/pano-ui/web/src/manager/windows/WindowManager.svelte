<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ComponentInfo, type WindowInfo } from "../../lib/api";

  let { notify }: { notify: (kind: "ok" | "error", text: string) => void } = $props();

  let windows: WindowInfo[] = $state([]);
  let components: ComponentInfo[] = $state([]);
  let loading = $state(true);

  // 新建窗口 / 切换组件 表单
  const draft = $state({ id: "", title: "" });
  let selectedComponent = $state("");
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
      .listComponents()
      .then((cs) => {
        components = cs;
        // 默认选中第一个可用组件（不可用组件置灰不可选）
        const first = cs.find((c) => c.available) ?? cs[0];
        selectedComponent = first?.id ?? "";
      })
      .catch((e) => notify("error", `读取组件目录失败：${e}`));
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });

  function componentName(id: string): string {
    return components.find((c) => c.id === id)?.name ?? id;
  }

  function componentAvailable(id: string): boolean {
    return components.find((c) => c.id === id)?.available ?? false;
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
    const id = draft.id.trim();
    if (!id) return notify("error", "窗口 id 不能为空");
    if (!selectedComponent) return notify("error", "请先选择组件");
    if (!componentAvailable(selectedComponent)) {
      return notify("error", "所选组件依赖的适配器未注册，不可用");
    }
    const title = draft.title.trim() || undefined;
    await run(id, "新建", () => api.createWindow(id, selectedComponent, title));
    if (!busy.has(id)) {
      draft.id = "";
      draft.title = "";
    }
  }

  function toggleVisible(w: WindowInfo) {
    void run(w.id, w.visible ? "隐藏" : "显示", () =>
      w.visible ? api.windowHide(w.id) : api.windowShow(w.id),
    );
  }

  /** 无边框切换（M2.4）：去掉系统边框，内容区（data-pano-drag）拖拽移动。 */
  function toggleDecorations(w: WindowInfo) {
    void run(w.id, w.decorations ? "恢复边框" : "无边框", () =>
      api.windowSetDecorations(w.id, !w.decorations),
    );
  }

  /** 切换组件：把表单选中的组件类型应用到目标窗口（仅换内容 / series，标题不动）。 */
  function switchComponent(w: WindowInfo) {
    if (!selectedComponent) return notify("error", "请先选择目标组件");
    if (!componentAvailable(selectedComponent)) {
      return notify("error", "所选组件依赖的适配器未注册，不可用");
    }
    if (selectedComponent === w.component) {
      return notify("error", "目标窗口已是该组件");
    }
    void run(w.id, "切换组件", () => api.setWindowComponent(w.id, selectedComponent));
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
      每个监控窗口分配一个 UI 组件（自带固定 series）；可新建、切换组件、隐藏 / 销毁。
      管理窗口固定存在、不可销毁。
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
                <th>组件</th>
                <th>series</th>
                <th>可见</th>
                <th>边框</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each windows as w (w.id)}
                <tr>
                  <td>
                    <div class="win-title">
                      {w.title}
                      {#if w.is_manager}
                        <span class="tag">管理</span>
                      {/if}
                    </div>
                    <div class="mono dim">{w.id}</div>
                  </td>
                  <td>
                    {#if w.component}
                      <span title={componentName(w.component)}>{componentName(w.component)}</span>
                    {:else}
                      <span class="dim">—</span>
                    {/if}
                  </td>
                  <td>
                    <div class="series-list">
                      {#if w.series.length === 0}
                        <span class="dim">无</span>
                      {:else}
                        {#each w.series as s}
                          <span class="mono dim">{s}</span>
                        {/each}
                      {/if}
                    </div>
                  </td>
                  <td>
                    <span class:off={!w.visible}>{w.visible ? "显示" : "隐藏"}</span>
                  </td>
                  <td>
                    {#if w.is_manager}
                      <span class="dim">—</span>
                    {:else}
                      <button
                        onclick={() => toggleDecorations(w)}
                        disabled={busy.has(w.id)}
                        title={w.decorations
                          ? "有系统边框：点击切换为无边框（内容区可拖拽移动）"
                          : "无边框：点击恢复系统边框"}
                      >
                        {w.decorations ? "有边框" : "无边框"}
                      </button>
                    {/if}
                  </td>
                  <td>
                    <div class="row-actions">
                      <button onclick={() => toggleVisible(w)} disabled={busy.has(w.id)}>
                        {w.visible ? "隐藏" : "显示"}
                      </button>
                      {#if !w.is_manager}
                        <button
                          onclick={() => switchComponent(w)}
                          disabled={busy.has(w.id) || !selectedComponent}
                          title="用右侧表单选中的组件替换本窗口内容"
                        >
                          切换组件
                        </button>
                        <button
                          class="danger"
                          onclick={() => destroy(w)}
                          disabled={busy.has(w.id)}
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
        <h3>新建窗口 / 切换组件</h3>
        <p class="dim">
          选择组件后「新建」；或对列表中的监控窗口点「切换组件」应用所选组件。
          组件自带固定 series；依赖适配器未注册（feature 未编译）的组件置灰不可选。
        </p>

        <div class="field">
          <label for="wm-win-id">窗口 id（新建）</label>
          <input
            id="wm-win-id"
            bind:value={draft.id}
            placeholder="如 cpu-monitor（全小写连字符）"
            onkeydown={(e) => e.key === "Enter" && create()}
          />
        </div>
        <div class="field">
          <label for="wm-win-title">标题（可选，缺省 = 组件默认标题）</label>
          <input id="wm-win-title" bind:value={draft.title} placeholder="留空使用组件默认标题" />
        </div>
        <div class="field">
          <label for="wm-component">UI 组件</label>
          <select id="wm-component" bind:value={selectedComponent}>
            {#each components as c}
              <option value={c.id} disabled={!c.available}>
                {c.name}（{c.id}）{#if !c.available}— 适配器未注册{/if}
              </option>
            {/each}
          </select>
          {#if selectedComponent}
            <p class="dim small">
              该组件 series：
              {#if (components.find((c) => c.id === selectedComponent)?.series ?? []).length === 0}
                无
              {:else}
                {(components.find((c) => c.id === selectedComponent)?.series ?? []).join("、")}
              {/if}
            </p>
          {/if}
        </div>
        <div class="form-actions">
          <button class="primary" onclick={create} disabled={!selectedComponent}>
            新建窗口
          </button>
        </div>
      </div>
    </div>
  </section>
{/if}

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 {
    font-size: 16px;
  }
  .dim {
    color: var(--text-dim);
  }
  .small {
    font-size: 12px;
    margin-top: 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 12px;
    align-items: start;
  }
  .panel {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    font-size: 14px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  th {
    font-size: 12px;
    color: var(--text-dim);
  }
  .win-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .tag {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 5px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .series-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .off {
    color: var(--text-dim);
  }
  .row-actions {
    display: flex;
    gap: 4px;
  }
  button {
    padding: 3px 8px;
    font-size: 12px;
  }
  .danger {
    color: var(--err);
    border-color: var(--err);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field label {
    font-size: 12px;
    color: var(--text-dim);
  }
  .field input,
  .field select {
    width: 100%;
  }
  .form-actions {
    margin-top: 6px;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
</style>
