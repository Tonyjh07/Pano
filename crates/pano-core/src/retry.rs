//! 运行期故障的自动退避重试（架构 §9，M2 交付「core 级自动退避」）。
//!
//! 提供：
//! - [`Backoff`]：简易指数退避（1s / 2s / 4s … 封顶 60s），核心层唯一共享定义
//!   （M1.2 时位于 pano-adapters remote 基座，M2 迁入 core 供全部适配器复用）；
//! - [`Retry`]：连续失败计数 + 退避 + `Error` 阈值，把「采集异常 → 退避重试 →
//!   超过阈值进入 Error（UI 标记）→ 成功自动恢复」固化为一个可测试的状态机。
//!
//! 语义（架构 §9 / roadmap M1 遗留）：
//! - 连续失败未达阈值：适配器保持原状态，仅退避等待（采集失败为可恢复的抖动）；
//! - 连续失败达到阈值：转为 [`AdapterStatus::Error`]（UI 红色标记 + 错误详情），
//!   但适配器**继续**后台退避重试（监控自动恢复，无需人工干预；
//!   管理界面 Restart = 立即重置重试）；
//! - 任一成功：重置退避与计数，回到运行状态。

use std::time::Duration;

use crate::adapter::AdapterStatus;

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

/// 运行期故障的自动重试状态机（架构 §9）。
///
/// 适配器采集循环的失败路径统一经此跟踪：记录连续失败、推进退避，
/// 达到阈值后返回 [`AdapterStatus::Error`]；成功时调用 [`Retry::on_success`]
/// 重置并回到运行状态。`Backoff` 与 `Retry` 是独立计数器：
/// `next_delay()` 只推进退避计数，`on_failure()` 只推进连续失败计数。
#[derive(Debug, Clone)]
pub struct Retry {
    backoff: Backoff,
    consecutive_failures: u32,
    /// 连续失败达到该次数后转为 Error 状态（仍继续后台重试）。
    error_after: u32,
}

impl Default for Retry {
    fn default() -> Self {
        Self::new()
    }
}

impl Retry {
    /// 默认参数：退避 1s..60s，连续失败 3 次进入 Error。
    pub fn new() -> Self {
        Self::with_params(Duration::from_secs(1), Duration::from_secs(60), 3)
    }

    /// 自定义退避与 Error 阈值。
    pub fn with_params(base: Duration, max: Duration, error_after: u32) -> Self {
        Self {
            backoff: Backoff::with_params(base, max),
            consecutive_failures: 0,
            error_after: error_after.max(1),
        }
    }

    /// 记录一次成功：重置退避与连续失败计数。
    pub fn on_success(&mut self) {
        self.backoff.reset();
        self.consecutive_failures = 0;
    }

    /// 记录一次失败并返回应切换到的状态：
    /// - 未达阈值 → `None`（保持原状态，仅需按 [`Retry::next_delay`] 退避等待）；
    /// - 达到阈值 → `Some(AdapterStatus::Error { last_error })`（仍继续后台重试）。
    pub fn on_failure(&mut self, last_error: impl Into<String>) -> Option<AdapterStatus> {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= self.error_after {
            Some(AdapterStatus::Error {
                last_error: last_error.into(),
            })
        } else {
            None
        }
    }

    /// 本次失败应等待的退避时长（推进退避计数）。
    pub fn next_delay(&mut self) -> Duration {
        self.backoff.next_delay()
    }

    /// 连续失败次数。
    pub fn failures(&self) -> u32 {
        self.consecutive_failures
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
    fn backoff_default_params_match_spec() {
        let mut b = Backoff::new();
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        // 默认封顶 60s：1,2,4,8,16,32,60...
        for _ in 0..10 {
            b.next_delay();
        }
        assert_eq!(b.next_delay(), Duration::from_secs(60));
    }

    #[test]
    fn retry_below_threshold_returns_none() {
        let mut r = Retry::with_params(Duration::from_millis(10), Duration::from_millis(100), 3);
        assert_eq!(r.on_failure("e1"), None);
        assert_eq!(r.on_failure("e2"), None);
        assert_eq!(r.failures(), 2);
    }

    #[test]
    fn retry_threshold_reaches_error() {
        let mut r = Retry::with_params(Duration::from_millis(10), Duration::from_millis(100), 3);
        r.on_failure("e1");
        r.on_failure("e2");
        let status = r.on_failure("boom").expect("达到阈值应返回 Error");
        match status {
            AdapterStatus::Error { last_error } => {
                assert_eq!(last_error, "boom");
            }
            other => panic!("期望 Error，实际 {other:?}"),
        }
        assert_eq!(r.failures(), 3);
    }

    #[test]
    fn retry_success_resets_and_recovers() {
        let mut r = Retry::with_params(Duration::from_millis(10), Duration::from_millis(100), 2);
        r.on_failure("e1");
        assert!(r.on_failure("e2").is_some(), "达到阈值");
        r.on_success();
        assert_eq!(r.failures(), 0);
        // 恢复后从头计数：连续失败 1 次未达阈值
        assert_eq!(r.on_failure("again"), None);
        assert_eq!(r.next_delay(), Duration::from_millis(10), "退避已重置");
    }

    #[test]
    fn retry_next_delay_independent_of_failure_counter() {
        let mut r = Retry::with_params(Duration::from_millis(100), Duration::from_secs(1), 3);
        r.on_failure("e1");
        r.on_failure("e2");
        // 失败计数与退避计数相互独立：next_delay 是第 1 次等待（100ms），
        // 不受 on_failure 调用次数影响；failure 计数推进为 2 且未达阈值。
        assert_eq!(r.next_delay(), Duration::from_millis(100));
        assert_eq!(r.failures(), 2);
        assert_eq!(r.next_delay(), Duration::from_millis(200), "退避继续推进");
    }

    #[test]
    fn retry_error_after_zero_clamped() {
        // error_after 至少为 1：首次失败即进入 Error。
        let mut r = Retry::with_params(Duration::from_millis(10), Duration::from_millis(100), 0);
        assert!(r.on_failure("e").is_some(), "error_after 钳制为 1");
    }
}
