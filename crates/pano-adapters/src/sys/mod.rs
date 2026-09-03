//! 系统监控适配器（M2，按平台条件编译：Windows / Linux / macOS）。
//!
//! 每个指标一个适配器、一个 feature（`adapter-sys-<指标>`），共用 `sysinfo` crate
//! （跨平台系统信息采集；`system` / `disk` / `network` 三个 feature 满足全部所需）。
//!
//! | 适配器 | series | 说明 |
//! | --- | --- | --- |
//! | `sys.cpu` | `usage`（%）、`cores`（JSON，可选） | CPU 全局平均使用率 / 各核心明细 |
//! | `sys.mem` | `used_percent`（%）、`used_bytes`、`total_bytes`、`swap_percent`（可选） | 内存用量 |
//! | `sys.disk` | `used_percent`（%）、`used_bytes`、`total_bytes`、`active_percent`（M2.3，%）、`busiest_disk`（M2.3.1，文本） | 磁盘用量 / 最忙盘真实忙碌时间（Windows PDH 先行） |
//! | `sys.net` | `recv_bps`、`sent_bps`（字节/秒）、`utilization`（M2.3，%） | 网络收发速率 / 链路利用率 |
//!
//! 能力：`SystemInfo` + `TimeSeries`；本地采集几乎不会失败（无退避重试路径）。
//!
//! M2.3 起四个 sys 适配器共享配置字段 `high_threshold`（高占用阈值，Number 默认
//! 80，域 0..=100）：仪表盘指示灯判定用，适配器自身不使用该值（仅作为数据源
//! 配置暴露，UI 经 `list_adapters` 读取）；`sys.net` 另有 `link_mbps`（参考带宽）。
//!
//! 采集在 tokio 任务中运行，sysinfo 刷新为短阻塞操作（毫秒级），在任务内
//! **直接同步调用**（不用 `block_in_place`，避免其与 tokio time driver 在运行时
//! 关闭时的竞态 panic，见 roadmap §M2）；首次刷新冷启动较慢（约 1s），由各
//! 适配器在 `start` 中**预热**执行。

use std::collections::HashMap;

use pano_core::adapter::{AdapterError, ConfigField, ConfigValue, FieldKind};

#[cfg(feature = "adapter-sys-cpu")]
pub mod cpu;

#[cfg(feature = "adapter-sys-mem")]
pub mod mem;

#[cfg(feature = "adapter-sys-disk")]
pub mod disk;

#[cfg(feature = "adapter-sys-disk")]
pub(crate) mod disk_busy;

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

/// 高占用阈值默认值（%）。
pub const DEFAULT_HIGH_THRESHOLD: f64 = 80.0;
/// 高占用阈值配置键（四个 sys 适配器通用；仪表盘指示灯判定用）。
pub const KEY_HIGH_THRESHOLD: &str = "high_threshold";

/// 解析 `high_threshold`（0..=100）；缺省 80，非法返回 [`AdapterError::Config`]。
pub fn resolve_high_threshold(config: &HashMap<String, ConfigValue>) -> Result<f64, AdapterError> {
    match config.get(KEY_HIGH_THRESHOLD) {
        None => Ok(DEFAULT_HIGH_THRESHOLD),
        Some(ConfigValue::Number(n)) if n.is_finite() && (0.0..=100.0).contains(n) => Ok(*n),
        Some(other) => Err(AdapterError::Config(format!(
            "high_threshold 必须为 0..=100 的数值，实际 {other:?}"
        ))),
    }
}

/// 高占用阈值配置字段（四个 sys 适配器共用；M2.3，schema 驱动表单渲染）。
pub fn high_threshold_field() -> ConfigField {
    ConfigField {
        key: KEY_HIGH_THRESHOLD.to_string(),
        label: "高占用阈值".to_string(),
        kind: FieldKind::Number,
        default: ConfigValue::Number(DEFAULT_HIGH_THRESHOLD),
        help: Some("占用率超过该阈值（%，0..=100）时，资源仪表盘指示灯亮红".to_string()),
    }
}

/// 链路利用率（%）：总速率（recv + sent，字节/秒）/ 参考带宽 × 100，钳制 0..=100。
///
/// `1 Mbps = 125_000 B/s`；`link_mbps <= 0` 视为无效带宽，返回 0（避免除零）。
pub fn link_utilization(recv_bps: f64, sent_bps: f64, link_mbps: f64) -> f64 {
    let bytes_per_sec = link_mbps * 125_000.0;
    if bytes_per_sec <= 0.0 {
        return 0.0;
    }
    let pct = (recv_bps + sent_bps) / bytes_per_sec * 100.0;
    pct.clamp(0.0, 100.0)
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

    #[test]
    fn resolve_high_threshold_default_and_valid() {
        let empty = HashMap::new();
        assert_eq!(resolve_high_threshold(&empty).unwrap(), 80.0);

        let mut cfg = HashMap::new();
        cfg.insert(KEY_HIGH_THRESHOLD.to_string(), ConfigValue::Number(90.0));
        assert_eq!(resolve_high_threshold(&cfg).unwrap(), 90.0);

        let mut cfg = HashMap::new();
        cfg.insert(KEY_HIGH_THRESHOLD.to_string(), ConfigValue::Number(0.0));
        assert_eq!(resolve_high_threshold(&cfg).unwrap(), 0.0);
        let mut cfg = HashMap::new();
        cfg.insert(KEY_HIGH_THRESHOLD.to_string(), ConfigValue::Number(100.0));
        assert_eq!(resolve_high_threshold(&cfg).unwrap(), 100.0);
    }

    #[test]
    fn resolve_high_threshold_rejects_invalid() {
        // 超域（>100 / <0 / 非有限）
        for bad in [150.0, -5.0, f64::NAN, f64::INFINITY] {
            let mut cfg = HashMap::new();
            cfg.insert(KEY_HIGH_THRESHOLD.to_string(), ConfigValue::Number(bad));
            assert!(matches!(
                resolve_high_threshold(&cfg),
                Err(AdapterError::Config(_))
            ));
        }
        // 非数值
        let mut cfg = HashMap::new();
        cfg.insert(
            KEY_HIGH_THRESHOLD.to_string(),
            ConfigValue::Text("x".into()),
        );
        assert!(matches!(
            resolve_high_threshold(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn link_utilization_maps_rate_to_percent() {
        // 1 Gbps 参考：半速 = 50%
        assert_eq!(link_utilization(62_500_000.0, 0.0, 1000.0), 50.0);
        // 收发叠加：125 MB/s 收 + 125 MB/s 发 = 满 100%（钳制）
        assert_eq!(
            link_utilization(125_000_000.0, 125_000_000.0, 1000.0),
            100.0
        );
        // 超带宽 → 钳制到 100
        assert_eq!(link_utilization(500_000_000.0, 0.0, 1000.0), 100.0);
        // 空闲
        assert_eq!(link_utilization(0.0, 0.0, 1000.0), 0.0);
        // 参考带宽为 0 / 负 → 0（避免除零）
        assert_eq!(link_utilization(100.0, 100.0, 0.0), 0.0);
        assert_eq!(link_utilization(100.0, 100.0, -1.0), 0.0);
        // 1 Mbps = 125_000 B/s 语义：满速 = 100%
        assert_eq!(link_utilization(125_000.0, 0.0, 1.0), 100.0);
    }
}
