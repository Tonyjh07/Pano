//! 系统监控适配器：`sys.cpu` —— CPU 使用率。
//!
//! - series：
//!   - `sys.cpu.usage`（Number，%）：全局平均使用率；
//!   - `sys.cpu.cores`（Json，可选）：`per_core = true` 时推送各核心使用率数组。
//! - 自定义配置：
//!   - `per_core`（Bool，默认 `false`）—— 是否额外推送各核心使用率。
//! - 能力：`SystemInfo` + `TimeSeries`。
//!
//! 注意：CPU 使用率需两次刷新才能得到有效值（首次为 0），首帧无碍。

use std::collections::HashMap;

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
};
use pano_core::capability::Capability;

const ID: &str = "sys.cpu";
const METRIC_USAGE: &str = "usage";
const METRIC_CORES: &str = "cores";
const KEY_PER_CORE: &str = "per_core";

/// `sys.cpu` 适配器：每采样周期推送全局平均 CPU 使用率（可含各核心明细）。
pub struct SysCpu {
    status: AdapterStatus,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl SysCpu {
    /// 构造 CPU 适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: AdapterStatus::Stopped,
            task: None,
            runtime: None,
        }
    }

    /// 解析 `per_core`；缺省 `false`，非布尔值返回 [`AdapterError::Config`]。
    fn resolve_per_core(config: &HashMap<String, ConfigValue>) -> Result<bool, AdapterError> {
        match config.get(KEY_PER_CORE) {
            None => Ok(false),
            Some(ConfigValue::Bool(b)) => Ok(*b),
            Some(other) => Err(AdapterError::Config(format!(
                "per_core 必须为布尔值，实际 {other:?}"
            ))),
        }
    }
}

impl Default for SysCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for SysCpu {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "CPU 使用率".to_string(),
            description: "系统 CPU 全局平均使用率（可含各核心明细）".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::new(Capability::SYSTEM_INFO),
            Capability::new(Capability::TIME_SERIES),
        ]
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![ConfigField {
                key: KEY_PER_CORE.to_string(),
                label: "各核心明细".to_string(),
                kind: FieldKind::Bool,
                default: ConfigValue::Bool(false),
                help: Some("是否额外推送各核心使用率（sys.cpu.cores，JSON 数组）".to_string()),
            }],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        let per_core = Self::resolve_per_core(&ctx.config)?;
        self.status = AdapterStatus::Running;

        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let series_usage = SeriesId::new(&self.meta().id, METRIC_USAGE);
        let series_cores = SeriesId::new(&self.meta().id, METRIC_CORES);
        self.runtime = Some(ctx.runtime.clone());
        // 预热：sysinfo 首次刷新较慢（Windows 上实测约 1s），提前执行，
        // 避免冷启动延迟污染采样节奏（interval 补爆 / 首样本间隔失真，见 roadmap §M2）。
        let mut system = sysinfo::System::new();
        system.refresh_cpu_usage();
        self.task = Some(ctx.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(sampling);
            // 慢周期（如某个刷新意外超时）不补发：Skip 而非默认 Burst，
            // 避免一次阻塞后连续补发导致采样间隔失真（见 roadmap §M2）。
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                // sysinfo 刷新为短阻塞（毫秒级）。直接同步调用：不用 block_in_place，
                // 避免其与 tokio time driver 在运行时关闭时的竞态 panic（见 roadmap §M2）。
                system.refresh_cpu_usage();
                let usage = system.global_cpu_usage() as f64;
                let timestamp = pano_core::adapter::now();
                sink.push(
                    series_usage.clone(),
                    Sample {
                        timestamp,
                        value: SampleValue::Number(usage),
                    },
                );
                // 仅 per_core 启用时才收集各核心明细，省去不必要的遍历。
                let cores: Vec<f64> = if per_core {
                    system.cpus().iter().map(|c| c.cpu_usage() as f64).collect()
                } else {
                    Vec::new()
                };
                if per_core {
                    sink.push(
                        series_cores.clone(),
                        Sample {
                            timestamp,
                            value: SampleValue::Json(serde_json::json!(cores)),
                        },
                    );
                }
                tracing::trace!(
                    target: "pano::adapters::sys_cpu",
                    usage,
                    cores = cores.len(),
                    "推送 CPU 样本"
                );
            }
        }));

        tracing::info!(target: "pano::adapters::sys_cpu", "适配器已启动");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AdapterError> {
        if let Some(task) = self.task.take() {
            task.abort();
            // abort 是异步的：阻塞等待任务真正结束，保证 stop 返回后不再产出样本。
            if let Some(rt) = &self.runtime {
                let _ = rt.block_on(task);
            }
        }
        self.status = AdapterStatus::Stopped;
        tracing::info!(target: "pano::adapters::sys_cpu", "适配器已停止");
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
    fn resolve_per_core_default_and_valid() {
        let empty = HashMap::new();
        assert!(!SysCpu::resolve_per_core(&empty).unwrap());

        let mut cfg = HashMap::new();
        cfg.insert(KEY_PER_CORE.to_string(), ConfigValue::Bool(true));
        assert!(SysCpu::resolve_per_core(&cfg).unwrap());
    }

    #[test]
    fn resolve_per_core_rejects_invalid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_PER_CORE.to_string(), ConfigValue::Text("x".into()));
        assert!(matches!(
            SysCpu::resolve_per_core(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn conformance() {
        let mut adapter = SysCpu::new();
        let valid = HashMap::new();
        let mut invalid = HashMap::new();
        invalid.insert(KEY_PER_CORE.to_string(), ConfigValue::Text("x".into()));
        let series = SeriesId::new(&adapter.meta().id, METRIC_USAGE);
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
