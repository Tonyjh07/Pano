<script lang="ts">
  import AdapterTable from "./adapters/AdapterTable.svelte";
  import SettingsPage from "./settings/SettingsPage.svelte";

  let tab: "adapters" | "settings" = "adapters";
  let toast: { kind: "ok" | "error"; text: string } | null = null;
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function notify(kind: "ok" | "error", text: string) {
    toast = { kind, text };
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 2500);
  }
</script>

<div class="manager">
  <header class="manager-header">
    <span class="logo">Pano</span>
    <nav>
      <button
        class="tab"
        class:active={tab === "adapters"}
        onclick={() => (tab = "adapters")}
      >
        适配器
      </button>
      <button
        class="tab"
        class:active={tab === "settings"}
        onclick={() => (tab = "settings")}
      >
        设置
      </button>
    </nav>
  </header>
  <main>
    {#if tab === "adapters"}
      <AdapterTable {notify} />
    {:else}
      <SettingsPage />
    {/if}
  </main>
</div>

{#if toast}
  <div class="toast" class:error={toast.kind === "error"} class:ok={toast.kind === "ok"}>
    {toast.text}
  </div>
{/if}

<style>
  .manager {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .manager-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .logo {
    font-weight: 700;
    color: var(--accent);
    font-size: 15px;
  }
  nav {
    display: flex;
    gap: 4px;
  }
  .tab {
    background: transparent;
    border: none;
    padding: 6px 12px;
    border-radius: var(--radius);
  }
  .tab.active {
    background: var(--panel-2);
    color: var(--accent);
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 12px;
  }
</style>
