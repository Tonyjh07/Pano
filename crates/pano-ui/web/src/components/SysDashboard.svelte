<script lang="ts">
  // 资源仪表盘（M2.3 / M2.4 升级）：汽车仪表盘式资源总览。
  // 布局：上方左右两个大仪表（CPU / 内存占用率），下方左右两个小仪表
  // （磁盘活动率 / 网络利用率）；每仪表一个指示灯，超 high_threshold 亮红闪烁。
  // 小分辨率自适应（M2.4）：ResizeObserver 观察容器高度，< COMPACT_H 时切换为
  // 紧凑横条布局（四个表盘横排、按可用宽度自适应尺寸）——支持 400×100 小屏。
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
    /** 紧凑模式短标签（表盘下空间有限，400×100 小屏场景）。 */
    short: string;
    adapter: string;
    big: boolean;
    /** 副读数 series（M2.3.1：磁盘仪表显示最忙盘符）。 */
    detailSeries?: string;
  }[] = [
    { key: "sys.cpu.usage", label: "CPU 使用率", short: "CPU", adapter: "sys.cpu", big: true },
    { key: "sys.mem.used_percent", label: "内存使用率", short: "内存", adapter: "sys.mem", big: true },
    {
      key: "sys.disk.active_percent",
      label: "磁盘活动",
      short: "磁盘",
      adapter: "sys.disk",
      big: false,
      detailSeries: "sys.disk.busiest_disk",
    },
    { key: "sys.net.utilization", label: "网络利用率", short: "网络", adapter: "sys.net", big: false },
  ];

  // 阈值（%）：listAdapters → config[].high_threshold；未配置回落 80。
  const DEFAULT_THRESHOLD = 80;
  let adapters: AdapterInfo[] = $state([]);
  let thresholds = $state<Record<string, number>>({});

  // 小分辨率自适应：容器高度低于阈值 → 紧凑横条；宽度决定紧凑表盘尺寸
  const COMPACT_H = 120; // px：与 ComponentWindow 折叠头部（140）配合，h<100 场景必命中
  let root: HTMLDivElement | undefined = $state();
  let compact = $state(false);
  let compactSize = $state(88);

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
    // 容器尺寸观察（小分辨率自适应）；非浏览器环境（单测）无 ResizeObserver → 常规模式
    if (typeof ResizeObserver === "undefined" || !root) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (!r) return;
      compact = r.height < COMPACT_H;
      if (compact) {
        const gapTotal = 8 * (METRICS.length - 1);
        const byWidth = Math.floor((r.width - gapTotal) / METRICS.length);
        // 垂直预算：面板 padding(8+6) + gap(2) + 读出行(~16) ≈ 32；
        // 表盘高 = size·0.66。400×100 小屏：main 内高约 78 → size ≤
        // (78-32)/0.66 ≈ 69，避免紧凑表盘底部读数被裁切。
        const byHeight = Math.floor((r.height - 32) / 0.66);
        compactSize = Math.max(48, Math.min(104, Math.min(byWidth, byHeight)));
      }
    });
    ro.observe(root);
    return () => ro.disconnect();
  });
</script>

<div
  class="dashboard"
  class:compact
  bind:this={root}
  data-pano-drag
>
  {#each METRICS as m}
    {@const value = metricValue(m.key)}
    {@const thr = metricThreshold(m.adapter)}
    {@const detail = metricDetail(m.detailSeries)}
    <div class="cell" class:big={m.big}>
      <Gauge
        value={value}
        threshold={thr}
        label={compact ? (detail ? `${m.short}·${detail}` : m.short) : m.label}
        size={compact ? compactSize : m.big ? 300 : 220}
        detail={compact ? null : detail}
      />
    </div>
  {/each}
</div>

<style>
  .dashboard {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 16px;
    align-items: start;
    justify-items: center;
    height: 100%;
  }
  /* 紧凑横条（小分辨率）：四个表盘一排，等分可用宽度 */
  .dashboard.compact {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    justify-content: center;
    gap: 8px;
    height: 100%;
  }
  .dashboard.compact .cell {
    flex: 0 0 auto;
  }
  .cell {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
</style>
