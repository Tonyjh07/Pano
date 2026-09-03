// 命令层 DTO 的 TS 类型镜像（与 crates/pano-ui/src/dto.rs 对应）+ invoke 封装。
import { invoke } from "@tauri-apps/api/core";

export interface FieldInfo {
  key: string;
  label: string;
  kind: "number" | "bool" | "text" | "choice";
  default: number | boolean | string;
  choices: string[];
  help: string | null;
}

export interface ConfigValueDto {
  key: string;
  value: number | boolean | string | Record<string, unknown> | unknown[] | null;
}

export interface AdapterInfo {
  id: string;
  name: string;
  description: string;
  version: string;
  capabilities: string[];
  series: string[];
  status: string;
  running: boolean;
  enabled: boolean;
  sampling_ms: number;
  last_error: string | null;
  schema: FieldInfo[];
  /** 当前自定义配置值（M2.3：high_threshold / link_mbps 等；未配置则无该项）。 */
  config: ConfigValueDto[];
}

/** 从适配器 config 中读取数值型自定义配置（缺省回落 `fallback`）。 */
export function configNumber(adapter: AdapterInfo | undefined, key: string, fallback: number): number {
  const v = adapter?.config.find((c) => c.key === key)?.value;
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

export interface SampleEvent {
  series: string;
  timestamp_ms: number;
  /** Rust 侧 SampleValue 的 JSON 表达：数值 / 布尔 / 文本 / 任意 JSON（含数组 / null）。 */
  value: number | boolean | string | Record<string, unknown> | unknown[] | null;
}

export interface StatusInfo {
  status: string;
  running: boolean;
  last_error: string | null;
}

export interface MonitorInfo {
  id: string;
  name: string;
  is_primary: boolean;
  position: [number, number];
  size: [number, number];
  scale_factor: number;
}

export interface WindowInfo {
  id: string;
  title: string;
  /** 绑定的 UI 组件类型 id（管理窗口为空串）。 */
  component: string;
  /** 该窗口展示的 series（由组件目录解析，管理窗口为空）。 */
  series: string[];
  is_manager: boolean;
  visible: boolean;
}

export interface ComponentInfo {
  id: string;
  name: string;
  /** 本组件固定消费的 series。 */
  series: string[];
  /** 是否可用（全部 series 所属适配器已注册）；未注册组件置灰不可选。 */
  available: boolean;
}

export interface WindowContent {
  component: string;
  series: string[];
}

/** 命令封装：参数名用 camelCase，Tauri 自动映射为 Rust 的 snake_case。 */
export const api = {
  listAdapters: () => invoke<AdapterInfo[]>("list_adapters"),

  adapterStatus: (id: string) => invoke<StatusInfo>("adapter_status", { id }),

  setAdapterEnabled: (id: string, enabled: boolean) =>
    invoke<void>("set_adapter_enabled", { id, enabled }),

  setAdapterSampling: (id: string, samplingMs: number) =>
    invoke<void>("set_adapter_sampling", { id, samplingMs }),

  restartAdapter: (id: string) => invoke<void>("restart_adapter", { id }),

  seriesHistory: (series: string, window?: number) =>
    invoke<SampleEvent[]>("series_history", { series, window }),

  seriesLatest: (series: string) => invoke<SampleEvent | null>("series_latest", { series }),

  listComponents: () => invoke<ComponentInfo[]>("list_components"),

  windowContent: (id: string) => invoke<WindowContent>("window_content", { id }),

  windowSetFullscreen: (label: string, enabled: boolean) =>
    invoke<void>("window_set_fullscreen", { label, enabled }),

  windowSetAlwaysOnTop: (label: string, enabled: boolean) =>
    invoke<void>("window_set_always_on_top", { label, enabled }),

  windowSetMonitor: (label: string, monitorId: string) =>
    invoke<void>("window_set_monitor", { label, monitorId }),

  windowHide: (label: string) => invoke<void>("window_hide", { label }),

  windowShow: (label: string) => invoke<void>("window_show", { label }),

  listWindows: () => invoke<WindowInfo[]>("list_windows"),

  /** 新建监控窗口并绑定一个 UI 组件（标题缺省取组件默认标题）。 */
  createWindow: (id: string, component: string, title?: string) =>
    invoke<void>("create_window", { id, component, title }),

  /** 切换窗口绑定的 UI 组件（仅换内容 / series，标题与几何不动）。 */
  setWindowComponent: (id: string, component: string) =>
    invoke<void>("set_window_component", { id, component }),

  destroyWindow: (id: string) => invoke<void>("destroy_window", { id }),

  monitors: () => invoke<MonitorInfo[]>("monitors"),

  configPreview: () => invoke<string>("config_preview"),

  configSchemaVersion: () => invoke<number>("config_schema_version"),
};

/** 数值格式化（数值卡 / 曲线标签）。 */
export function formatValue(value: SampleEvent["value"]): string {
  if (typeof value === "number") {
    // 整数不补零，小数保留 2 位并去尾零
    if (Number.isInteger(value)) return value.toString();
    return value.toFixed(2).replace(/\.?0+$/, "");
  }
  if (typeof value === "boolean") return value ? "是" : "否";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

/** 样本序列 → uPlot 数据（时间 ms + 数值；非数值样本记为 null）。 */
export function toPlotData(samples: SampleEvent[]): { x: number[]; y: (number | null)[] } {
  const x: number[] = [];
  const y: (number | null)[] = [];
  for (const s of samples) {
    x.push(s.timestamp_ms);
    y.push(typeof s.value === "number" ? s.value : null);
  }
  return { x, y };
}
