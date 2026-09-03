<script lang="ts">
  // 可复用 SVG 半圆仪表（M2.3）：0-100 刻度、红区（阈值→100）、指针、指示灯。
  // 几何：0% 在左、100% 在右，半圆过顶点（时钟 9 点 → 12 点 → 3 点方向）。
  // point(v) = (cx + R·cosθ, cy − R·sinθ)，θ = π·(1 − v/100)。
  let { value, threshold, label, size = 240, detail = null }: {
    value: number | null;
    threshold: number;
    label: string;
    size?: number;
    /** 副读数（M2.3.1：磁盘仪表显示最忙盘符，如 `C:`）。 */
    detail?: string | null;
  } = $props();

  const v = $derived(value === null ? 0 : Math.min(100, Math.max(0, value)));
  const clamped = $derived(Math.min(100, Math.max(0, threshold)));
  // 超阈值 → 指示灯亮红
  const alarmed = $derived(value !== null && value > clamped);

  const R = $derived(size * 0.4);
  const cx = $derived(size / 2);
  const cy = $derived(size / 2 - 4);

  function pt(val: number): { x: number; y: number } {
    const theta = Math.PI * (1 - val / 100);
    return { x: cx + R * Math.cos(theta), y: cy - R * Math.sin(theta) };
  }

  // 满刻度弧（0→100，过顶点，sweep=1）
  const trackStart = $derived(pt(0));
  const trackEnd = $derived(pt(100));
  // 红区弧（阈值→100）
  const redStart = $derived(pt(clamped));
  const needle = $derived(pt(v));

  // 刻度（每 10 一格；0 / 50 / 100 数值标签硬编码在下方 <text>）
  const ticks = $derived(
    Array.from({ length: 11 }, (_, i) => {
      const val = i * 10;
      const outer = pt(val);
      // 径向短线：内外半径差
      const inner = {
        x: cx + (R - 6) * Math.cos(Math.PI * (1 - val / 100)),
        y: cy - (R - 6) * Math.sin(Math.PI * (1 - val / 100)),
      };
      return { outer, inner };
    }),
  );
</script>

<div class="gauge" style={`width: ${size}px`}>
  <svg width={size} height={size / 2 + 6} viewBox={`0 0 ${size} ${size / 2 + 6}`}>
    <!-- 满刻度底弧 -->
    <path
      d={`M ${trackStart.x} ${trackStart.y} A ${R} ${R} 0 0 1 ${trackEnd.x} ${trackEnd.y}`}
      class="track"
    />
    <!-- 红区（阈值 → 100） -->
    <path
      d={`M ${redStart.x} ${redStart.y} A ${R} ${R} 0 0 1 ${trackEnd.x} ${trackEnd.y}`}
      class="redzone"
    />
    <!-- 刻度线 -->
    {#each ticks as t}
      <line
        x1={t.inner.x}
        y1={t.inner.y}
        x2={t.outer.x}
        y2={t.outer.y}
        class="tick"
      />
    {/each}
    <!-- 0 / 50 / 100 刻度数值 -->
    <text x={trackStart.x - 4} y={cy + 4} class="tick-label" text-anchor="end">0</text>
    <text x={cx} y={10} class="tick-label" text-anchor="middle">50</text>
    <text x={trackEnd.x + 4} y={cy + 4} class="tick-label" text-anchor="start">100</text>
    <!-- 指针 -->
    <line
      x1={cx}
      y1={cy}
      x2={needle.x}
      y2={needle.y}
      class="needle"
      stroke={alarmed ? "var(--err, #f85149)" : "var(--accent, #4f9cf9)"}
    />
    <circle cx={cx} cy={cy} r={4} class="hub" />
  </svg>
  <div class="readout">
    <span class="value" class:alarmed>{value === null ? "—" : `${v.toFixed(0)}%`}</span>
    <span class="light" class:alarmed title={alarmed ? "占用偏高" : "正常"}></span>
    <span class="label">{label}</span>
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
  }
  svg {
    display: block;
  }
  .track {
    fill: none;
    stroke: var(--panel-2, #333);
    stroke-width: 8;
    stroke-linecap: round;
  }
  .redzone {
    fill: none;
    stroke: var(--err, #f85149);
    stroke-width: 8;
    stroke-linecap: round;
    opacity: 0.85;
  }
  .tick {
    stroke: var(--border, #555);
    stroke-width: 1.5;
  }
  .tick-label {
    fill: var(--text-dim, #999);
    font-size: 11px;
    font-family: var(--mono, monospace);
  }
  .needle {
    stroke-width: 3;
    stroke-linecap: round;
  }
  .hub {
    fill: var(--text, #eee);
  }
  .readout {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .value {
    font-family: var(--mono, monospace);
    font-weight: 600;
    min-width: 42px;
    text-align: right;
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
  .light {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--ok, #3fb950); /* 正常：绿 */
    box-shadow: 0 0 6px var(--ok, #3fb950);
    flex: none;
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
  .label {
    color: var(--text-dim, #999);
  }
</style>
