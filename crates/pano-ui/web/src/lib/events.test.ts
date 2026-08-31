// 事件订阅单测：过滤 + 节流合并（Vitest + jsdom 假定时器）。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { onSample, EVENT_SAMPLE } from "./events";
import type { SampleEvent } from "./api";

// 模拟 Tauri 事件 API：捕获监听器，测试中手动触发。
const listeners = new Map<string, (event: { payload: SampleEvent }) => void>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (eventName: string, handler: (event: { payload: SampleEvent }) => void) => {
    listeners.set(eventName, handler);
    return () => listeners.delete(eventName);
  }),
}));

function fire(series: string, timestamp_ms: number) {
  const handler = listeners.get(EVENT_SAMPLE);
  expect(handler).toBeDefined();
  handler!({ payload: { series, timestamp_ms, value: 1 } });
}

describe("onSample", () => {
  beforeEach(() => {
    listeners.clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("不过滤时收到全部样本", async () => {
    const received: string[] = [];
    await onSample((ev) => received.push(ev.series));
    fire("a.b", 1);
    fire("a.b", 2);
    expect(received).toEqual(["a.b", "a.b"]);
  });

  it("按 series 过滤：不关心的 series 被丢弃", async () => {
    const received: string[] = [];
    await onSample((ev) => received.push(ev.series), ["watch.me"]);
    fire("watch.me", 1);
    fire("ignore.me", 2);
    expect(received).toEqual(["watch.me"]);
  });

  it("节流合并：窗口内同 series 只回调一次（取最新）", async () => {
    const received: SampleEvent[] = [];
    await onSample((ev) => received.push(ev), ["a.b"], 100);
    fire("a.b", 1);
    fire("a.b", 2);
    fire("a.b", 3);
    expect(received).toEqual([]); // 窗口内未触发
    vi.advanceTimersByTime(100);
    expect(received).toHaveLength(1);
    expect(received[0].timestamp_ms).toBe(3); // 合并后取最新
  });

  it("取消订阅后不再收到事件", async () => {
    const received: string[] = [];
    const unlisten = await onSample((ev) => received.push(ev.series));
    unlisten();
    // 订阅已取消：即使事件到达也不回调
    listeners.get(EVENT_SAMPLE)?.({ payload: { series: "a.b", timestamp_ms: 1, value: 1 } });
    expect(received).toEqual([]);
    expect(listeners.has(EVENT_SAMPLE)).toBe(false);
  });
});
