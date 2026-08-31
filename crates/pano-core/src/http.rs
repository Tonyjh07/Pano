//! 远程数据源预留（M1.2 R3，架构 §14）：core 定义轻量 `HttpClient` 抽象。
//!
//! - core **不依赖**具体 HTTP 库（reqwest 等）；实现留在 pano-adapters 的
//!   remote 基座 / pano-app 装配层；
//! - `get` 返回 boxed future，保持 trait 可做 trait-object（dyn-compatible）；
//! - 凭据 / 请求参数走适配器自定义配置（`config_schema`），不新增 core 机制。

use std::future::Future;
use std::pin::Pin;

/// HTTP 响应（仅保留状态码与原始字节，解析交给适配器）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应体原始字节。
    pub body: Vec<u8>,
}

/// HTTP 客户端错误，映射自具体实现（reqwest 等）。
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    /// 请求超时。
    #[error("请求超时：{0}")]
    Timeout(String),

    /// 非 2xx 状态码。
    #[error("HTTP 状态错误：{0}")]
    Status(u16),

    /// IO 层错误。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    /// 其他错误（连接失败、DNS、TLS 等）。
    #[error("请求失败：{0}")]
    Other(String),
}

/// 轻量 HTTP 客户端抽象：core 只依赖本 trait，不依赖具体库。
pub trait HttpClient: Send + Sync {
    /// 发起 GET 请求，返回响应体。
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'a>>;
}
