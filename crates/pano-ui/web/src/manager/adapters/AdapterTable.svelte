<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AdapterInfo } from "../../lib/api";
  import StatusBadge from "../../widgets/StatusBadge.svelte";

  let { notify }: { notify: (kind: "ok" | "error", text: string) => void } = $props();

  let adapters: AdapterInfo[] = $state([]);
  let loading = $state(true);
  const busy = new Set<string>();
  /** 采样间隔草稿（未提交的输入值，提交 = 失焦 / 回车）。 */
  const drafts: Record<string, string> = {};

  async function refresh() {
    try {
      adapters = await api.listAdapters();
    } catch (e) {
      notify("error", `刷新适配器列表失败：${e}`);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    // 状态刷新：聚焦时重拉（数据本身走事件推送，不轮询，ui.md §6）。
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });

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

  function toggleEnabled(a: AdapterInfo) {
    void run(a.id, a.enabled ? "停用" : "启用", () => api.setAdapterEnabled(a.id, !a.enabled));
  }

  function applySampling(a: AdapterInfo) {
    const raw = drafts[a.id] ?? String(a.sampling_ms);
    const ms = Number(raw);
    if (!Number.isFinite(ms) || ms <= 0) {
      notify("error", `${a.id}：采样间隔必须为正数`);
      return;
    }
    if (ms === a.sampling_ms) return;
    void run(a.id, "采样间隔修改", async () => {
      await api.setAdapterSampling(a.id, ms);
      delete drafts[a.id]; // 成功后清理草稿，避免刷新后残留过期输入
    });
  }

  function restart(a: AdapterInfo) {
    void run(a.id, "重启", () => api.restartAdapter(a.id));
  }
</script>

{#if loading}
  <p class="dim">加载中…</p>
{:else if adapters.length === 0}
  <p class="dim">没有可用的适配器（未启用任何 feature？）</p>
{:else}
  <table>
    <thead>
      <tr>
        <th>状态</th>
        <th>适配器</th>
        <th>能力</th>
        <th>采样间隔</th>
        <th>启用</th>
        <th>操作</th>
      </tr>
    </thead>
    <tbody>
      {#each adapters as a (a.id)}
        <tr>
          <td>
            <StatusBadge status={a.status} running={a.running} lastError={a.last_error} />
          </td>
          <td>
            <div class="adapter-name">{a.name}</div>
            <div class="adapter-id">{a.id} · v{a.version}</div>
            <div class="adapter-desc">{a.description}</div>
          </td>
          <td>
            {#each a.capabilities as cap}
              <span class="cap" class:remote={cap === "RemoteSource"}>{cap}</span>
            {/each}
          </td>
          <td>
            <input
              type="number"
              min="1"
              value={drafts[a.id] ?? a.sampling_ms}
              oninput={(e) => (drafts[a.id] = (e.currentTarget as HTMLInputElement).value)}
              onchange={() => applySampling(a)}
              onkeydown={(e) => e.key === "Enter" && applySampling(a)}
            />
            <span class="unit">ms</span>
          </td>
          <td>
            <input
              type="checkbox"
              checked={a.enabled}
              disabled={busy.has(a.id)}
              onchange={() => toggleEnabled(a)}
            />
          </td>
          <td>
            <button
              disabled={busy.has(a.id) || !a.last_error}
              onclick={() => restart(a)}
              title={a.last_error ?? "仅在 Error 状态可用"}
            >
              重启
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .dim {
    color: var(--text-dim);
  }
  .adapter-name {
    font-weight: 600;
  }
  .adapter-id {
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 11px;
  }
  .adapter-desc {
    color: var(--text-dim);
    font-size: 12px;
  }
  .cap {
    display: inline-block;
    margin: 2px;
    padding: 1px 6px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-dim);
  }
  .cap.remote {
    border-color: var(--warn);
    color: var(--warn);
  }
  .unit {
    color: var(--text-dim);
    margin-left: 4px;
  }
</style>
