<script lang="ts">
  // 可复用 SVG 汽车仪表盘（M2.4 升级）：0-100 刻度、240° 弧形（12 点位为 0°、
  // 顺时针为正、行程 -120° → +120°，即真实汽车仪表盘几何：0 在 8 点方向、
  // 50 在 12 点顶部、100 在 4 点方向）、红区（阈值→100）、指针 + 尾翼、指示灯。
  // point(v)：φ(v) = -120° + 240°·(v/100)；x = cx + R·sinφ，y = cy − R·cosφ。
  let { value, threshold, label, size = 240, detail = null, compact = false }: {
    value: number | null;
    threshold: number;
    label: string;
    size?: number;
    /** 副读数（M2.3.1：磁盘仪表显示最忙盘符，如 `C:`）。 */
    detail?: string | null;
    /** 紧凑模式（M2.4 修复）：小分辨率横条下缩减垂直开销
     *  （padding/读数行/间距），让 400×100 @125% 缩放的逻辑窗口
     *  高度（320×80 → main 内高约 58px）能放下表盘不被裁切。 */
    compact?: boolean;
  } = $props();

  const v = $derived(value === null ? 0 : Math.min(100, Math.max(0, value)));
  const clamped = $derived(Math.min(100, Math.max(0, threshold)));
  // 超阈值 → 指示灯亮红
  const alarmed = $derived(value !== null && value > clamped);

  // 几何：12 点位为 0°，顺时针为正；起始 -120°（8 点）、终点 +120°（4 点）。
  const R = $derived(size * 0.4);
  const cx = $derived(size / 2);
  const cy = $derived(size * 0.44);
  const H = $derived(size * 0.66);

  function rad(deg: number): number {
    return (deg * Math.PI) / 180;
  }

  /** 百分比 → 表盘上的屏幕坐标（y 向下；12 点为 0°，顺时针为正）。 */
  function pt(val: number, r: number = R): { x: number; y: number } {
    const phi = rad(-120 + 240 * (val / 100));
    return { x: cx + r * Math.sin(phi), y: cy - r * Math.cos(phi) };
  }

  /** 百分比区间 → SVG 弧路径（顺时针 sweep=1；弧角 >180° 时 large-arc=1）。
   *  240° 总行程下弧角 = |Δv|·2.4°，>180° ⇔ |Δv| > 75。 */
  function arc(fromV: number, toV: number, r: number): string {
    const a = pt(fromV, r);
    const b = pt(toV, r);
    const large = Math.abs(toV - fromV) > 75 ? 1 : 0;
    return `M ${a.x} ${a.y} A ${r} ${r} 0 ${large} 1 ${b.x} ${b.y}`;
  }

  // 满刻度弧（0→100）与外圈金属边框
  const track = $derived(arc(0, 100, R));
  const rim = $derived(arc(0, 100, R + 13));
  // 红区弧（阈值→100）
  const red = $derived(arc(clamped, 100, R));
  // 指针（前端点 + 反向尾翼）与中心 hub
  const needle = $derived(pt(v));
  const tail = $derived({
    x: cx - (needle.x - cx) * 0.22,
    y: cy - (needle.y - cy) * 0.22,
  });

  // 刻度：每 10 一条（每 20 加长）；数值标签 0/25/50/75/100 于刻度内圈
  const ticks = $derived(
    Array.from({ length: 11 }, (_, i) => {
      const val = i * 10;
      const long = i % 2 === 0;
      const outer = pt(val);
      const phi = rad(-120 + 240 * (val / 100));
      const innerR = R - (long ? 12 : 6);
      return {
        x1: cx + innerR * Math.sin(phi),
        y1: cy - innerR * Math.cos(phi),
        x2: outer.x,
        y2: outer.y,
        long,
      };
    }),
  );
  // 数值标签半径按比例取刻度内圈（R·0.75）：任何 size（含紧凑 75 / 下限 48）
  // 都落在 viewBox 内（弧外圈 R+Δ 在顶部/底部会越界，审查 B2 修复）。
  const labelR = $derived(R * 0.75);
  const labelTicks = $derived(
    [0, 25, 50, 75, 100].map((val) => {
      const p = pt(val, labelR);
      return {
        val: String(val),
        x: p.x,
        y: p.y + 3,
        anchor: val === 0 ? "end" : val === 100 ? "start" : "middle",
      };
    }),
  );
</script>

<div class="gauge" class:compact style={`width: ${size}px`}>
  <svg width={size} height={H} viewBox={`0 0 ${size} ${H}`}>
    <!-- 外圈金属边框 -->
    <path d={rim} class="rim" />
    <!-- 满刻度底弧 -->
    <path d={track} class="track" />
    <!-- 红区（阈值 → 100） -->
    <path d={red} class="redzone" />
    <!-- 刻度线 -->
    {#each ticks as t}
      <line
        x1={t.x1}
        y1={t.y1}
        x2={t.x2}
        y2={t.y2}
        class="tick"
        class:long={t.long}
      />
    {/each}
    <!-- 数值标签 0 / 25 / 50 / 75 / 100 -->
    {#each labelTicks as lt}
      <text
        x={lt.x}
        y={lt.y}
        class="tick-label"
        text-anchor={lt.anchor}
      >
        {lt.val}
      </text>
    {/each}
    <!-- 指针（含尾翼） -->
    <line x1={tail.x} y1={tail.y} x2={needle.x} y2={needle.y} class="needle" class:alarmed />
    <line x1={cx} y1={cy} x2={tail.x} y2={tail.y} class="needle-tail" class:alarmed />
    <circle cx={cx} cy={cy} r={4.5} class="hub" />
    <!-- 标签印在表盘内（hub 下方，弧底部开口处空白；紧凑模式省掉独立行的高度） -->
    <text x={cx} y={cy + size * 0.07} class="gauge-label" text-anchor="middle">{label}</text>
  </svg>
  <div class="readout">
    <span class="value" class:alarmed>{value === null ? "—" : `${v.toFixed(0)}%`}</span>
    <span class="light" class:alarmed title={alarmed ? "占用偏高" : "正常"}></span>
  </div>
  {#if detail}
    <span class="detail">{detail}</span>
  {/if}
</div>

<style>
  .gauge {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    /* 表盘面板质感：径向渐变 + 内阴影 */
    background:
      radial-gradient(120% 90% at 50% 0%, var(--panel-2, #2d2d30), var(--panel, #252526) 70%);
    border: 1px solid var(--border, #333);
    border-radius: 14px;
    padding: 8px 0 6px; /* 水平不内缩：SVG 宽度 = size 恰好铺满，紧凑模式按宽自适应 */
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.04),
      0 6px 18px rgba(0, 0, 0, 0.45);
  }
  /* 紧凑模式（小分辨率横条）：缩减垂直开销让 320×80 逻辑窗口（main 内高约 58）
    也放得下——padding 8+6→5+4、gap 2→1、读数行 13→10px、灯 12→9px，
    垂直开销从 ~32 降到 ~22，表盘尺寸可自适应更小而不被裁切（M2.4 修复）。 */
  .gauge.compact {
    gap: 1px;
    border-radius: 10px;
    padding: 5px 0 4px;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.04),
      0 4px 12px rgba(0, 0, 0, 0.4);
  }
  svg {
    display: block;
  }
  .rim {
    fill: none;
    stroke: var(--border, #555);
    stroke-width: 1.5;
    opacity: 0.6;
  }
  .track {
    fill: none;
    stroke: var(--panel-2, #3a3a3f);
    stroke-width: 9;
    stroke-linecap: round;
  }
  .redzone {
    fill: none;
    stroke: var(--err, #f85149);
    stroke-width: 9;
    stroke-linecap: round;
    opacity: 0.9;
    filter: drop-shadow(0 0 4px var(--err, #f85149));
  }
  .tick {
    stroke: var(--text-dim, #999);
    stroke-width: 1.5;
    opacity: 0.8;
  }
  .tick.long {
    stroke: var(--text, #d4d4d4);
    stroke-width: 2;
  }
  .tick-label {
    fill: var(--text-dim, #999);
    font-size: 11px;
    font-family: var(--mono, monospace);
  }
  .needle,
  .needle-tail {
    stroke: var(--accent, #4f9cf9);
    stroke-linecap: round;
    filter: drop-shadow(0 0 3px var(--accent, #4f9cf9));
  }
  .needle {
    stroke-width: 3.5;
  }
  .needle-tail {
    stroke-width: 2.5;
    opacity: 0.55;
  }
  .needle.alarmed,
  .needle-tail.alarmed {
    stroke: var(--err, #f85149);
    filter: drop-shadow(0 0 3px var(--err, #f85149));
  }
  .hub {
    fill: var(--text, #eee);
    filter: drop-shadow(0 0 3px var(--text, #eee));
  }
  .readout {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .gauge.compact .readout {
    gap: 4px;
    font-size: 10px;
  }
  .value {
    font-family: var(--mono, monospace);
    font-weight: 600;
    min-width: 42px;
    text-align: right;
  }
  .gauge.compact .value {
    min-width: 34px;
  }
  .detail {
    color: var(--accent, #4f9cf9);
    font-family: var(--mono, monospace);
    font-size: 12px;
    letter-spacing: 0.5px;
  }
  .value.alarmed {
    color: var(--err, #f85149);
  }
  .gauge-label {
    fill: var(--text-dim, #999);
    font-size: 12px;
    letter-spacing: 0.5px;
  }
  .light {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--ok, #3fb950); /* 正常：绿 */
    box-shadow: 0 0 6px var(--ok, #3fb950);
    flex: none;
  }
  .gauge.compact .light {
    width: 9px;
    height: 9px;
  }
  .light.alarmed {
    background: var(--err, #f85149); /* 超阈值：红 */
    box-shadow: 0 0 8px var(--err, #f85149);
    animation: blink 1s ease-in-out infinite;
  }
  @keyframes blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
</style>
