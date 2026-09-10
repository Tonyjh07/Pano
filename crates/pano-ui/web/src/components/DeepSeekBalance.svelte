<script lang="ts">
  // DeepSeek 额度组件（M2.5）：专用渲染器，数据源 = 远程适配器 deepseek.balance
  // （DeepSeek API 余额）。布局：状态条（可用 / 高峰时段 / 低余额）+ CNY 主卡
  // （大数字 + 充值/赠送细分 + 近 5min/30min 消耗）+ USD 副卡 + 趋势曲线。
  import { onMount } from "svelte";
  import type { AdapterInfo, SampleEvent } from "../lib/api";
  import { api, configNumber } from "../lib/api";
  import { isPeakHour, consumptionInWindow } from "../lib/deepseek";
  import TimeSeriesChart from "./TimeSeriesChart.svelte";

  // 数据管道（ComponentWindow 窗口壳提供，与通用渲染一致）
  let { seriesList, samples, latest }: {
    seriesList: string[];
    samples: Record<string, SampleEvent[]>;
    latest: Record<string, SampleEvent>;
  } = $props();

  const S_TOTAL_CNY = "deepseek.balance.total_cny";
  const S_GRANTED_CNY = "deepseek.balance.granted_cny";
  const S_TOPPED_CNY = "deepseek.balance.topped_up_cny";
  const S_TOTAL_USD = "deepseek.balance.total_usd";
  const S_AVAILABLE = "deepseek.balance.is_available";

  // 低余额阈值来自适配器自定义配置（list_adapters 读回；api_key 已被掩码）
  let adapters: AdapterInfo[] = $state([]);
  let adapterError: string | null = $state(null);
  onMount(() => {
    void api
      .listAdapters()
      .then((list) => (adapters = list))
      .catch((e: unknown) => (adapterError = String(e)));
  });

  const deepseek = $derived(adapters.find((a) => a.id === "deepseek.balance"));
  const lowThreshold = $derived(configNumber(deepseek, "low_threshold", 20));
  /** 适配器 Error 提示（list_adapters 快照；last_error 非空才显示）。 */
  const adapterStatusHint = $derived(
    deepseek?.last_error ? `适配器异常：${deepseek.last_error}` : null,
  );

  // “现在”每 30s 推进一次：驱动高峰时段判定与消耗窗口重算
  let nowMs = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => (nowMs = Date.now()), 30_000);
    return () => clearInterval(timer);
  });

  const totalCny = $derived(num(latest[S_TOTAL_CNY]?.value));
  const totalUsd = $derived(num(latest[S_TOTAL_USD]?.value));
  const hasUsd = $derived(seriesList.includes(S_TOTAL_USD));
  const available = $derived(
    typeof latest[S_AVAILABLE]?.value === "boolean"
      ? (latest[S_AVAILABLE]!.value as boolean)
      : null,
  );
  const peak = $derived(isPeakHour(new Date(nowMs)));
  const lowBalance = $derived(totalCny !== null && totalCny < lowThreshold);

  const cnySamples = $derived(samples[S_TOTAL_CNY] ?? []);
  const consumed5m = $derived(consumptionInWindow(cnySamples, 5 * 60_000, nowMs));
  const consumed30m = $derived(consumptionInWindow(cnySamples, 30 * 60_000, nowMs));

  function num(v: unknown): number | null {
    return typeof v === "number" && Number.isFinite(v) ? v : null;
  }
  function yuan(v: unknown): string {
    const n = num(v);
    return n === null ? "—" : `¥${n.toFixed(2)}`;
  }
  /** 消耗显示：负值（期间充值）记 0，另以「期间充值」标注。 */
  function consumed(v: number | null): string {
    return v === null ? "—" : `¥${Math.max(v, 0).toFixed(2)}`;
  }
</script>

<div class="balance" data-pano-drag>
  <div class="status-row">
    <span class="badge" class:ok={available === true} class:bad={available === false}>
      {available === null ? "账户状态未知" : available ? "账户可用" : "账户不可用"}
    </span>
    <span class="badge peak" class:hot={peak}>
      {peak ? "高峰 · 价格翻倍" : "空闲时段"}
    </span>
    <span class="badge" class:ok={totalCny !== null && !lowBalance} class:bad={lowBalance}>
      {lowBalance ? `余额不足（< ¥${lowThreshold}）` : "余额充足"}
    </span>
  </div>
  {#if adapterError}
    <p class="hint err">阈值读取失败：{adapterError}</p>
  {/if}
  {#if adapterStatusHint}
    <p class="hint err">{adapterStatusHint}</p>
  {/if}
  {#if cnySamples.length === 0}
    <p class="hint">暂无余额数据（适配器未启用 / 尚未获取到样本？见管理窗口）。</p>
  {/if}

  <div class="cards">
    <section class="card">
      <span class="label">CNY 总余额</span>
      <span class="big" class:low={lowBalance}>
        {totalCny === null ? "—" : `¥${totalCny.toFixed(2)}`}
      </span>
      <span class="sub">充值 {yuan(latest[S_TOPPED_CNY]?.value)} · 赠送 {yuan(latest[S_GRANTED_CNY]?.value)}</span>
      <span class="consumed">
        近 5 分钟消耗 {consumed(consumed5m)}{#if consumed5m !== null && consumed5m < 0}
          <span class="dim">（期间充值）</span>
        {/if}
        · 近 30 分钟 {consumed(consumed30m)}{#if consumed30m !== null && consumed30m < 0}
          <span class="dim">（期间充值）</span>
        {/if}
      </span>
    </section>

    {#if hasUsd}
      <section class="card usd">
        <span class="label">USD 总余额</span>
        <span class="big">{totalUsd === null ? "—" : `$${totalUsd.toFixed(2)}`}</span>
        <span class="sub">总余额（含赠送）</span>
      </section>
    {/if}
  </div>

  <div class="chart-wrap">
    <TimeSeriesChart seriesName={S_TOTAL_CNY} samples={cnySamples} />
  </div>
</div>

<style>
  .balance {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
  }
  .status-row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  /* 状态徽标复用全局 .badge（圆点 + 文案），本地补充配色 */
  .badge.ok {
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 12%, transparent);
  }
  .badge.bad {
    color: var(--err);
    background: color-mix(in srgb, var(--err) 12%, transparent);
  }
  .badge.peak {
    color: var(--text-dim);
    background: transparent;
    border-color: var(--border);
  }
  .badge.peak.hot {
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 12%, transparent);
  }
  .cards {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
    align-items: stretch;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .card.usd {
    min-width: 130px;
  }
  .label {
    color: var(--text-dim);
    font-size: 12px;
  }
  .big {
    font-family: var(--mono);
    font-size: 30px;
    font-weight: 600;
    line-height: 1.2;
  }
  .big.low {
    color: var(--err);
  }
  .sub {
    color: var(--text-dim);
    font-size: 12px;
  }
  .consumed {
    font-size: 12px;
  }
  .dim {
    color: var(--text-dim);
  }
  .hint {
    color: var(--text-dim);
    margin: 0;
  }
  .hint.err {
    color: var(--err);
  }
  .chart-wrap {
    flex: 1;
    min-height: 140px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px;
  }
</style>
