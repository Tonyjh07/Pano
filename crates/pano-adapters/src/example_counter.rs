//! 示例适配器：`example.counter` —— 每次采样递增的计数器。
//!
//! - series：`example.counter.value`（Number）
//! - 自定义配置：`step`（Number，默认 1.0，必须 > 0）

use std::collections::HashMap;
use std::time::SystemTime;

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
};
use pano_core::capability::Capability;

const ID: &str = "example.counter";
const METRIC: &str = "value";
const KEY_STEP: &str = "step";

/// `example.counter` 适配器：自持状态，`start` 后每采样周期推送一个递增数值。
pub struct ExampleCounter {
    status: AdapterStatus,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl ExampleCounter {
    /// 构造计数器适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: AdapterStatus::Stopped,
            task: None,
            runtime: None,
        }
    }

    /// 解析自定义配置中的 `step`；缺省 1.0，非法值返回 [`AdapterError::Config`]。
    fn resolve_step(config: &HashMap<String, ConfigValue>) -> Result<f64, AdapterError> {
        match config.get(KEY_STEP) {
            None => Ok(1.0),
            Some(ConfigValue::Number(n)) if *n > 0.0 => Ok(*n),
            Some(ConfigValue::Number(n)) => {
                Err(AdapterError::Config(format!("step 必须大于 0，实际 {n}")))
            }
            Some(other) => Err(AdapterError::Config(format!(
                "step 必须为数值，实际 {other:?}"
            ))),
        }
    }
}

impl Default for ExampleCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for ExampleCounter {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "示例计数器".to_string(),
            description: "每次采样递增的计数器，用于演示与验证链路".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::new(Capability::TIME_SERIES)]
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![ConfigField {
                key: KEY_STEP.to_string(),
                label: "步长".to_string(),
                kind: FieldKind::Number,
                default: ConfigValue::Number(1.0),
                help: Some("每次采样递增的步长（必须大于 0）".to_string()),
            }],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        let step = Self::resolve_step(&ctx.config)?;
        self.status = AdapterStatus::Running;

        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let series = SeriesId::new(&self.meta().id, METRIC);
        self.runtime = Some(ctx.runtime.clone());
        self.task = Some(ctx.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(sampling);
            let mut value = 0.0_f64;
            loop {
                ticker.tick().await;
                value += step;
                sink.push(
                    series.clone(),
                    Sample {
                        timestamp: SystemTime::now(),
                        value: SampleValue::Number(value),
                    },
                );
                tracing::trace!(target: "pano::adapters::example_counter", series = %series, value, "推送样本");
            }
        }));

        tracing::info!(target: "pano::adapters::example_counter", "适配器已启动");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AdapterError> {
        if let Some(task) = self.task.take() {
            task.abort();
            // abort 是异步的：阻塞等待任务真正结束，保证 stop 返回后不再产出样本
            //（一致性测试「生命周期检查」依赖此语义）。
            if let Some(rt) = &self.runtime {
                let _ = rt.block_on(task);
            }
        }
        self.status = AdapterStatus::Stopped;
        tracing::info!(target: "pano::adapters::example_counter", "适配器已停止");
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
    fn resolve_step_default_and_valid() {
        let empty = HashMap::new();
        assert_eq!(ExampleCounter::resolve_step(&empty).unwrap(), 1.0);

        let mut cfg = HashMap::new();
        cfg.insert(KEY_STEP.to_string(), ConfigValue::Number(2.5));
        assert_eq!(ExampleCounter::resolve_step(&cfg).unwrap(), 2.5);
    }

    #[test]
    fn resolve_step_rejects_invalid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_STEP.to_string(), ConfigValue::Number(0.0));
        assert!(matches!(
            ExampleCounter::resolve_step(&cfg),
            Err(AdapterError::Config(_))
        ));

        let mut cfg = HashMap::new();
        cfg.insert(KEY_STEP.to_string(), ConfigValue::Text("x".into()));
        assert!(matches!(
            ExampleCounter::resolve_step(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn conformance() {
        let mut adapter = ExampleCounter::new();
        let mut valid = HashMap::new();
        valid.insert(KEY_STEP.to_string(), ConfigValue::Number(2.0));
        let mut invalid = HashMap::new();
        invalid.insert(KEY_STEP.to_string(), ConfigValue::Number(0.0));
        pano_core::test_harness::run_all(&mut adapter, valid, Duration::from_millis(50), invalid)
            .expect("一致性测试失败");
    }
}
