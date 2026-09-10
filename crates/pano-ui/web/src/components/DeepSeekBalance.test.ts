import { vi, describe, expect, it, beforeAll, afterAll } from "vitest";
import { tick } from "svelte";
import { render } from "@testing-library/svelte";

// mock 命令层：listAdapters 返回 deepseek.balance（low_threshold = 20，api_key 掩码）
vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    api: {
      ...actual.api,
      listAdapters: vi.fn().mockResolvedValue([
        {
          id: "deepseek.balance",
          name: "DeepSeek 额度",
          description: "",
          version: "0.2.0",
          capabilities: ["TimeSeries", "RemoteSource"],
          series: [],
          status: "运行中",
          running: true,
          enabled: true,
          sampling_ms: 60000,
          last_error: null,
          schema: [],
          config: [
            { key: "low_threshold", value: 20 },
            { key: "api_key", value: "********" },
          ],
        },
      ]),
    },
  };
});

// mock 高峰时段判定：测试固定为高峰（组件其它逻辑保持真实）
vi.mock("../lib/deepseek", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/deepseek")>();
  return {
    ...actual,
    isPeakHour: vi.fn(() => true),
  };
});

// mock uPlot：TimeSeriesChart 挂载时会 new uPlot + ResizeObserver，
// jsdom 无真实测量，mock 为无操作类（组件其它逻辑保持真实）。
vi.mock("uplot", () => ({
  default: class MockUPlot {
    setData() {}
    setSize() {}
    destroy() {}
  },
}));

import DeepSeekBalance from "./DeepSeekBalance.svelte";
import { api } from "../lib/api";

const S = {
  TOTAL_CNY: "deepseek.balance.total_cny",
  GRANTED_CNY: "deepseek.balance.granted_cny",
  TOPPED_CNY: "deepseek.balance.topped_up_cny",
  TOTAL_USD: "deepseek.balance.total_usd",
  AVAILABLE: "deepseek.balance.is_available",
};

const SERIES_ALL = Object.values(S);

// TimeSeriesChart 的 ResizeObserver（jsdom 无此 API；mock 为无操作，同 SysDashboard 测试）
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeAll(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});
afterAll(() => {
  vi.unstubAllGlobals();
});

function latest(map: Record<string, number | boolean>) {
  const out: Record<string, { series: string; timestamp_ms: number; value: number | boolean }> =
    {};
  for (const [k, v] of Object.entries(map)) {
    out[k] = { series: k, timestamp_ms: Date.now(), value: v };
  }
  return out;
}

/** 近 5/30 分钟消耗可计算：窗口内至少 2 个数值样本（60s 与 30s 前）。 */
function samplesWithHistory() {
  const now = Date.now();
  return {
    [S.TOTAL_CNY]: [
      { series: S.TOTAL_CNY, timestamp_ms: now - 60_000, value: 110 },
      { series: S.TOTAL_CNY, timestamp_ms: now - 1000, value: 108.5 },
    ],
  };
}

describe("DeepSeekBalance", () => {
  it("renders status badges, CNY card, breakdown and consumption chips", async () => {
    const { container, unmount } = render(DeepSeekBalance, {
      props: {
        seriesList: SERIES_ALL,
        samples: samplesWithHistory(),
        latest: latest({
          [S.TOTAL_CNY]: 110,
          [S.GRANTED_CNY]: 10,
          [S.TOPPED_CNY]: 100,
          [S.TOTAL_USD]: 15.25,
          [S.AVAILABLE]: true,
        }),
      },
    });
    await tick();

    expect(container.textContent).toContain("账户可用");
    expect(container.textContent).toContain("余额充足");
    // CNY 主卡：大数字 + 充值/赠送细分
    expect(container.querySelector(".big")!.textContent).toBe("¥110.00");
    expect(container.textContent).toContain("充值 ¥100.00");
    expect(container.textContent).toContain("赠送 ¥10.00");
    // 近 5/30 分钟消耗（60s 与 30s 前样本差分 = 1.5）
    expect(container.textContent).toContain("近 5 分钟消耗 ¥1.50");
    expect(container.textContent).toContain("近 30 分钟 ¥1.50");
    // USD 副卡
    expect(container.querySelector(".card.usd")!.textContent).toContain("$15.25");
    // 无红包（余额充足）
    expect(container.querySelectorAll(".badge.bad").length).toBe(0);

    unmount();
  });

  it("shows empty state when no balance samples yet", async () => {
    const { container, unmount } = render(DeepSeekBalance, {
      props: {
        seriesList: SERIES_ALL,
        samples: {},
        latest: {},
      },
    });
    await tick();
    expect(container.textContent).toContain("暂无余额数据");
    expect(container.querySelector(".big")!.textContent).toBe("—");
    unmount();
  });

  it("lights the low-balance badge and red big number below threshold", async () => {
    const { container, unmount } = render(DeepSeekBalance, {
      props: {
        seriesList: SERIES_ALL,
        samples: samplesWithHistory(),
        latest: latest({
          [S.TOTAL_CNY]: 10,
          [S.AVAILABLE]: true,
        }),
      },
    });
    await tick();

    expect(container.querySelector(".big")!.classList.contains("low")).toBe(true);
    expect(container.querySelector(".big")!.textContent).toBe("¥10.00");
    expect(container.textContent).toContain("余额不足（< ¥20）");
    // 低余额 → 一个红灯徽标（「余额不足」），可用徽标仍绿
    expect(container.querySelectorAll(".badge.bad").length).toBe(1);

    unmount();
  });

  it("shows the peak-hour badge (price doubled) when isPeakHour is true", async () => {
    const { container, unmount } = render(DeepSeekBalance, {
      props: {
        seriesList: SERIES_ALL,
        samples: {},
        latest: latest({ [S.AVAILABLE]: true }),
      },
    });
    await tick();
    expect(container.textContent).toContain("高峰 · 价格翻倍");
    unmount();
  });

  it("shows the adapter last_error hint when the adapter is in error state", async () => {
    vi.mocked(api.listAdapters).mockResolvedValueOnce([
      {
        id: "deepseek.balance",
        name: "DeepSeek 额度",
        description: "",
        version: "0.2.0",
        capabilities: [],
        series: [],
        status: "错误",
        running: false,
        enabled: true,
        sampling_ms: 60000,
        last_error: "HTTP 状态错误：401",
        schema: [],
        config: [{ key: "low_threshold", value: 20 }],
      },
    ]);
    const { container, unmount } = render(DeepSeekBalance, {
      props: { seriesList: SERIES_ALL, samples: {}, latest: {} },
    });
    await tick();
    expect(container.textContent).toContain("适配器异常：HTTP 状态错误：401");
    unmount();
  });

  it("hides the USD card when the component does not subscribe to total_usd", async () => {
    const { container, unmount } = render(DeepSeekBalance, {
      props: {
        seriesList: [S.TOTAL_CNY, S.AVAILABLE],
        samples: {},
        latest: latest({ [S.TOTAL_CNY]: 110 }),
      },
    });
    await tick();
    expect(container.querySelector(".card.usd")).toBeNull();
    unmount();
  });

  it("marks the panel as a drag region for frameless windows (M2.4 惯例)", async () => {
    const { container, unmount } = render(DeepSeekBalance, {
      props: { seriesList: SERIES_ALL, samples: {}, latest: {} },
    });
    await tick();
    expect(container.querySelector(".balance")!.hasAttribute("data-pano-drag")).toBe(true);
    unmount();
  });
});
