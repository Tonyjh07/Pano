<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import { toPlotData, type SampleEvent } from "../lib/api";

  let { seriesName, samples, maxPoints = 300 }: {
    seriesName: string;
    samples: SampleEvent[];
    /** 曲线只取最近 N 点（ui.md §6），避免全量历史重算。 */
    maxPoints?: number;
  } = $props();

  let container: HTMLDivElement;
  let plot: uPlot | null = null;

  onMount(() => {
    const d = toPlotData(samples.slice(-maxPoints));
    const opts: uPlot.Options = {
      width: container.clientWidth,
      height: container.clientHeight,
      legend: { show: true },
      series: [
        { label: "时间" },
        {
          label: seriesName,
          stroke: "#4f9cf9",
          width: 2,
          points: { show: false },
        },
      ],
      axes: [
        { stroke: "#9d9d9d", grid: { stroke: "#333", width: 1 } },
        { stroke: "#9d9d9d", grid: { stroke: "#333", width: 1 } },
      ],
      scales: { x: { time: true } },
    };
    plot = new uPlot(opts, [d.x, d.y], container);

    // 窗口尺寸变化时自适应
    const ro = new ResizeObserver(() => {
      if (!plot) return;
      const w = container.clientWidth;
      const h = container.clientHeight;
      if (w > 0 && h > 0) plot.setSize({ width: w, height: h });
    });
    ro.observe(container);

    return () => ro.disconnect();
  });

  // 样本变化 → 增量更新曲线（响应式；高频事件已由订阅端节流合并）
  $effect(() => {
    if (!plot) return;
    const d = toPlotData(samples.slice(-maxPoints));
    plot.setData([d.x, d.y]);
  });

  onDestroy(() => plot?.destroy());
</script>

<div class="chart" bind:this={container}></div>

<style>
  .chart {
    width: 100%;
    height: 100%;
    min-height: 140px;
  }
</style>
