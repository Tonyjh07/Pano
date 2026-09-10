// DeepSeek 额度组件纯函数（M2.5）：高峰时段判定 + 余额消耗窗口计算。
// 与 Rust 侧无关，纯前端派生逻辑（数据源 = total_cny 样本缓冲，前端每 series
// 保留 500 点，60s 采样下覆盖约 8 小时——5min / 30min 窗口只需 30 点，够用）。
import type { SampleEvent } from "./api";

/** 高峰时段（周一~周五 9:00~12:00、14:00~18:00，本地时间；边界 12:00 / 18:00 归空闲）。 */
export const PEAK_WINDOWS: ReadonlyArray<{ startHour: number; endHour: number }> = [
  { startHour: 9, endHour: 12 },
  { startHour: 14, endHour: 18 },
];

/** 是否为高峰时段：周一至周五（getDay() 1..=5）且小时落在高峰窗口内（价格翻倍）。 */
export function isPeakHour(date: Date): boolean {
  const day = date.getDay();
  if (day === 0 || day === 6) return false;
  const hour = date.getHours();
  return PEAK_WINDOWS.some((w) => hour >= w.startHour && hour < w.endHour);
}

/**
 * 余额窗口差（¥）：窗口内最早样本值 − 最新样本值。
 *
 * - `samples` 须按时间升序（事件缓冲 / seriesHistory 均升序追加）；
 * - 窗口内不足 2 个数值样本 → `null`（显示「—」）；
 * - **正值 = 消耗，负值 = 期间充值**（组件据此记 0 并标注「期间充值」）。
 */
export function consumptionInWindow(
  samples: SampleEvent[],
  windowMs: number,
  nowMs: number,
): number | null {
  const cutoff = nowMs - windowMs;
  const inWindow = samples.filter(
    (s) => typeof s.value === "number" && s.timestamp_ms >= cutoff,
  );
  if (inWindow.length < 2) return null;
  const first = inWindow[0].value as number;
  const last = inWindow[inWindow.length - 1].value as number;
  return first - last;
}
