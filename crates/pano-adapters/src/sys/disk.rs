//! 系统监控适配器：`sys.disk` —— 磁盘用量。
//!
//! - series：
//!   - `sys.disk.used_percent`（Number，%）
//!   - `sys.disk.used_bytes`（Number）
//!   - `sys.disk.total_bytes`（Number）
//! - 自定义配置：
//!   - `device`（Text，可选）—— 磁盘名 / 挂载点子串过滤（大小写不敏感）；
//!     缺省统计全部磁盘。
//! - 能力：`SystemInfo` + `TimeSeries`。
//!
//! 用量为聚合值（used = Σ(total - available)，对匹配设备求和）。

use std::collections::HashMap;

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
};
use pano_core::capability::Capability;

use super::percent;

const ID: &str = "sys.disk";
const METRIC_USED_PERCENT: &str = "used_percent";
const METRIC_USED_BYTES: &str = "used_bytes";
const METRIC_TOTAL_BYTES: &str = "total_bytes";
const KEY_DEVICE: &str = "device";

/// `sys.disk` 适配器：每采样周期推送磁盘用量（可 `device` 过滤）。
pub struct SysDisk {
    status: AdapterStatus,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl SysDisk {
    /// 构造磁盘适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: AdapterStatus::Stopped,
            task: None,
            runtime: None,
        }
    }

    /// 解析 `device` 过滤子串；缺省 `None`，非文本值返回 [`AdapterError::Config`]。
    fn resolve_device(
        config: &HashMap<String, ConfigValue>,
    ) -> Result<Option<String>, AdapterError> {
        match config.get(KEY_DEVICE) {
            None => Ok(None),
            Some(ConfigValue::Text(s)) if !s.is_empty() => Ok(Some(s.clone())),
            Some(ConfigValue::Text(_)) => Err(AdapterError::Config("device 不能为空".into())),
            Some(other) => Err(AdapterError::Config(format!(
                "device 必须为文本（磁盘名 / 挂载点子串），实际 {other:?}"
            ))),
        }
    }
}

impl Default for SysDisk {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for SysDisk {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "磁盘使用率".to_string(),
            description: "系统磁盘用量（已用/总量/使用率，可按设备过滤）".to_string(),
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
            SeriesId::new(&id, METRIC_USED_PERCENT),
            SeriesId::new(&id, METRIC_USED_BYTES),
            SeriesId::new(&id, METRIC_TOTAL_BYTES),
        ]
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![ConfigField {
                key: KEY_DEVICE.to_string(),
                label: "设备过滤".to_string(),
                kind: FieldKind::Text,
                default: ConfigValue::Text(String::new()),
                help: Some("仅统计磁盘名 / 挂载点包含该子串的设备（留空 = 全部）".to_string()),
            }],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        let device = Self::resolve_device(&ctx.config)?;
        self.status = AdapterStatus::Running;

        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let series_used_percent = SeriesId::new(&self.meta().id, METRIC_USED_PERCENT);
        let series_used_bytes = SeriesId::new(&self.meta().id, METRIC_USED_BYTES);
        let series_total_bytes = SeriesId::new(&self.meta().id, METRIC_TOTAL_BYTES);
        let filter = device.map(|d| d.to_lowercase());
        self.runtime = Some(ctx.runtime.clone());
        // 预热：首次刷新较慢，提前执行避免污染采样节奏（见 roadmap §M2）。
        let mut disks = sysinfo::Disks::new_with_refreshed_list();
        disks.refresh(true);
        self.task = Some(ctx.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(sampling);
            // 慢周期（如某个刷新意外超时）不补发：Skip 而非默认 Burst（见 roadmap §M2）。
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                // sysinfo 刷新为短阻塞（毫秒级）。直接同步调用：不用 block_in_place，
                // 避免其与 tokio time driver 在运行时关闭时的竞态 panic（见 roadmap §M2）。
                disks.refresh(true);
                let mut used = 0u64;
                let mut total = 0u64;
                for disk in disks.list() {
                    if let Some(filter) = &filter {
                        let name = disk.name().to_string_lossy().to_lowercase();
                        let mount = disk.mount_point().to_string_lossy().to_lowercase();
                        if !name.contains(filter.as_str()) && !mount.contains(filter.as_str()) {
                            continue;
                        }
                    }
                    total += disk.total_space();
                    used += disk.total_space().saturating_sub(disk.available_space());
                }
                let timestamp = pano_core::adapter::now();
                sink.push(
                    series_used_percent.clone(),
                    Sample {
                        timestamp,
                        value: SampleValue::Number(percent(used, total)),
                    },
                );
                sink.push(
                    series_used_bytes.clone(),
                    Sample {
                        timestamp,
                        value: SampleValue::Number(used as f64),
                    },
                );
                sink.push(
                    series_total_bytes.clone(),
                    Sample {
                        timestamp,
                        value: SampleValue::Number(total as f64),
                    },
                );
                tracing::trace!(
                    target: "pano::adapters::sys_disk",
                    used_percent = percent(used, total),
                    used_bytes = used,
                    total_bytes = total,
                    "推送磁盘样本"
                );
            }
        }));

        tracing::info!(target: "pano::adapters::sys_disk", "适配器已启动");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AdapterError> {
        // 上下文安全的 join：非运行时线程阻塞等待；运行时内（async 命令路径）
        // 仅 abort 不 block_on（避免「Cannot start a runtime from within a
        // runtime」panic），见 util::shutdown_task / roadmap §M2.1。
        crate::util::shutdown_task(self.task.take(), self.runtime.as_ref());
        self.status = AdapterStatus::Stopped;
        tracing::info!(target: "pano::adapters::sys_disk", "适配器已停止");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn resolve_device_default_and_valid() {
        let empty = HashMap::new();
        assert_eq!(SysDisk::resolve_device(&empty).unwrap(), None);

        let mut cfg = HashMap::new();
        cfg.insert(KEY_DEVICE.to_string(), ConfigValue::Text("C:".into()));
        assert_eq!(
            SysDisk::resolve_device(&cfg).unwrap().as_deref(),
            Some("C:")
        );
    }

    #[test]
    fn resolve_device_rejects_invalid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_DEVICE.to_string(), ConfigValue::Text(String::new()));
        assert!(
            matches!(SysDisk::resolve_device(&cfg), Err(AdapterError::Config(_))),
            "空 device 应拒绝"
        );

        let mut cfg = HashMap::new();
        cfg.insert(KEY_DEVICE.to_string(), ConfigValue::Number(1.0));
        assert!(matches!(
            SysDisk::resolve_device(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn conformance() {
        let mut adapter = SysDisk::new();
        let valid = HashMap::new();
        let mut invalid = HashMap::new();
        invalid.insert(KEY_DEVICE.to_string(), ConfigValue::Number(1.0));
        let series = SeriesId::new(&adapter.meta().id, METRIC_USED_PERCENT);
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
