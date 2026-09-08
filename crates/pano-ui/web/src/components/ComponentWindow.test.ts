import { vi, describe, expect, it, beforeEach } from "vitest";
import { tick } from "svelte";
import { render } from "@testing-library/svelte";

// ---- mock Tauri 窗口 / 事件 / 命令层（ComponentWindow 依赖链） ----
// getCurrentWindow 返回可编程的假窗口：记录事件回调以便测试驱动。
// Tauri 事件回调统一签名为 `({ payload })`，payload 内才是数据。
interface ResizedEv {
  payload: { height: number; width: number };
}
interface ScaleChangedEv {
  payload: { scaleFactor: number; size: { height: number; width: number } };
}
let winCb = {
  resized: [] as ((ev: ResizedEv) => void)[],
  scaleChanged: [] as ((ev: ScaleChangedEv) => void)[],
};
let fakeWin: {
  label: string;
  scaleFactor: ReturnType<typeof vi.fn>;
  innerSize: ReturnType<typeof vi.fn>;
  onResized: ReturnType<typeof vi.fn>;
  onScaleChanged: ReturnType<typeof vi.fn>;
  startDragging: ReturnType<typeof vi.fn>;
};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => fakeWin,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    api: {
      ...actual.api,
      windowContent: vi.fn().mockResolvedValue({
        component: "sys-cpu",
        series: [],
        decorations: false,
      }),
      seriesHistory: vi.fn().mockResolvedValue([]),
      monitors: vi.fn().mockResolvedValue([]),
    },
  };
});

import ComponentWindow from "./ComponentWindow.svelte";

/** 等待异步挂载链完成（watchHeight 的 scaleFactor/innerSize/监听 + load 链）。 */
async function flushMount() {
  for (let i = 0; i < 20; i++) await tick();
}

function stubWindow() {
  winCb = { resized: [], scaleChanged: [] };
  fakeWin = {
    label: "sys-cpu",
    scaleFactor: vi.fn(),
    innerSize: vi.fn(),
    onResized: vi.fn((cb: (ev: ResizedEv) => void) => {
      winCb.resized.push(cb);
      return Promise.resolve(() => {});
    }),
    onScaleChanged: vi.fn((cb: (ev: ScaleChangedEv) => void) => {
      winCb.scaleChanged.push(cb);
      return Promise.resolve(() => {});
    }),
    startDragging: vi.fn().mockResolvedValue(undefined),
  };
}

describe("ComponentWindow 紧凑判定（M2.4 高 DPI 单位换算）", () => {
  beforeEach(() => {
    stubWindow();
  });

  it("物理高度换算为逻辑后触发紧凑（factor 2 → 逻辑 100 < 140）", async () => {
    // 高 DPI（200%）：物理 200px = 逻辑 100px → 命中紧凑（header 折叠）
    fakeWin.scaleFactor.mockResolvedValue(2);
    fakeWin.innerSize.mockResolvedValue({ height: 200, width: 800 });
    const { container } = render(ComponentWindow);
    await flushMount();

    expect(container.querySelector(".win-header.compact")).not.toBeNull();
  });

  it("onResized 路径：窗口高度变化按当前 factor 换算更新紧凑判定", async () => {
    fakeWin.scaleFactor.mockResolvedValue(2);
    fakeWin.innerSize.mockResolvedValue({ height: 600, width: 800 }); // 逻辑 300 ≥ 140
    const { container } = render(ComponentWindow);
    await flushMount();
    expect(container.querySelector(".win-header.compact")).toBeNull();

    // 用户把无边框小屏窗口拉高到物理 200（factor 2 → 逻辑 100 < 140）
    winCb.resized[0]({ payload: { height: 200, width: 800 } });
    await flushMount();
    expect(container.querySelector(".win-header.compact")).not.toBeNull();
  });

  it("无边框窗口内容区 mousedown 委托 startDragging（用户实测回归：拖不动）", async () => {
    // windowContent mock 返回 decorations:false → 无边框 → data-pano-drag 区域可拖拽
    fakeWin.scaleFactor.mockResolvedValue(1);
    fakeWin.innerSize.mockResolvedValue({ height: 400, width: 800 });
    const { container } = render(ComponentWindow);
    await flushMount();

    const name = container.querySelector<HTMLElement>(".component-name[data-pano-drag]");
    expect(name).not.toBeNull();
    name!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    await flushMount();

    // 用户原始 bug 根因是 ACL 缺 start-dragging 权限；前端委托链路本身必须调用它
    expect(fakeWin.startDragging).toHaveBeenCalledTimes(1);
  });

  it("物理高度不换算时可能误判，换算后大窗口不触发紧凑（factor 2 → 逻辑 300 ≥ 140）", async () => {
    fakeWin.scaleFactor.mockResolvedValue(2);
    fakeWin.innerSize.mockResolvedValue({ height: 600, width: 800 });
    const { container } = render(ComponentWindow);
    await flushMount();

    // 物理 600 / factor 2 = 逻辑 300 ≥ 140 → 常规模式（header 不折叠）
    expect(container.querySelector(".win-header.compact")).toBeNull();
  });

  it("onScaleChanged 刷新 factor，跨 DPI 拖动后判定随新缩放更新", async () => {
    fakeWin.scaleFactor.mockResolvedValue(1);
    fakeWin.innerSize.mockResolvedValue({ height: 200, width: 800 }); // 逻辑 200 ≥ 140
    const { container } = render(ComponentWindow);
    await flushMount();
    expect(container.querySelector(".win-header.compact")).toBeNull();

    // 窗口拖到 200% DPI 显示器：scaleFactor 2 + 新物理高 200（逻辑 100 < 140）
    // → 前端必须按新 factor 重算，否则停留旧判定（误为常规）。
    // Tauri 事件回调签名统一为 `({ payload })`，payload 内才是数据。
    winCb.scaleChanged[0]({
      payload: { scaleFactor: 2, size: { height: 200, width: 800 } },
    });
    await flushMount();
    expect(container.querySelector(".win-header.compact")).not.toBeNull();
  });

  it("尺寸 API 不可用时回退常规模式，不抛错", async () => {
    fakeWin.scaleFactor.mockRejectedValue(new Error("not allowed"));
    fakeWin.innerSize.mockRejectedValue(new Error("not allowed"));
    const { container } = render(ComponentWindow);
    await flushMount();

    expect(container.querySelector(".win-header.compact")).toBeNull();
    expect(container.querySelector(".error-box")).toBeNull();
  });
});
