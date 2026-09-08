import "@testing-library/jest-dom/vitest";

// jsdom 无 matchMedia（uPlot / 部分布局库在初始化时查询系统缩放）。
// ComponentWindow 测试经 TimeSeriesPanel 间接引入 uPlot，需要它存在。
if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}
