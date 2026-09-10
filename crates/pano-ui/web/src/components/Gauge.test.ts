import { describe, expect, it } from "vitest";
import { render } from "@testing-library/svelte";
import Gauge from "./Gauge.svelte";

describe("Gauge", () => {
  it("shows value and a green light when below threshold", () => {
    const { container } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "CPU 使用率" },
    });
    const light = container.querySelector(".light");
    expect(light).not.toBeNull();
    expect(light!.classList.contains("alarmed")).toBe(false);
    expect(container.querySelector(".value")!.textContent).toBe("50%");
    expect(container.textContent).toContain("CPU 使用率");
  });

  it("lights red when value exceeds threshold", () => {
    const { container } = render(Gauge, {
      props: { value: 92, threshold: 80, label: "内存使用率" },
    });
    const light = container.querySelector(".light");
    expect(light!.classList.contains("alarmed")).toBe(true);
    expect(container.querySelector(".value")!.textContent).toBe("92%");
  });

  it("clamps out-of-range value into 0..100", () => {
    const { container } = render(Gauge, {
      props: { value: 150, threshold: 80, label: "x" },
    });
    expect(container.querySelector(".value")!.textContent).toBe("100%");
    expect(container.querySelector(".light")!.classList.contains("alarmed")).toBe(true);
  });

  it("shows a dash when value is unavailable (no alarming)", () => {
    const { container } = render(Gauge, {
      props: { value: null, threshold: 80, label: "x" },
    });
    expect(container.querySelector(".value")!.textContent).toBe("—");
    expect(container.querySelector(".light")!.classList.contains("alarmed")).toBe(false);
  });

  it("shows the detail sub-readout when provided (M2.3.1)", () => {
    const { container } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "磁盘活动", detail: "E:" },
    });
    expect(container.querySelector(".detail")!.textContent).toBe("E:");
  });

  it("omits the detail line when absent", () => {
    const { container } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "CPU 使用率" },
    });
    expect(container.querySelector(".detail")).toBeNull();
  });

  it("compact mode applies reduced-overhead styles (M2.4 125% 缩放)", () => {
    const { container } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "CPU", size: 51, compact: true },
    });
    // 紧凑类挂在面板根节点，垂直开销缩减（padding/gap/读数行/指示灯）
    expect(container.querySelector(".gauge.compact")).not.toBeNull();
    // 常规模式不误开紧凑样式
    const { container: normal } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "CPU", size: 220, compact: false },
    });
    expect(normal.querySelector(".gauge.compact")).toBeNull();
  });

  it("hides the readout row when readout=false (极端矮窗口，M2.4 动态适配)", () => {
    const { container } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "CPU", size: 36, compact: true, readout: false },
    });
    expect(container.querySelector(".readout")).toBeNull();
    // 表盘与组件名仍在（组件名已印于表盘内，可辨识）
    expect(container.querySelector("svg")).not.toBeNull();
    expect(container.textContent).toContain("CPU");
    // 默认 readout=true → 读数行显示
    const { container: withReadout } = render(Gauge, {
      props: { value: 50, threshold: 80, label: "CPU", size: 100 },
    });
    expect(withReadout.querySelector(".readout")).not.toBeNull();
  });

  it("draws a 240° arc from -120° to +120° (12 o'clock zero, clockwise, M2.4)", () => {
    const { container } = render(Gauge, {
      props: { value: 0, threshold: 80, label: "x" },
    });
    const d = container.querySelector(".track")!.getAttribute("d")!;
    // 弧路径：M <start> A <R> <R> 0 <large-arc=1> <sweep=1> <end>
    expect(d).toMatch(/^M [\d.]+ [\d.]+ A [\d.]+ [\d.]+ 0 1 1 [\d.]+ [\d.]+$/);
    // 起点 = 左下（x < cx）、终点 = 右下（x > cx），对称于 12 点轴（y 相等）
    const m = d.match(/^M ([\d.]+) ([\d.]+) A [\d.]+ [\d.]+ 0 1 1 ([\d.]+) ([\d.]+)$/);
    expect(m).not.toBeNull();
    const x0 = parseFloat(m![1]);
    const y0 = parseFloat(m![2]);
    const x1 = parseFloat(m![3]);
    const y1 = parseFloat(m![4]);
    expect(x0).toBeLessThan(x1);
    expect(y0).toBeCloseTo(y1, 5);
  });

  it("draws the redzone arc from the threshold toward 100", () => {
    const { container } = render(Gauge, {
      props: { value: 10, threshold: 80, label: "x" },
    });
    const red = container.querySelector(".redzone");
    expect(red).not.toBeNull();
    expect(red!.getAttribute("d")).toContain("A");
  });

  it("uses a short arc (large-arc=0) for a narrow redzone (M2.4)", () => {
    // thr=80 → 红区仅 48°（<180°）→ large-arc 必须为 0，否则会画成反向大弧
    const { container } = render(Gauge, {
      props: { value: 10, threshold: 80, label: "x" },
    });
    const d = container.querySelector(".redzone")!.getAttribute("d")!;
    expect(d).toMatch(/A [\d.]+ [\d.]+ 0 0 1 [\d.]+ [\d.]+$/);
  });

  it("uses a large arc (large-arc=1) for a wide redzone (M2.4)", () => {
    // thr=0 → 红区覆盖整段 240°（>180°）→ large-arc=1
    const { container } = render(Gauge, {
      props: { value: 10, threshold: 0, label: "x" },
    });
    const d = container.querySelector(".redzone")!.getAttribute("d")!;
    expect(d).toMatch(/A [\d.]+ [\d.]+ 0 1 1 [\d.]+ [\d.]+$/);
  });

  it("keeps numeric labels inside the viewBox for all sizes (M2.4 B2 回归)", () => {
    // 标签在刻度内圈（R·0.75）：任何 size（含紧凑尺寸 / 下限 44）都不越出 viewBox
    for (const size of [240, 75, 44]) {
      const { container, unmount } = render(Gauge, {
        props: { value: 50, threshold: 80, label: "x", size },
      });
      const H = size * 0.66;
      const labels = container.querySelectorAll(".tick-label");
      expect(labels.length).toBe(5);
      labels.forEach((el) => {
        const x = parseFloat(el.getAttribute("x")!);
        const y = parseFloat(el.getAttribute("y")!);
        expect(x).toBeGreaterThanOrEqual(0);
        expect(x).toBeLessThanOrEqual(size);
        expect(y).toBeGreaterThanOrEqual(0);
        expect(y).toBeLessThanOrEqual(H);
      });
      unmount();
    }
  });
});
