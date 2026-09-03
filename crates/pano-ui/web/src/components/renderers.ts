// 渲染器分派（ui.md §4 / 架构 §7，M2.2）：按 UI 组件类型 id 解析对应渲染器。
//
// M2.2 全部组件使用通用渲染（TimeSeriesPanel：组件固定 series 逐个「数值卡 +
// 曲线」）；M2.3 起注册专用渲染组件（"sys-dashboard": SysDashboard），
// 未注册的类型回退到通用渲染。
import type { Component } from "svelte";
import TimeSeriesPanel from "./TimeSeriesPanel.svelte";
import SysDashboard from "./SysDashboard.svelte";

// 渲染器 props 各异（通用面板传 seriesList/samples/latest，专用渲染器自定义），
// 故以宽松组件类型登记，动态分派时按各渲染器实际 props 传参。
export const renderers: Record<string, Component<any>> = {
  "sys-dashboard": SysDashboard,
};

/** 按组件类型 id 解析渲染器；未注册（未知）类型回退到通用渲染。 */
export function resolveRenderer(componentId: string): Component<any> {
  return renderers[componentId] ?? TimeSeriesPanel;
}
