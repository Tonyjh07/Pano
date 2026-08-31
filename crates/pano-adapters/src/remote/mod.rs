//! 远程数据源共享基座（M1.2 R3 预留，架构 §14 / roadmap §M1.2）。
//!
//! 本模块只提供**共享骨架**，不实现具体远程适配器（归 M2+ 按需实现，
//! 接入路径见架构 §14）：
//!
//! - [`Backoff`]：简易指数退避（1s / 2s / 4s … 封顶 60s，架构 §9 约定）；
//! - [`http_poll`]（feature `remote-http`）：HTTP 轮询模板 —— 按采样周期
//!   GET → 回调解析（serde_json）→ `sink.push`；失败内置退避重试；
//! - [`ws_push`]（feature `remote-ws`）：WebSocket 推送模板 —— 长连接 →
//!   消息回调 → `sink.push`；断线重连内置简易退避；连接状态映射
//!   [`pano_core::adapter::AdapterStatus`]（连接中 = `Starting`，断线重连 = `Error`）。
//!
//! 凭据 / 请求参数走适配器自定义配置（`config_schema`），不新增 core 机制。

use std::time::Duration;

#[cfg(feature = "remote-http")]
pub mod http_poll;

#[cfg(feature = "remote-ws")]
pub mod ws_push;

/// 简易指数退避（架构 §9：1s / 2s / 4s … 封顶 60s）。
///
/// 线程安全、无共享状态；调用方（适配器任务）自持实例。
#[derive(Debug, Clone)]
pub struct Backoff {
    /// 基础等待时长。
    base: Duration,
    /// 封顶等待时长。
    max: Duration,
    /// 连续失败次数。
    attempts: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    /// 默认参数：base 1s，封顶 60s。
    pub fn new() -> Self {
        Self::with_params(Duration::from_secs(1), Duration::from_secs(60))
    }

    /// 自定义 base / 封顶。
    pub fn with_params(base: Duration, max: Duration) -> Self {
        Self {
            base,
            max,
            attempts: 0,
        }
    }

    /// 返回本次失败应等待的时长，并推进退避计数。
    ///
    /// 第 n 次连续失败等待 `base * 2^(n-1)`，封顶 `max`；指数上限 10 防溢出。
    pub fn next_delay(&mut self) -> Duration {
        let shift = self.attempts.min(10);
        let delay = self.base.saturating_mul(1u32 << shift);
        self.attempts = self.attempts.saturating_add(1);
        delay.min(self.max)
    }

    /// 连续失败次数（供断线重连状态展示）。
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// 成功恢复后重置退避。
    pub fn reset(&mut self) {
        self.attempts = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_exponential_and_cap() {
        let mut b = Backoff::with_params(Duration::from_millis(100), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_millis(100));
        assert_eq!(b.next_delay(), Duration::from_millis(200));
        assert_eq!(b.next_delay(), Duration::from_millis(400));
        assert_eq!(b.next_delay(), Duration::from_millis(800));
        assert_eq!(b.next_delay(), Duration::from_millis(1000), "封顶 1s");
        assert_eq!(b.next_delay(), Duration::from_millis(1000), "持续封顶");
        assert_eq!(b.attempts(), 6);
    }

    #[test]
    fn backoff_reset() {
        let mut b = Backoff::new();
        b.next_delay();
        b.next_delay();
        b.reset();
        assert_eq!(b.attempts(), 0);
        assert_eq!(b.next_delay(), Duration::from_secs(1), "重置后从头开始");
    }

    #[test]
    fn default_params_match_spec() {
        let mut b = Backoff::new();
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        // 默认封顶 60s：1,2,4,8,16,32,60...
        for _ in 0..10 {
            b.next_delay();
        }
        assert_eq!(b.next_delay(), Duration::from_secs(60));
    }
}
