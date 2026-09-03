<script lang="ts">
  // 资源仪表盘（M2.3，专用渲染器）：汽车仪表盘式资源总览。
  // 布局：上方左右两个大仪表（CPU / 内存占用率），下方左右两个小仪表
  // （磁盘活动率 / 网络利用率）；每仪表一个指示灯，超 high_threshold 亮红闪烁。
  import { onMount } from "svelte";
  import type { AdapterInfo, SampleEvent } from "../lib/api";
  import { api, configNumber } from "../lib/api";
  import Gauge from "./Gauge.svelte";

  let { seriesList, samples, latest }: {
    seriesList: string[];
    samples: Record<string, SampleEvent[]>;
    latest: Record<string, SampleEvent>;
  } = $props();

  // 各仪表 → series id 与适配器 id（阈值按适配器自定义配置 high_threshold 读取）
  const METRICS: {
    key: string;
    label: string;
    adapter: string;
    big: boolean;
    /** 副读数 series（M2.3.1：磁盘仪表显示最忙盘符）。 */
    detailSeries?: string;
  }[] = [
    { key: "sys.cpu.usage", label: "CPU 使用率", adapter: "sys.cpu", big: true },
    { key: "sys.mem.used_percent", label: "内存使用率", adapter: "sys.mem", big: true },
    {
      key: "sys.disk.active_percent",
      label: "磁盘活动",
      adapter: "sys.disk",
      big: false,
      detailSeries: "sys.disk.busiest_disk",
    },
    { key: "sys.net.utilization", label: "网络利用率", adapter: "sys.net", big: false },
  ];

  // 阈值（%）：listAdapters → config[].high_threshold；未配置回落 80。
  const DEFAULT_THRESHOLD = 80;
  let adapters: AdapterInfo[] = $state([]);
  let thresholds = $state<Record<string, number>>({});

  function metricValue(key: string): number | null {
    const v = latest[key]?.value;
    return typeof v === "number" && Number.isFinite(v) ? v : null;
  }

  function metricThreshold(adapter: string): number {
    const a = adapters.find((x) => x.id === adapter);
    return configNumber(a, "high_threshold", DEFAULT_THRESHOLD);
  }

  /** 副读数（如磁盘仪表的最忙盘符）；非文本值 / 缺失 → null。 */
  function metricDetail(series: string | undefined): string | null {
    if (!series) return null;
    const v = latest[series]?.value;
    return typeof v === "string" && v.length > 0 ? v : null;
  }

  onMount(() => {
    void api
      .listAdapters()
      .then((list) => {
        adapters = list;
        // 按需读取（listAdapters 常驻快照，切换窗口后重载）
      })
      .catch(() => {
        /* 阈值读取失败回落默认 80，不影响仪表显示 */
      });
  });
</script>

<div class="dashboard">
  {#each METRICS as m}
    {@const value = metricValue(m.key)}
    {@const thr = metricThreshold(m.adapter)}
    {@const detail = metricDetail(m.detailSeries)}
    <div class="cell" class:big={m.big}>
      <Gauge
        value={value}
        threshold={thr}
        label={m.label}
        size={m.big ? 280 : 200}
        detail={detail}
      />
    </div>
  {/each}
</div>

<style>
  .dashboard {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 12px;
    align-items: start;
    justify-items: center;
    height: 100%;
  }
  .cell {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
</style>
