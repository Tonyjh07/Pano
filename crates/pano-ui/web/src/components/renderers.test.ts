import { vi, describe, expect, it } from "vitest";

// TimeSeriesPanel 引入 uplot（import 时调用 matchMedia），jsdom 环境无此 API，
// 故 mock 之——本用例只验证分派逻辑，不渲染真实面板。
vi.mock("./TimeSeriesPanel.svelte", () => ({ default: { __mock: true } }));

import { renderers, resolveRenderer } from "./renderers";
import TimeSeriesPanel from "./TimeSeriesPanel.svelte";
import SysDashboard from "./SysDashboard.svelte";

describe("renderers", () => {
  it("unregistered component id falls back to generic TimeSeriesPanel", () => {
    expect(resolveRenderer("ghost-component")).toBe(TimeSeriesPanel);
  });

  it("catalog components without dedicated renderer fall back", () => {
    // M2.3：仅 sys-dashboard 注册专用渲染器，其余 sys-* 组件仍走通用渲染
    expect(resolveRenderer("sys-cpu")).toBe(TimeSeriesPanel);
    expect(resolveRenderer("sys-net")).toBe(TimeSeriesPanel);
  });

  it("sys-dashboard dispatches to the dedicated SysDashboard renderer", () => {
    expect(resolveRenderer("sys-dashboard")).toBe(SysDashboard);
    expect(renderers["sys-dashboard"]).toBe(SysDashboard);
  });

  it("registered renderer is dispatched by component id", () => {
    const original = { ...renderers };
    try {
      renderers["fake-component"] = TimeSeriesPanel;
      expect(resolveRenderer("fake-component")).toBe(TimeSeriesPanel);
    } finally {
      // 恢复目录（避免污染其他用例）
      for (const key of Object.keys(renderers)) delete renderers[key];
      for (const [k, v] of Object.entries(original)) renderers[k] = v;
    }
  });
});
