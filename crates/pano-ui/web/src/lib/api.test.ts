// 纯函数单测：值格式化 / 曲线数据转换（Vitest）。
import { describe, expect, it } from "vitest";
import { formatValue, toPlotData, type SampleEvent } from "./api";

describe("formatValue", () => {
  it("格式化整数不带小数", () => {
    expect(formatValue(42)).toBe("42");
    expect(formatValue(-3)).toBe("-3");
  });

  it("格式化小数保留 2 位并去尾零", () => {
    expect(formatValue(3.14159)).toBe("3.14");
    expect(formatValue(1.5)).toBe("1.5");
  });

  it("格式化布尔与文本", () => {
    expect(formatValue(true)).toBe("是");
    expect(formatValue(false)).toBe("否");
    expect(formatValue("运行中")).toBe("运行中");
  });

  it("JSON 值序列化", () => {
    expect(formatValue({ a: 1 })).toBe('{"a":1}');
  });
});

describe("toPlotData", () => {
  const samples: SampleEvent[] = [
    { series: "a.b", timestamp_ms: 1000, value: 1 },
    { series: "a.b", timestamp_ms: 2000, value: 2.5 },
    { series: "a.b", timestamp_ms: 3000, value: "非数值" },
  ];

  it("数值样本进入曲线，非数值记为 null", () => {
    const { x, y } = toPlotData(samples);
    expect(x).toEqual([1000, 2000, 3000]);
    expect(y).toEqual([1, 2.5, null]);
  });

  it("空数组返回空数据", () => {
    const { x, y } = toPlotData([]);
    expect(x).toEqual([]);
    expect(y).toEqual([]);
  });
});
