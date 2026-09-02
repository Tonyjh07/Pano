import { vi, describe, expect, it } from "vitest";

// TimeSeriesPanel 引入 uplot（import 时调用 matchMedia），jsdom 环境无此 API，
// 故 mock 之——本用例只验证分派逻辑，不渲染真实面板。
vi.mock("./TimeSeriesPanel.svelte", () => ({ default: { __mock: true } }));

import { renderers, resolveRenderer } from "./renderers";
import TimeSeriesPanel from "./TimeSeriesPanel.svelte";

describe("renderers", () => {
  it("unregistered component id falls back to generic TimeSeriesPanel", () => {
    expect(resolveRenderer("ghost-component")).toBe(TimeSeriesPanel);
  });

  it("catalog components without dedicated renderer also fall back", () => {
    // M2.2：sys-* 组件均走通用渲染；M2.3 起注册专用渲染器后此处应命中
    expect(resolveRenderer("sys-cpu")).toBe(TimeSeriesPanel);
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
