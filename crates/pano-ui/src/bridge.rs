//! 事件桥接（core → 前端，ui.md §6）：订阅 SampleStore 变化，
//! 任一 series 有新样本 → Tauri 事件 emit（按窗口订阅分发：前端按
//! 自己组件的 series 过滤）。**无轮询、无帧内检测**。

use std::sync::Arc;

use pano_core::sample_store::SampleStore;
use tauri::{AppHandle, Emitter};

use crate::dto::SampleEventDto;

/// 前端监听的事件名（core 样本推送）。
pub const EVENT_SAMPLE: &str = "pano://sample";

/// 前端监听的事件名（窗口内容变更：切换组件类型 / series，命令层定向 emit 给目标窗口）。
pub const EVENT_WINDOW_COMPONENT: &str = "pano://window-component";

/// 启动桥接任务：在 tauri 异步运行时上订阅 SampleStore，样本到达即 emit。
///
/// 前端加载完成后先拉取快照（`series_history` 命令）再增量订阅；
/// 订阅者消费慢时收到 `Lagged`（丢弃旧通知，读端以快照 / 最新值兜底）；
/// 全部接收端关闭（应用退出）时任务自然结束。
pub fn spawn_bridge(app: AppHandle, store: Arc<SampleStore>) {
    let mut rx = store.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(series) => {
                    if let Some(sample) = store.latest(&series) {
                        let payload = SampleEventDto::from_sample(&series, &sample);
                        let _ = app.emit(EVENT_SAMPLE, payload);
                    }
                }
                // 消费慢：跳过积压通知（前端曲线只画最近 N 点，丢旧无碍）。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
