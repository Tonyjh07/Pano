import { vi, describe, expect, it } from "vitest";
import { tick } from "svelte";
import { render } from "@testing-library/svelte";

// mock 命令层：listAdapters 返回空表 → 阈值回落默认 80（不触发真实 Tauri invoke）
vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    api: {
      ...actual.api,
      listAdapters: vi.fn().mockResolvedValue([]),
    },
  };
});

import SysDashboard from "./SysDashboard.svelte";

const SERIES = [
  "sys.cpu.usage",
  "sys.mem.used_percent",
  "sys.disk.active_percent",
  "sys.disk.busiest_disk",
  "sys.net.utilization",
];

describe("SysDashboard", () => {
  function latest(map: Record<string, number | string>) {
    const out: Record<string, { series: string; timestamp_ms: number; value: number | string }> = {};
    for (const [k, v] of Object.entries(map)) {
      out[k] = { series: k, timestamp_ms: 0, value: v };
    }
    return out;
  }

  it("renders the four dashboard gauges with labels", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.disk.busiest_disk": "C:",
          "sys.net.utilization": 5,
        }),
      },
    });
    expect(container.querySelectorAll(".gauge").length).toBe(4);
    expect(container.textContent).toContain("CPU 使用率");
    expect(container.textContent).toContain("内存使用率");
    expect(container.textContent).toContain("磁盘活动");
    expect(container.textContent).toContain("网络利用率");
    // 全部低于默认阈值 80 → 无红灯
    expect(container.querySelectorAll(".light.alarmed").length).toBe(0);
  });

  it("shows the busiest disk letter on the disk gauge (M2.3.1)", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 87,
          "sys.disk.busiest_disk": "E:",
          "sys.net.utilization": 5,
        }),
      },
    });
    expect(container.querySelector(".detail")!.textContent).toBe("E:");
    // 磁盘活动超默认阈值 80 → 指示灯亮红
    expect(container.querySelectorAll(".light.alarmed").length).toBe(1);
  });

  it("omits the disk detail when busiest_disk is unavailable (e.g. non-Windows)", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.net.utilization": 5,
        }),
      },
    });
    expect(container.querySelector(".detail")).toBeNull();
  });

  it("lights red for metrics above the default threshold (80)", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 95,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.disk.busiest_disk": "C:",
          "sys.net.utilization": 5,
        }),
      },
    });
    // 仅 CPU 超阈值 → 恰好一个红灯
    expect(container.querySelectorAll(".light.alarmed").length).toBe(1);
  });

  it("marks the dashboard as a drag region for frameless windows (M2.4)", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.disk.busiest_disk": "C:",
          "sys.net.utilization": 5,
        }),
      },
    });
    // 无边框窗口（M2.4）下内容区整体可拖拽移动（窗口壳据此 startDragging）
    expect(container.querySelector(".dashboard")!.hasAttribute("data-pano-drag")).toBe(true);
  });

  it("switches to the compact strip layout on short containers (M2.4)", async () => {
    // mock ResizeObserver：触发一次回调，模拟 400×80 容器（小屏 400×100 场景）
    let cb: ((entries: { contentRect: { width: number; height: number } }[]) => void) | null =
      null;
    class MockResizeObserver {
      constructor(c: typeof cb) {
        cb = c;
      }
      observe(_el: unknown) {}
      unobserve(_el: unknown) {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
    const { container, unmount } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 87,
          "sys.disk.busiest_disk": "E:",
          "sys.net.utilization": 5,
        }),
      },
    });
    cb!([{ contentRect: { width: 400, height: 80 } }]);
    await tick();

    // 紧凑横条布局：仍 4 个表盘，尺寸按宽/高双预算取小
    // 400 宽 → (400-24)/4=94；80 高 → (80-24)/0.66=84（高度预算主导，防溢出裁切）
    expect(container.querySelector(".dashboard.compact")).not.toBeNull();
    expect(container.querySelectorAll(".gauge").length).toBe(4);
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("84");
    // 短标签 + 磁盘最忙盘符拼入标签（紧凑模式无独立 detail 行）
    expect(container.textContent).toContain("磁盘·E:");
    expect(container.querySelector(".detail")).toBeNull();
    // 磁盘活动 87 > 默认阈值 80 → 红灯仍亮
    expect(container.querySelectorAll(".light.alarmed").length).toBe(1);

    unmount();
    vi.unstubAllGlobals();
  });

  it("keeps compact gauges within a 400×100 window inner height (M2.4)", async () => {
    // 400×100 小屏（用户实测场景，100% 缩放）：main 内高约 78（窗口 100 − 折叠头部 ~18 − padding 4）。
    // 高度预算 (78-24)/0.66 ≈ 81，宽预算 (400-24)/4=94 → size=81，紧凑表盘不溢出裁切。
    let cb: ((entries: { contentRect: { width: number; height: number } }[]) => void) | null =
      null;
    class MockResizeObserver {
      constructor(c: typeof cb) {
        cb = c;
      }
      observe(_el: unknown) {}
      unobserve(_el: unknown) {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
    const { container, unmount } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.net.utilization": 5,
        }),
      },
    });
    cb!([{ contentRect: { width: 400, height: 78 } }]);
    await tick();

    expect(container.querySelector(".dashboard.compact")).not.toBeNull();
    // 表盘尺寸受高度预算约束：81 < 宽预算 94，防小屏垂直溢出
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("81");

    unmount();
    vi.unstubAllGlobals();
  });

  it("fits compact gauges on a 400×100 @125% 缩放屏幕（逻辑 320×80）(M2.4)", async () => {
    // 用户实测：400×100 物理分辨率 + 125% 缩放 → 逻辑 320×80，main 内高约 58
    // （窗口 80 − 折叠头部 ~18 − padding 4）。紧凑预算 (58-24)/0.66 ≈ 51，
    // 宽预算 (320-24)/4=74 → size=51，表盘总高 51·0.66+24 ≈ 57.7 ≤ 58 不裁切。
    let cb: ((entries: { contentRect: { width: number; height: number } }[]) => void) | null =
      null;
    class MockResizeObserver {
      constructor(c: typeof cb) {
        cb = c;
      }
      observe(_el: unknown) {}
      unobserve(_el: unknown) {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
    const { container, unmount } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.net.utilization": 5,
        }),
      },
    });
    cb!([{ contentRect: { width: 320, height: 58 } }]);
    await tick();

    expect(container.querySelector(".dashboard.compact")).not.toBeNull();
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("51");
    // 紧凑 Gauge 开启垂直开销缩减样式（放得下才可能不被裁切）
    expect(container.querySelector(".gauge.compact")).not.toBeNull();

    unmount();
    vi.unstubAllGlobals();
  });

  it("stays in the regular 2×2 layout for tall containers (M2.4)", async () => {
    let cb: ((entries: { contentRect: { width: number; height: number } }[]) => void) | null =
      null;
    class MockResizeObserver {
      constructor(c: typeof cb) {
        cb = c;
      }
      observe(_el: unknown) {}
      unobserve(_el: unknown) {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
    const { container, unmount } = render(SysDashboard, {
      props: {
        seriesList: SERIES,
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.net.utilization": 5,
        }),
      },
    });
    cb!([{ contentRect: { width: 860, height: 480 } }]);
    await tick();

    expect(container.querySelector(".dashboard.compact")).toBeNull();
    expect(container.querySelectorAll(".gauge").length).toBe(4);

    unmount();
    vi.unstubAllGlobals();
  });
});
