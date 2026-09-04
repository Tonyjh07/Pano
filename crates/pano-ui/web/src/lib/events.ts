// 事件订阅（ui.md §6）：core → 前端事件推送，前端按组件 series 过滤；
// 高频事件合并（可配置节流，如曲线 500ms 粒度重绘）。
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SampleEvent } from "./api";

export const EVENT_SAMPLE = "pano://sample";

/** 窗口内容变更（切换组件类型，命令层定向 emit 给目标窗口）。 */
export const EVENT_WINDOW_COMPONENT = "pano://window-component";

/** 窗口无边框切换（M2.4，命令层定向 emit 给目标窗口，payload: boolean）。 */
export const EVENT_WINDOW_DECORATIONS = "pano://window-decorations";

export interface SampleSubscription {
  /** 取消订阅。 */
  unlisten: UnlistenFn;
}

/**
 * 订阅样本事件。
 *
 * @param onSample 样本回调（已按 series 过滤/合并）
 * @param series   关注的 series 列表；为空则不过滤
 * @param throttleMs 合并窗口（毫秒）；0 = 不合并
 */
export async function onSample(
  onSample: (event: SampleEvent) => void,
  series: string[] = [],
  throttleMs = 0,
): Promise<UnlistenFn> {
  const pending = new Map<string, SampleEvent>();
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    for (const ev of pending.values()) onSample(ev);
    pending.clear();
    timer = null;
  };

  const unlisten = await listen<SampleEvent>(EVENT_SAMPLE, (event) => {
    const ev = event.payload;
    if (series.length > 0 && !series.includes(ev.series)) return;
    pending.set(ev.series, ev);
    if (throttleMs <= 0) {
      flush();
      return;
    }
    if (timer === null) {
      timer = setTimeout(flush, throttleMs);
    }
  });

  // 取消订阅时同时清理挂起的节流定时器，避免销毁后多余的一次 flush。
  return () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    pending.clear();
    unlisten();
  };
}
