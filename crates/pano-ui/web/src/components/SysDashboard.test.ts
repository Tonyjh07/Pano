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

    // 紧凑横条布局：仍 4 个表盘（400 宽 / 80 高 → 宽高比 5 ≥ 1.4 → 4 列），
    // 尺寸按宽/高双预算取小：宽 (400-24)/4=94；高 (80-25)/0.66=83（高度主导）
    expect(container.querySelector(".dashboard.compact")).not.toBeNull();
    expect(container.querySelectorAll(".gauge").length).toBe(4);
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("83");
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
    // 动态紧凑预算：宽 (400-24)/4=94；高 (78-25)/0.66=80 → size=80，紧凑表盘不溢出裁切。
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
    // 表盘尺寸受高度预算约束：80 < 宽预算 94，防小屏垂直溢出
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("80");

    unmount();
    vi.unstubAllGlobals();
  });

  it("fits compact gauges on a 400×100 @125% 缩放屏幕（逻辑 320×80）(M2.4)", async () => {
    // 用户实测：400×100 物理分辨率 + 125% 缩放 → 逻辑 320×80，main 内高约 58
    // （窗口 80 − 折叠头部 ~18 − padding 4）。动态紧凑预算：宽 (320-24)/4=74；
    // 高 (58-25)/0.66=50 → size=50，表盘总高 50·0.66+25 ≈ 58 ≤ 58 不裁切。
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
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("50");
    // 紧凑 Gauge 开启垂直开销缩减样式（放得下才可能不被裁切）
    expect(container.querySelector(".gauge.compact")).not.toBeNull();

    unmount();
    vi.unstubAllGlobals();
  });

  it("uses 2×2 compact layout on narrow-and-short containers (宽高比 < 1.4，M2.4 动态适配)", async () => {
    // 100×100：h<120 → 紧凑；宽高比 1.0 < 1.4 → 2 列
    // 表盘 = min((100-8)/2=46, (100-25)/0.66=113) = 46
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
    cb!([{ contentRect: { width: 100, height: 100 } }]);
    await tick();

    expect(container.querySelector(".dashboard.compact")).not.toBeNull();
    // 2 列（--cols=2）：CSS 变量反应动态列数
    expect(container.querySelector(".dashboard.compact")!.getAttribute("style")).toContain(
      "--cols: 2",
    );
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("46");

    unmount();
    vi.unstubAllGlobals();
  });

  it("hides the readout row on ultra-short containers (极端矮，M2.4 动态适配)", async () => {
    // 400×35：紧凑，宽高比 11.4 ≥ 1.4 → 4 列；h=35 < 50 → 隐藏读数行
    // byWidth=(400-24)/4=94；byHeight=(35-11)/0.66=36 → size=36
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
    cb!([{ contentRect: { width: 400, height: 35 } }]);
    await tick();

    expect(container.querySelector(".dashboard.compact")).not.toBeNull();
    // 读数行被隐藏（只留表盘，组件名已印于表盘内）
    expect(container.querySelector(".readout")).toBeNull();
    // 表盘尺寸 = min(94, (35-11)/0.66=36) = 36
    expect(container.querySelector(".gauge svg")!.getAttribute("width")).toBe("36");

    unmount();
    vi.unstubAllGlobals();
  });

  it("scales the regular 2×2 gauges with the container (非硬编码 300/220，M2.4 动态适配)", async () => {
    // 高度 ≥120 常规 2×2：大表盘受宽/高双约束缩放。
    // 400×200：byWidth=(400-16)/2=192；byHeight=(200-34-50-8)/(0.66·1.73)=108/1.14=94
    // （rowBig 34 + rowSmall 50 含磁盘 detail 行，见 SysDashboard relayout 注释）→ big=94
    // 生产场景磁盘恒有 busiest_disk，latest 显式传入以覆盖 detail 行（M2.4 阻断回归）。
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
          "sys.disk.busiest_disk": "C:",
          "sys.net.utilization": 5,
        }),
      },
    });
    cb!([{ contentRect: { width: 400, height: 200 } }]);
    await tick();

    expect(container.querySelector(".dashboard.compact")).toBeNull();
    // 大表盘 = 94（非固定 300），小表盘 = round(94·0.73) = 69
    const svgs = container.querySelectorAll(".gauge svg");
    expect(svgs[0]!.getAttribute("width")).toBe("94");
    expect(svgs[2]!.getAttribute("width")).toBe("69");
    // 磁盘 detail 行存在（生产恒有），预算已含其开销 → 总高不溢出
    expect(container.querySelector(".detail")!.textContent).toBe("C:");

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
