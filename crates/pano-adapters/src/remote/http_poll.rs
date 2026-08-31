//! HTTP 轮询模板（架构 §14 / roadmap M1.2 R3，feature `remote-http`）。
//!
//! 提供：
//! - [`ReqwestHttpClient`]：`pano_core::http::HttpClient` 的 reqwest 实现
//!   （错误映射：超时 → `HttpError::Timeout`、非 2xx → `HttpError::Status`）；
//! - [`poll_loop`]：轮询骨架 —— 按周期 GET，成功回调推送样本，失败走 core 级
//!   [`pano_core::retry::Retry`] 退避重试（连续失败达阈值 → `AdapterStatus::Error`，
//!   经 `on_status` 回调同步给适配器；仍继续后台重试，成功自动恢复）。
//!
//! ## 模板用法（远程适配器参考，如 `remote.http-sample`）
//!
//! ```ignore
//! fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
//!     let http = ctx
//!         .http
//!         .clone()
//!         .ok_or_else(|| AdapterError::NotAvailable("未注入 HttpClient".into()))?;
//!     let sink = ctx.sink.clone();
//!     let sampling = ctx.sampling;
//!     let url: String = /* 自定义配置 */;
//!     let headers: Vec<(String, String)> = /* 自定义配置 */;
//!     let status: Arc<Mutex<AdapterStatus>> = /* 适配器共享状态 */;
//!     self.task = Some(ctx.runtime.spawn(async move {
//!         let status_ref = status.clone();
//!         pano_adapters::remote::http_poll::poll_loop(
//!             http.as_ref(),
//!             &url,
//!             &headers,
//!             sampling,
//!             move |resp| {
//!                 // serde_json 解析响应体 → sink.push(series, sample)
//!             },
//!             move |new_status: pano_core::adapter::AdapterStatus| {
//!                 *status_ref.lock().unwrap() = new_status;
//!             },
//!         )
//!         .await;
//!     }));
//!     Ok(())
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use pano_core::adapter::AdapterStatus;
use pano_core::http::{HttpClient, HttpError, HttpResponse};

use crate::remote::Retry;

/// reqwest 实现的 [`HttpClient`]（pano-app 装配层按 feature 构造并注入 core）。
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    /// 默认 10s 超时。
    pub fn new() -> Result<Self, HttpError> {
        Self::with_timeout(Duration::from_secs(10))
    }

    /// 指定请求超时。
    pub fn with_timeout(timeout: Duration) -> Result<Self, HttpError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| HttpError::Other(format!("构建 reqwest 客户端失败：{e}")))?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestHttpClient {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'a>> {
        Box::pin(async move {
            let mut request = self.client.get(url);
            for (key, value) in headers {
                request = request.header(key, value);
            }
            let response = request.send().await.map_err(map_reqwest_error)?;
            let status = response.status().as_u16();
            // 先查状态码再读 body：大错误体不白白下载。
            if !(200..300).contains(&status) {
                return Err(HttpError::Status(status));
            }
            let body = response.bytes().await.map_err(map_reqwest_error)?.to_vec();
            Ok(HttpResponse { status, body })
        })
    }
}

/// 轮询循环骨架：成功 → 回调解析推送 → 等一个采样周期；
/// 失败 → core 级退避等待后重试（请求本身超时由客户端超时兜底）。
///
/// 状态经 `on_status` 回调同步：成功 → `Running`；连续失败达阈值 →
/// `AdapterStatus::Error { last_error }`（仍继续后台重试，成功自动恢复）。
///
/// 该循环不会自行退出，须运行在适配器任务中（`stop` 时 abort）。
pub async fn poll_loop(
    client: &dyn HttpClient,
    url: &str,
    headers: &[(String, String)],
    interval: Duration,
    mut on_response: impl FnMut(HttpResponse),
    mut on_status: impl FnMut(AdapterStatus),
) {
    let mut retry = Retry::new();
    loop {
        match client.get(url, headers).await {
            Ok(response) => {
                retry.on_success();
                on_status(AdapterStatus::Running);
                on_response(response);
                tokio::time::sleep(interval).await;
            }
            Err(error) => {
                let delay = retry.next_delay();
                if let Some(status) = retry.on_failure(error.to_string()) {
                    on_status(status);
                }
                tracing::warn!(
                    target: "pano::adapters::remote",
                    url,
                    error = %error,
                    failures = retry.failures(),
                    delay_ms = delay.as_millis(),
                    "HTTP 轮询失败，退避重试"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// 把 reqwest 错误映射为 [`HttpError`]。
fn map_reqwest_error(error: reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::Timeout(error.to_string())
    } else if let Some(status) = error.status() {
        HttpError::Status(status.as_u16())
    } else {
        HttpError::Other(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_client() {
        // 仅验证可构造（不发起网络请求）。
        let _client = ReqwestHttpClient::new().expect("构造成功");
    }
}
