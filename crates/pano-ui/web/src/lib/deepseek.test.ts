import { describe, expect, it } from "vitest";
import { isPeakHour, consumptionInWindow, PEAK_WINDOWS } from "./deepseek";
import type { SampleEvent } from "./api";

// 2025-06 月历：6/2 = 周一、6/7 = 周六、6/8 = 周日
function at(day: number, hour: number, minute = 0): Date {
  return new Date(2025, 5, day, hour, minute);
}

function s(ts: number, v: number): SampleEvent {
  return { series: "deepseek.balance.total_cny", timestamp_ms: ts, value: v };
}

describe("isPeakHour", () => {
  it("is peak on weekdays within 9-12 and 14-18", () => {
    expect(isPeakHour(at(2, 9, 0))).toBe(true); // 周一 09:00
    expect(isPeakHour(at(2, 11, 59))).toBe(true);
    expect(isPeakHour(at(2, 14, 0))).toBe(true); // 14:00 起高峰
    expect(isPeakHour(at(2, 17, 59))).toBe(true);
  });

  it("is off-peak at window boundaries and outside windows", () => {
    expect(isPeakHour(at(2, 12, 0))).toBe(false); // 12:00 归空闲
    expect(isPeakHour(at(2, 13, 59))).toBe(false);
    expect(isPeakHour(at(2, 18, 0))).toBe(false); // 18:00 归空闲
    expect(isPeakHour(at(2, 8, 59))).toBe(false);
    expect(isPeakHour(at(2, 20, 0))).toBe(false);
  });

  it("is off-peak on weekends", () => {
    expect(isPeakHour(at(7, 10, 0))).toBe(false); // 周六
    expect(isPeakHour(at(8, 15, 0))).toBe(false); // 周日
  });

  it("declares two peak windows 9-12 and 14-18", () => {
    expect(PEAK_WINDOWS).toEqual([
      { startHour: 9, endHour: 12 },
      { startHour: 14, endHour: 18 },
    ]);
  });
});

describe("consumptionInWindow", () => {
  const now = 1_000_000_000;

  it("computes delta over the full window (consumption positive)", () => {
    const samples = [
      s(now - 30 * 60_000, 110),
      s(now - 25 * 60_000, 109),
      s(now - 1000, 108.5),
    ];
    expect(consumptionInWindow(samples, 30 * 60_000, now)).toBeCloseTo(1.5);
  });

  it("uses only samples inside the window (5min)", () => {
    const samples = [
      s(now - 30 * 60_000, 110), // 窗口外，忽略
      s(now - 4 * 60_000, 109),
      s(now - 1000, 108.5),
    ];
    expect(consumptionInWindow(samples, 5 * 60_000, now)).toBeCloseTo(0.5);
  });

  it("returns null when fewer than 2 samples in the window", () => {
    expect(consumptionInWindow([s(now - 1000, 110)], 5 * 60_000, now)).toBeNull();
    expect(consumptionInWindow([], 5 * 60_000, now)).toBeNull();
    // 仅 1 个样本在窗口内 → 不足
    expect(
      consumptionInWindow([s(now - 60_000, 110), s(now - 1000, 109)], 30 * 1000, now),
    ).toBeNull();
  });

  it("records a negative delta on top-up (balance rose within the window)", () => {
    const samples = [s(now - 60_000, 110), s(now - 1000, 150)];
    expect(consumptionInWindow(samples, 5 * 60_000, now)).toBe(-40);
  });

  it("ignores non-numeric samples", () => {
    const samples = [
      { series: "x", timestamp_ms: now - 60_000, value: 110 },
      { series: "x", timestamp_ms: now - 30_000, value: "text" as unknown as number },
      { series: "x", timestamp_ms: now - 1000, value: 109 },
    ];
    expect(consumptionInWindow(samples, 5 * 60_000, now)).toBeCloseTo(1);
  });
});
