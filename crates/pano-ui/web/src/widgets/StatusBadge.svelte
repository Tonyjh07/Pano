<script lang="ts">
  // 徽标配色只依赖稳定的布尔字段（running / lastError），不匹配中文文案。
  let { status, running, lastError = null }: {
    status: string;
    running: boolean;
    lastError?: string | null;
  } = $props();

  const cls = $derived(running ? "running" : lastError ? "error" : "stopped");
</script>

<span class="badge {cls}" title={lastError ?? status}>{status}</span>
