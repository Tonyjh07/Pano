//! 系统监控适配器：`sys.net` —— 网络收发速率与链路利用率。
//!
//! - series：
//!   - `sys.net.recv_bps`（Number，字节/秒）：下载速率；
//!   - `sys.net.sent_bps`（Number，字节/秒）：上传速率；
//!   - `sys.net.utilization`（Number，%，M2.3）：**链路利用率** =（recv_bps +
//!     sent_bps）/（参考带宽）× 100，钳制 0..=100。
//! - 自定义配置：
//!   - `interface`（Text，可选）—— 网卡名子串过滤（大小写不敏感）；缺省聚合全部网卡；
//!   - `link_mbps`（Number，默认 1000 = 1 Gbps，M2.3）—— 参考链路带宽，用于折算利用率；
//!   - `high_threshold`（Number，默认 80，域 0..=100，M2.3）—— 高占用阈值
//!     （仪表盘指示灯判定用；适配器自身不使用，仅作配置暴露）。
//! - 能力：`SystemInfo` + `TimeSeries`。
//!
//! 速率为相邻两次刷新间累计字节差 / 经过时间；start 预热捕获累计基线、
//! 首 tick 不产出，首帧（预热后首个采样周期）即为真实速率（见 roadmap §M2）。

use std::collections::HashMap;
use std::time::Instant;

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
};
use pano_core::capability::Capability;

const ID: &str = "sys.net";
const METRIC_RECV: &str = "recv_bps";
const METRIC_SENT: &str = "sent_bps";
const METRIC_UTILIZATION: &str = "utilization";
const KEY_INTERFACE: &str = "interface";
const KEY_LINK_MBPS: &str = "link_mbps";
const DEFAULT_LINK_MBPS: f64 = 1000.0;

/// `sys.net` 适配器：每采样周期推送网络收发速率（可 `interface` 过滤）。
pub struct SysNet {
    status: AdapterStatus,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl SysNet {
    /// 构造网络适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: AdapterStatus::Stopped,
            task: None,
            runtime: None,
        }
    }

    /// 解析 `interface` 过滤子串；缺省 `None`，非文本值返回 [`AdapterError::Config`]。
    fn resolve_interface(
        config: &HashMap<String, ConfigValue>,
    ) -> Result<Option<String>, AdapterError> {
        match config.get(KEY_INTERFACE) {
            None => Ok(None),
            Some(ConfigValue::Text(s)) if !s.is_empty() => Ok(Some(s.clone())),
            Some(ConfigValue::Text(_)) => Err(AdapterError::Config("interface 不能为空".into())),
            Some(other) => Err(AdapterError::Config(format!(
                "interface 必须为文本（网卡名子串），实际 {other:?}"
            ))),
        }
    }

    /// 解析 `link_mbps`（参考带宽，> 0）；缺省 1000，非法返回 [`AdapterError::Config`]。
    fn resolve_link_mbps(config: &HashMap<String, ConfigValue>) -> Result<f64, AdapterError> {
        match config.get(KEY_LINK_MBPS) {
            None => Ok(DEFAULT_LINK_MBPS),
            Some(ConfigValue::Number(n)) if n.is_finite() && *n > 0.0 => Ok(*n),
            Some(other) => Err(AdapterError::Config(format!(
                "link_mbps 必须为 > 0 的数值，实际 {other:?}"
            ))),
        }
    }
}

impl Default for SysNet {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for SysNet {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "网络速率".to_string(),
            description: "网络收发速率与链路利用率（字节/秒 + 利用率 %，可按网卡过滤）".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::new(Capability::SYSTEM_INFO),
            Capability::new(Capability::TIME_SERIES),
        ]
    }

    fn series(&self) -> Vec<SeriesId> {
        let id = self.meta().id;
        vec![
            SeriesId::new(&id, METRIC_RECV),
            SeriesId::new(&id, METRIC_SENT),
            SeriesId::new(&id, METRIC_UTILIZATION),
        ]
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![
                ConfigField {
                    key: KEY_INTERFACE.to_string(),
                    label: "网卡过滤".to_string(),
                    kind: FieldKind::Text,
                    default: ConfigValue::Text(String::new()),
                    help: Some("仅统计网卡名包含该子串的接口（留空 = 聚合全部）".to_string()),
                },
                ConfigField {
                    key: KEY_LINK_MBPS.to_string(),
                    label: "参考带宽 (Mbps)".to_string(),
                    kind: FieldKind::Number,
                    default: ConfigValue::Number(DEFAULT_LINK_MBPS),
                    help: Some(
                        "链路参考带宽（Mbps），用于折算 sys.net.utilization（利用率 %）"
                            .to_string(),
                    ),
                },
                super::high_threshold_field(),
            ],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        let interface = Self::resolve_interface(&ctx.config)?;
        let link_mbps = Self::resolve_link_mbps(&ctx.config)?;
        // 校验高占用阈值（M2.3）：非法配置在启动即拒绝（一致性测试要求）。
        super::resolve_high_threshold(&ctx.config)?;
        self.status = AdapterStatus::Running;

        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let series_recv = SeriesId::new(&self.meta().id, METRIC_RECV);
        let series_sent = SeriesId::new(&self.meta().id, METRIC_SENT);
        let series_utilization = SeriesId::new(&self.meta().id, METRIC_UTILIZATION);
        let filter = interface.map(|i| i.to_lowercase());
        self.runtime = Some(ctx.runtime.clone());
        // 预热：首次刷新较慢，提前执行避免污染采样节奏（见 roadmap §M2）。
        // sysinfo 的 total_* 为自网卡启用以来的累计值：预热刷新后立刻捕获作为
        // 首帧基线，配合「首帧不产出」，避免首帧速率尖峰（见 roadmap M2 / 审查 B1）。
        let mut networks = sysinfo::Networks::new_with_refreshed_list();
        networks.refresh(true);
        let (mut prev_recv, mut prev_sent) = collect_usage(&networks, &filter);
        self.task = Some(ctx.runtime.spawn(async move {
            let mut prev_time = Instant::now();
            let mut first = true;
            let mut ticker = tokio::time::interval(sampling);
            // 慢周期（如 sysinfo 首次冷启动约 1s）不补发：Skip 而非默认 Burst，
            // 避免一次阻塞后连续补发导致采样间隔失真（见 roadmap §M2）。
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                // sysinfo 刷新为短阻塞（毫秒级）。直接同步调用：不用 block_in_place，
                // 避免其与 tokio time driver 在运行时关闭时的竞态 panic（见 roadmap §M2）。
                networks.refresh(true);
                let (recv, sent) = collect_usage(&networks, &filter);
                // 首帧：interval 首 tick 立即返回、elapsed 极小，仅更新基线不产出，
                // 避免速率尖峰（见 roadmap M2 / 审查 B1）。
                let elapsed = prev_time.elapsed().as_secs_f64();
                if !first && elapsed > 0.0 {
                    let recv_bps = recv.saturating_sub(prev_recv) as f64 / elapsed;
                    let sent_bps = sent.saturating_sub(prev_sent) as f64 / elapsed;
                    let utilization = super::link_utilization(recv_bps, sent_bps, link_mbps);
                    let timestamp = pano_core::adapter::now();
                    sink.push(
                        series_recv.clone(),
                        Sample {
                            timestamp,
                            value: SampleValue::Number(recv_bps),
                        },
                    );
                    sink.push(
                        series_sent.clone(),
                        Sample {
                            timestamp,
                            value: SampleValue::Number(sent_bps),
                        },
                    );
                    sink.push(
                        series_utilization.clone(),
                        Sample {
                            timestamp,
                            value: SampleValue::Number(utilization),
                        },
                    );
                    tracing::trace!(
                        target: "pano::adapters::sys_net",
                        recv_bps,
                        sent_bps,
                        utilization,
                        "推送网络样本"
                    );
                }
                prev_recv = recv;
                prev_sent = sent;
                prev_time = Instant::now();
                first = false;
            }
        }));

        tracing::info!(target: "pano::adapters::sys_net", "适配器已启动");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AdapterError> {
        // 上下文安全的 join：非运行时线程阻塞等待；运行时内（async 命令路径）
        // 仅 abort 不 block_on（避免「Cannot start a runtime from within a
        // runtime」panic），见 util::shutdown_task / roadmap §M2.1。
        crate::util::shutdown_task(self.task.take(), self.runtime.as_ref());
        self.status = AdapterStatus::Stopped;
        tracing::info!(target: "pano::adapters::sys_net", "适配器已停止");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }
}

/// 汇总匹配过滤的接口累计收发字节（sysinfo 的 `total_*` 为自网卡启用的累计值；
/// 速率 = 相邻两次采样差值 / 时间差，见 [`SysNet::start`]）。
fn collect_usage(networks: &sysinfo::Networks, filter: &Option<String>) -> (u64, u64) {
    let mut recv = 0u64;
    let mut sent = 0u64;
    for (name, data) in networks.list() {
        if let Some(filter) = filter
            && !name.to_lowercase().contains(filter.as_str())
        {
            continue;
        }
        recv += data.total_received();
        sent += data.total_transmitted();
    }
    (recv, sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn resolve_interface_default_and_valid() {
        let empty = HashMap::new();
        assert_eq!(SysNet::resolve_interface(&empty).unwrap(), None);

        let mut cfg = HashMap::new();
        cfg.insert(KEY_INTERFACE.to_string(), ConfigValue::Text("eth".into()));
        assert_eq!(
            SysNet::resolve_interface(&cfg).unwrap().as_deref(),
            Some("eth")
        );
    }

    #[test]
    fn resolve_interface_rejects_invalid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_INTERFACE.to_string(), ConfigValue::Text(String::new()));
        assert!(matches!(
            SysNet::resolve_interface(&cfg),
            Err(AdapterError::Config(_))
        ));

        let mut cfg = HashMap::new();
        cfg.insert(KEY_INTERFACE.to_string(), ConfigValue::Bool(true));
        assert!(matches!(
            SysNet::resolve_interface(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn resolve_link_mbps_default_and_valid() {
        let empty = HashMap::new();
        assert_eq!(SysNet::resolve_link_mbps(&empty).unwrap(), 1000.0);

        let mut cfg = HashMap::new();
        cfg.insert(KEY_LINK_MBPS.to_string(), ConfigValue::Number(100.0));
        assert_eq!(SysNet::resolve_link_mbps(&cfg).unwrap(), 100.0);
    }

    #[test]
    fn resolve_link_mbps_rejects_invalid() {
        // 非正数 / 非数值
        for bad in [0.0, -10.0, f64::NAN] {
            let mut cfg = HashMap::new();
            cfg.insert(KEY_LINK_MBPS.to_string(), ConfigValue::Number(bad));
            assert!(matches!(
                SysNet::resolve_link_mbps(&cfg),
                Err(AdapterError::Config(_))
            ));
        }
        let mut cfg = HashMap::new();
        cfg.insert(KEY_LINK_MBPS.to_string(), ConfigValue::Text("x".into()));
        assert!(matches!(
            SysNet::resolve_link_mbps(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn conformance() {
        let mut adapter = SysNet::new();
        let valid = HashMap::new();
        // 非法 high_threshold 也应在启动即拒绝（M2.3）
        let mut invalid = HashMap::new();
        invalid.insert(
            crate::sys::KEY_HIGH_THRESHOLD.to_string(),
            ConfigValue::Number(101.0),
        );
        let series = SeriesId::new(&adapter.meta().id, METRIC_RECV);
        pano_core::test_harness::run_all(
            &mut adapter,
            &[series],
            valid,
            Duration::from_millis(200),
            invalid,
        )
        .expect("一致性测试失败");
    }
}
