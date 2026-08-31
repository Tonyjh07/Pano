//! WebSocket 推送模板（架构 §14 / roadmap M1.2 R3，feature `remote-ws`）。
//!
//! 提供：
//! - [`WsConnectionState`]：连接状态 → [`AdapterStatus`] 映射
//!   （连接中 = `Starting`，已连接 = `Running`，断线重连 = `Error` + 计数，UI 可见）；
//! - [`run_ws_loop`]：长连接 + 断线重连骨架 —— 消息回调推送样本；
//!   断线按退避等待重连（模板内置简易退避，core 级自动退避归 M2）。
//!
//! ## 模板用法（M2 实现远程适配器时参考）
//!
//! ```ignore
//! fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
//!     let sink = ctx.sink.clone();
//!     let url: String = /* 自定义配置 */;
//!     let headers: Vec<(String, String)> = /* 自定义配置 */;
//!     self.task = Some(ctx.runtime.spawn(async move {
//!         pano_adapters::remote::ws_push::run_ws_loop(
//!             &url,
//!             headers,
//!             move |text| {
//!                 // 解析消息 → sink.push(series, sample)
//!             },
//!             |state| {
//!                 // 同步到适配器自持状态：*status = state.to_adapter_status()
//!             },
//!         )
//!         .await;
//!     }));
//!     Ok(())
//! }
//! ```
//!
//! 生命周期契约不变：`stop` 返回后不得再产出样本 —— WS 型适配器需在
//! `stop` 中关闭连接并 join 接收任务（见架构 §14 设计约束）。

use futures_util::StreamExt;
use pano_core::adapter::AdapterStatus;

use crate::remote::Backoff;

/// WS 连接状态（供适配器映射到 [`AdapterStatus`]，架构 §14.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsConnectionState {
    /// 连接中 / 重连中 → `AdapterStatus::Starting`。
    Connecting,
    /// 已连接 → `AdapterStatus::Running`。
    Connected,
    /// 断线（退避重连中）→ `AdapterStatus::Error { last_error }`。
    Reconnecting { attempts: u32, last_error: String },
}

impl WsConnectionState {
    /// 映射为适配器可见状态。
    pub fn to_adapter_status(&self) -> AdapterStatus {
        match self {
            Self::Connecting => AdapterStatus::Starting,
            Self::Connected => AdapterStatus::Running,
            Self::Reconnecting {
                attempts,
                last_error,
            } => AdapterStatus::Error {
                last_error: format!("WS 断线重连（第 {attempts} 次）：{last_error}"),
            },
        }
    }
}

/// WS 长连接 + 断线重连骨架：消息经 `on_message` 回调推送样本，
/// 状态变化经 `on_state` 回调同步到适配器状态。
///
/// 该循环不会自行退出，须运行在适配器任务中（`stop` 时 abort）。
pub async fn run_ws_loop(
    url: &str,
    headers: Vec<(String, String)>,
    mut on_message: impl FnMut(String),
    mut on_state: impl FnMut(WsConnectionState),
) {
    let mut backoff = Backoff::new();
    loop {
        on_state(WsConnectionState::Connecting);
        match connect_once(url, &headers).await {
            Ok(mut stream) => {
                on_state(WsConnectionState::Connected);
                let mut healthy = false;
                // 消息循环：断开（Err / None）后跳出，进入退避重连。
                loop {
                    match stream.next().await {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            healthy = true;
                            on_message(text.to_string());
                        }
                        Some(Ok(_)) => {} // 忽略二进制 / Ping / Pong 等
                        Some(Err(error)) => {
                            let attempts = backoff.attempts().saturating_add(1);
                            on_state(WsConnectionState::Reconnecting {
                                attempts,
                                last_error: error.to_string(),
                            });
                            break;
                        }
                        None => {
                            let attempts = backoff.attempts().saturating_add(1);
                            on_state(WsConnectionState::Reconnecting {
                                attempts,
                                last_error: "连接被对端关闭".into(),
                            });
                            break;
                        }
                    }
                }
                // 抖动保护：仅在连接曾收到消息（健康）时重置退避；
                // 「连上即断」的抖动对端退避持续升级，避免 1s 一次的重连风暴。
                if healthy {
                    backoff.reset();
                }
                let delay = backoff.next_delay();
                tracing::warn!(
                    target: "pano::adapters::remote",
                    url,
                    delay_ms = delay.as_millis(),
                    "WS 连接断开，退避重连"
                );
                tokio::time::sleep(delay).await;
            }
            Err(error) => {
                let delay = backoff.next_delay();
                on_state(WsConnectionState::Reconnecting {
                    attempts: backoff.attempts(),
                    last_error: error.to_string(),
                });
                tracing::warn!(
                    target: "pano::adapters::remote",
                    url,
                    delay_ms = delay.as_millis(),
                    error = %error,
                    "WS 连接失败，退避重试"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// 建立单次 WS 连接（带自定义请求头）。
async fn connect_once(
    url: &str,
    headers: &[(String, String)],
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Error,
> {
    use tokio_tungstenite::tungstenite::http;

    let mut request = http::Request::builder()
        .uri(url)
        .body(())
        // 请求构建错误（非法 URI 等）→ tungstenite::Error::HttpFormat（#[from] 自动转换）。
        .map_err(tokio_tungstenite::tungstenite::Error::from)?;
    for (key, value) in headers {
        let name = http::HeaderName::from_bytes(key.as_bytes())
            .map_err(tokio_tungstenite::tungstenite::Error::from)?;
        let value = value
            .parse::<http::HeaderValue>()
            .map_err(tokio_tungstenite::tungstenite::Error::from)?;
        request.headers_mut().insert(name, value);
    }
    let (stream, _response) = tokio_tungstenite::connect_async(request).await?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_mapping() {
        assert_eq!(
            WsConnectionState::Connecting.to_adapter_status(),
            AdapterStatus::Starting
        );
        assert_eq!(
            WsConnectionState::Connected.to_adapter_status(),
            AdapterStatus::Running
        );
        let err = WsConnectionState::Reconnecting {
            attempts: 3,
            last_error: "timeout".into(),
        }
        .to_adapter_status();
        match err {
            AdapterStatus::Error { last_error } => {
                assert!(last_error.contains("第 3 次"), "含重连计数：{last_error}");
            }
            other => panic!("期望 Error，实际 {other:?}"),
        }
    }
}
