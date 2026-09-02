<script lang="ts">
  import type { SampleEvent } from "../lib/api";
  import ValueCard from "./ValueCard.svelte";
  import TimeSeriesChart from "./TimeSeriesChart.svelte";

  // 通用渲染（M2.2）：组件固定 series 逐个渲染「数值卡 + 实时曲线」。
  // 由 ComponentWindow（窗口壳）提供数据管道：seriesList / samples / latest。
  let { seriesList, samples, latest }: {
    seriesList: string[];
    samples: Record<string, SampleEvent[]>;
    latest: Record<string, SampleEvent>;
  } = $props();
</script>

{#if seriesList.length === 0}
  <p class="dim">该组件未声明任何 series（数据源未启用？见管理窗口）。</p>
{:else}
  {#each seriesList as series}
    <section class="panel">
      <ValueCard title={series} value={latest[series]?.value} />
      <div class="chart-wrap">
        <TimeSeriesChart seriesName={series} samples={samples[series] ?? []} />
      </div>
    </section>
  {/each}
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .chart-wrap {
    height: 220px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px;
  }
  .dim {
    color: var(--text-dim);
  }
</style>
