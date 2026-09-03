import { vi, describe, expect, it } from "vitest";
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

describe("SysDashboard", () => {
  function latest(map: Record<string, number>) {
    const out: Record<string, { series: string; timestamp_ms: number; value: number }> = {};
    for (const [k, v] of Object.entries(map)) {
      out[k] = { series: k, timestamp_ms: 0, value: v };
    }
    return out;
  }

  it("renders the four dashboard gauges with labels", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: [
          "sys.cpu.usage",
          "sys.mem.used_percent",
          "sys.disk.active_percent",
          "sys.net.utilization",
        ],
        samples: {},
        latest: latest({
          "sys.cpu.usage": 10,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
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

  it("lights red for metrics above the default threshold (80)", () => {
    const { container } = render(SysDashboard, {
      props: {
        seriesList: [
          "sys.cpu.usage",
          "sys.mem.used_percent",
          "sys.disk.active_percent",
          "sys.net.utilization",
        ],
        samples: {},
        latest: latest({
          "sys.cpu.usage": 95,
          "sys.mem.used_percent": 40,
          "sys.disk.active_percent": 30,
          "sys.net.utilization": 5,
        }),
      },
    });
    // 仅 CPU 超阈值 → 恰好一个红灯
    expect(container.querySelectorAll(".light.alarmed").length).toBe(1);
  });
});
