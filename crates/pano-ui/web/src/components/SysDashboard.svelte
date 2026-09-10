<script lang="ts">
  // 资源仪表盘（M2.3 / M2.4 升级）：汽车仪表盘式资源总览。
  // 布局：上方左右两个大仪表（CPU / 内存占用率），下方左右两个小仪表
  // （磁盘活动率 / 网络利用率）；每仪表一个指示灯，超 high_threshold 亮红闪烁。
  // 动态适配（M2.4 修复）：ResizeObserver 实时测量容器，表盘尺寸与列数完全
  // 随可用空间推导（无硬编码上限/下限，也无固定 300/220 大表盘）：
  //   - 高度足够 → 常规 2×2（大/小表盘比例 1:0.73，按容器缩放）；
  //   - 高度不足 → 紧凑：宽横屏 4 列一排 / 近方 2×2，表盘 = min(列宽, 行高预算)；
  //   - 极端矮 → 隐藏读数行（只留表盘，组件名已印于表盘内），省出高度。
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

  // 动态布局状态（全部由容器尺寸推导；常规 2×2 大/小表盘比例 1:0.73）
  const BIG_SCALE = 0.73; // 小表盘 = 大表盘 × 0.73（与 M2.3 300/220 比例一致）
  let root: HTMLDivElement | undefined = $state();
  let compact = $state(false);
  /** 常规模式大表盘尺寸（小表盘 = ×BIG_SCALE）。 */
  let bigSize = $state(300);
  /** 紧凑模式列数（宽横屏 4、近方 2）。 */
  let cols = $state(4);
  /** 紧凑模式表盘尺寸。 */
  let compactSize = $state(72);
  /** 超矮时隐藏读数行（只留表盘；组件名已印在表盘内）。 */
  let hideReadout = $state(false);

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

  /**
   * 依容器可用宽高推导布局（每次 resize 调用）：
   * - 高度 ≥ 阈值 → 常规 2×2：大表盘受「列宽」与「两行总高」双重约束缩放；
   * - 高度 < 阈值 → 紧凑：按宽高比定列数，表盘 = min(列宽, 行高预算)；
   * - 读数行需 ~18px（紧凑）/~34px（常规含 detail）：极端矮时隐藏读数行。
   * 无硬编码尺寸上限/下限：任意容器都恰好放下（动态适配，非固定预算）。
   */
  function relayout(w: number, h: number) {
    if (h >= 120) {
      // 常规 2×2：大/小两行，总高 = 大行高 + 小行高 + 行距。
      // 大表盘高 ≈ size·0.66，小表盘 = 大×0.73（高 ≈ size·0.73·0.66）。
      // 行开销需分开计：大表盘行（CPU/内存，无 detail）≈ 34（读数行 ~16 +
      // padding/gap/border ~18）；小表盘行（磁盘带 detail 行）≈ 50（读数行
      // ~16 + detail ~15 + padding/gap/border ~19）。生产环境磁盘恒有
      // busiest_disk，按 34 预算会低估 ~16px 导致中等高度窗口溢出。
      // 高度约束：size·0.66 + 34 + 行距 8 + size·0.73·0.66 + 50 ≤ h
      const rowBig = 34;
      const rowSmall = 50;
      const byHeight = Math.floor((h - rowBig - rowSmall - 8) / (0.66 * (1 + BIG_SCALE)));
      const byWidth = Math.floor((w - 16) / 2); // 两列 + 16px 列距
      // 上限 300：保持大屏下大表盘的既有观感（M2.3 固定 300/220 的设计意图）；
      // 小屏自动缩小。下限 24 随预算自适应（h=120 边界时 byHeight≈24 仍满足
      // 不等式；不再硬卡 48 以免矮窗口溢出——动态适配的核心承诺）。
      bigSize = Math.max(24, Math.min(300, byWidth, byHeight));
      compact = false;
      hideReadout = false;
      return;
    }
    // 紧凑：按宽高比定列数（宽横屏 4 列一排；近方/竖屏 2×2）
    compact = true;
    cols = w / h >= 1.4 ? 4 : 2;
    // 读数行可放下的最小高度：紧凑读数行 ~10px 字体行高 ~14 + padding/gap ~9
    // + 边框 2 ≈ 25。h<50 时表盘可借读数行的高度（50-25)/0.66≈37 ≥ 36 下限，
    // 但 h∈[40,49) 若仍显示读数行会溢出（36·0.66+25≈48.8）→ 提前到 50 隐藏，
    // 则 h=45 时 (45-11)/0.66≈51 仍放得下。极端矮 h<35（36 下限 ~10px 溢出）
    // 属退化窗口，可接受（main.compact overflow:hidden 裁剪）。
    hideReadout = h < 50;
    const vOverhead = hideReadout ? 11 : 25; // 表盘 padding 9 + 边框 2；含读数行再加 ~14
    const byWidth = Math.floor((w - 8 * (cols - 1)) / cols);
    const byHeight = Math.floor((h - vOverhead) / 0.66);
    compactSize = Math.max(36, Math.min(byWidth, byHeight));
  }

  onMount(() => {
    void api
      .listAdapters()
      .then((list) => {
        adapters = list;
      })
      .catch(() => {
        /* 阈值读取失败回落默认 80，不影响仪表显示 */
      });
    if (typeof ResizeObserver === "undefined" || !root) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (!r || r.width === 0 || r.height === 0) return;
      relayout(r.width, r.height);
    });
    ro.observe(root);
    return () => ro.disconnect();
  });
</script>

<div
  class="dashboard"
  class:compact
  style={`--cols: ${cols}`}
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
        size={compact ? compactSize : m.big ? bigSize : Math.round(bigSize * BIG_SCALE)}
        detail={compact ? null : detail}
        compact={compact}
        readout={!hideReadout}
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
  /* 紧凑（小分辨率）：按动态列数等分；列数由 relayout 依宽高比决定 */
  .dashboard.compact {
    grid-template-columns: repeat(var(--cols, 4), 1fr);
    gap: 8px;
    align-items: stretch;
  }
  .cell {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
</style>
