//! 系统监控适配器：`sys.mem` —— 内存用量。
//!
//! - series：
//!   - `sys.mem.used_percent`（Number，%）
//!   - `sys.mem.used_bytes`（Number）
//!   - `sys.mem.total_bytes`（Number）
//!   - `sys.mem.swap_percent`（Number，可选）：`swap = true` 时额外推送交换分区使用率。
//! - 自定义配置：
//!   - `swap`（Bool，默认 `false`）—— 是否额外推送交换分区使用率；
//!   - `high_threshold`（Number，默认 80，域 0..=100，M2.3）—— 高占用阈值
//!     （仪表盘指示灯判定用；适配器自身不使用，仅作配置暴露）。
//! - 能力：`SystemInfo` + `TimeSeries`。

use std::collections::HashMap;

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
};
use pano_core::capability::Capability;

use super::percent;

const ID: &str = "sys.mem";
const METRIC_USED_PERCENT: &str = "used_percent";
const METRIC_USED_BYTES: &str = "used_bytes";
const METRIC_TOTAL_BYTES: &str = "total_bytes";
const METRIC_SWAP_PERCENT: &str = "swap_percent";
const KEY_SWAP: &str = "swap";

/// `sys.mem` 适配器：每采样周期推送内存用量（可选交换分区）。
pub struct SysMem {
    status: AdapterStatus,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl SysMem {
    /// 构造内存适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: AdapterStatus::Stopped,
            task: None,
            runtime: None,
        }
    }

    /// 解析 `swap`；缺省 `false`，非布尔值返回 [`AdapterError::Config`]。
    fn resolve_swap(config: &HashMap<String, ConfigValue>) -> Result<bool, AdapterError> {
        match config.get(KEY_SWAP) {
            None => Ok(false),
            Some(ConfigValue::Bool(b)) => Ok(*b),
            Some(other) => Err(AdapterError::Config(format!(
                "swap 必须为布尔值，实际 {other:?}"
            ))),
        }
    }
}

impl Default for SysMem {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for SysMem {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "内存使用率".to_string(),
            description: "系统内存用量（已用/总量/使用率，可选交换分区）".to_string(),
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
            SeriesId::new(&id, METRIC_SWAP_PERCENT),
        ]
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![
                ConfigField {
                    key: KEY_SWAP.to_string(),
                    label: "交换分区".to_string(),
                    kind: FieldKind::Bool,
                    default: ConfigValue::Bool(false),
                    help: Some("是否额外推送交换分区使用率（sys.mem.swap_percent）".to_string()),
                },
                super::high_threshold_field(),
            ],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        let swap = Self::resolve_swap(&ctx.config)?;
        // 校验高占用阈值（M2.3）：非法配置在启动即拒绝（一致性测试要求）。
        super::resolve_high_threshold(&ctx.config)?;
        self.status = AdapterStatus::Running;

        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let series_used_percent = SeriesId::new(&self.meta().id, METRIC_USED_PERCENT);
        let series_used_bytes = SeriesId::new(&self.meta().id, METRIC_USED_BYTES);
        let series_total_bytes = SeriesId::new(&self.meta().id, METRIC_TOTAL_BYTES);
        let series_swap_percent = SeriesId::new(&self.meta().id, METRIC_SWAP_PERCENT);
        self.runtime = Some(ctx.runtime.clone());
        // 预热：sysinfo 首次刷新较慢，提前执行避免污染采样节奏（见 roadmap §M2）。
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        self.task = Some(ctx.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(sampling);
            // 慢周期（如某个刷新意外超时）不补发：Skip 而非默认 Burst（见 roadmap §M2）。
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                // sysinfo 刷新为短阻塞（毫秒级）。直接同步调用：不用 block_in_place（见 roadmap §M2）。
                system.refresh_memory();
                let used = system.used_memory();
                let total = system.total_memory();
                let used_swap = system.used_swap();
                let total_swap = system.total_swap();
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
                if swap {
                    sink.push(
                        series_swap_percent.clone(),
                        Sample {
                            timestamp,
                            value: SampleValue::Number(percent(used_swap, total_swap)),
                        },
                    );
                }
                tracing::trace!(
                    target: "pano::adapters::sys_mem",
                    used_percent = percent(used, total),
                    used_bytes = used,
                    total_bytes = total,
                    "推送内存样本"
                );
            }
        }));

        tracing::info!(target: "pano::adapters::sys_mem", "适配器已启动");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AdapterError> {
        // 上下文安全的 join：非运行时线程阻塞等待；运行时内（async 命令路径）
        // 仅 abort 不 block_on（避免「Cannot start a runtime from within a
        // runtime」panic），见 util::shutdown_task / roadmap §M2.1。
        crate::util::shutdown_task(self.task.take(), self.runtime.as_ref());
        self.status = AdapterStatus::Stopped;
        tracing::info!(target: "pano::adapters::sys_mem", "适配器已停止");
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
    fn resolve_swap_default_and_valid() {
        let empty = HashMap::new();
        assert!(!SysMem::resolve_swap(&empty).unwrap());

        let mut cfg = HashMap::new();
        cfg.insert(KEY_SWAP.to_string(), ConfigValue::Bool(true));
        assert!(SysMem::resolve_swap(&cfg).unwrap());
    }

    #[test]
    fn resolve_swap_rejects_invalid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_SWAP.to_string(), ConfigValue::Number(1.0));
        assert!(matches!(
            SysMem::resolve_swap(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn conformance() {
        let mut adapter = SysMem::new();
        let valid = HashMap::new();
        // 非法 high_threshold 也应在启动即拒绝（M2.3）
        let mut invalid = HashMap::new();
        invalid.insert(
            crate::sys::KEY_HIGH_THRESHOLD.to_string(),
            ConfigValue::Number(-1.0),
        );
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
