<script lang="ts">
  import { formatValue } from "../lib/api";

  let { title, value, unit = null }: {
    title: string;
    value: number | boolean | string | Record<string, unknown> | unknown[] | null | undefined;
    unit?: string | null;
  } = $props();
</script>

<div class="value-card">
  <span class="label">{title}</span>
  <span class="value">{value === undefined ? "—" : formatValue(value)}{unit ? ` ${unit}` : ""}</span>
</div>

<style>
  .value-card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    color: var(--text-dim);
    font-size: 12px;
  }
  .value {
    font-family: var(--mono);
    font-size: 22px;
    font-weight: 600;
  }
</style>
