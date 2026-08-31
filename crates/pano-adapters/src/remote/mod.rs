//! 远程数据源共享基座（M1.2 R3 预留，架构 §14 / roadmap §M1.2；M2 落地具体远程适配器）。
//!
//! 本模块提供**共享骨架**（M2 实现的具体远程示例适配器见 `http_sample` / `ws_sample`，
//! 随对应 feature 编译）：
//!
//! - [`Backoff`] / [`Retry`]（再导出自 [`pano_core::retry`]）：core 级自动退避重试
//!   （1s / 2s / 4s … 封顶 60s + Error 阈值，架构 §9；M2 起 core 级单一共享定义）；
//! - [`http_poll`]（feature `remote-http`）：HTTP 轮询骨架 —— 按采样周期
//!   GET → 回调解析（serde_json）→ `sink.push`；失败走 core 级 `Retry` 退避重试；
//! - [`ws_push`]（feature `remote-ws`）：WebSocket 推送骨架 —— 长连接 →
//!   消息回调 → `sink.push`；断线重连内置简易退避；连接状态映射
//!   [`pano_core::adapter::AdapterStatus`]（连接中 = `Starting`，断线重连 = `Error`）。
//!
//! 凭据 / 请求参数走适配器自定义配置（`config_schema`），不新增 core 机制。

/// core 级退避 / 重试（架构 §9）的共享再导出，兼容 M1.2 模板写法（`crate::remote::Backoff`）。
pub use pano_core::retry::{Backoff, Retry};

#[cfg(feature = "remote-http")]
pub mod http_poll;

#[cfg(feature = "remote-ws")]
pub mod ws_push;
