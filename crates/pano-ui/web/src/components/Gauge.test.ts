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
});
