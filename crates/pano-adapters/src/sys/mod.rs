//! 系统监控适配器（M2，按平台条件编译：Windows / Linux / macOS）。
//!
//! 每个指标一个适配器、一个 feature（`adapter-sys-<指标>`），共用 `sysinfo` crate
//! （跨平台系统信息采集；`system` / `disk` / `network` 三个 feature 满足全部所需）。
//!
//! | 适配器 | series | 说明 |
//! | --- | --- | --- |
//! | `sys.cpu` | `usage`（%）、`cores`（JSON，可选） | CPU 全局平均使用率 / 各核心明细 |
//! | `sys.mem` | `used_percent`（%）、`used_bytes`、`total_bytes`、`swap_percent`（可选） | 内存用量 |
//! | `sys.disk` | `used_percent`（%）、`used_bytes`、`total_bytes` | 磁盘用量（可 `device` 过滤） |
//! | `sys.net` | `recv_bps`、`sent_bps`（字节/秒速率） | 网络收发速率（可 `interface` 过滤） |
//!
//! 能力：`SystemInfo` + `TimeSeries`；本地采集几乎不会失败（无退避重试路径）。
//!
//! 采集在 tokio 任务中运行，sysinfo 刷新为短阻塞操作（毫秒级），在任务内
//! **直接同步调用**（不用 `block_in_place`，避免其与 tokio time driver 在运行时
//! 关闭时的竞态 panic，见 roadmap §M2）；首次刷新冷启动较慢（约 1s），由各
//! 适配器在 `start` 中**预热**执行。

#[cfg(feature = "adapter-sys-cpu")]
pub mod cpu;

#[cfg(feature = "adapter-sys-mem")]
pub mod mem;

#[cfg(feature = "adapter-sys-disk")]
pub mod disk;

#[cfg(feature = "adapter-sys-net")]
pub mod net;

/// 计算百分比（part / total * 100）；total 为 0 时返回 0（避免除零 / NaN）。
fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_basic() {
        assert_eq!(percent(50, 100), 50.0);
        assert_eq!(percent(0, 100), 0.0);
        assert_eq!(percent(100, 100), 100.0);
    }

    #[test]
    fn percent_zero_total_no_nan() {
        assert_eq!(percent(10, 0), 0.0, "total 为 0 时不得产生 NaN/无穷");
        assert_eq!(percent(0, 0), 0.0);
    }
}
