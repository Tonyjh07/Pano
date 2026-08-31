//! 示例适配器：`example.sine` —— 正弦波发生器。
//!
//! - series：`example.sine.value`（Number）
//! - 自定义配置：
//!   - `amplitude`（Number，默认 1.0，必须 >= 0）
//!   - `frequency`（Number，默认 1.0，必须 > 0；单位：弧度每采样）

use std::collections::HashMap;

use pano_core::adapter::{
    Adapter, AdapterContext, AdapterError, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema,
    ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
};
use pano_core::capability::Capability;

const ID: &str = "example.sine";
const METRIC: &str = "value";
const KEY_AMPLITUDE: &str = "amplitude";
const KEY_FREQUENCY: &str = "frequency";
const PHASE_STEP_PER_SAMPLE: f64 = 0.1;

/// `example.sine` 适配器：每采样周期推送 `amplitude * sin(相位)`。
pub struct ExampleSine {
    status: AdapterStatus,
    task: Option<tokio::task::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl ExampleSine {
    /// 构造正弦波适配器（初始为 Stopped）。
    pub fn new() -> Self {
        Self {
            status: AdapterStatus::Stopped,
            task: None,
            runtime: None,
        }
    }

    /// 解析自定义配置；缺省 amplitude=1.0、frequency=1.0，非法值返回 [`AdapterError::Config`]。
    fn resolve_config(config: &HashMap<String, ConfigValue>) -> Result<(f64, f64), AdapterError> {
        let amplitude = match config.get(KEY_AMPLITUDE) {
            None => 1.0,
            Some(ConfigValue::Number(n)) if *n >= 0.0 => *n,
            Some(ConfigValue::Number(n)) => {
                return Err(AdapterError::Config(format!(
                    "amplitude 必须 >= 0，实际 {n}"
                )));
            }
            Some(other) => {
                return Err(AdapterError::Config(format!(
                    "amplitude 必须为数值，实际 {other:?}"
                )));
            }
        };
        let frequency = match config.get(KEY_FREQUENCY) {
            None => 1.0,
            Some(ConfigValue::Number(n)) if *n > 0.0 => *n,
            Some(ConfigValue::Number(n)) => {
                return Err(AdapterError::Config(format!(
                    "frequency 必须大于 0，实际 {n}"
                )));
            }
            Some(other) => {
                return Err(AdapterError::Config(format!(
                    "frequency 必须为数值，实际 {other:?}"
                )));
            }
        };
        Ok((amplitude, frequency))
    }
}

impl Default for ExampleSine {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter for ExampleSine {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: pano_core::adapter::AdapterId::new(ID),
            name: "示例正弦波".to_string(),
            description: "正弦波发生器，用于演示实时曲线与采样周期".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::new(Capability::TIME_SERIES)]
    }

    fn config_schema(&self) -> ConfigSchema {
        ConfigSchema {
            fields: vec![
                ConfigField {
                    key: KEY_AMPLITUDE.to_string(),
                    label: "振幅".to_string(),
                    kind: FieldKind::Number,
                    default: ConfigValue::Number(1.0),
                    help: Some("正弦波振幅（必须 >= 0）".to_string()),
                },
                ConfigField {
                    key: KEY_FREQUENCY.to_string(),
                    label: "频率".to_string(),
                    kind: FieldKind::Number,
                    default: ConfigValue::Number(1.0),
                    help: Some("相位推进速度（弧度每采样，必须 > 0）".to_string()),
                },
            ],
        }
    }

    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
        let (amplitude, frequency) = Self::resolve_config(&ctx.config)?;
        self.status = AdapterStatus::Running;

        let sink = ctx.sink.clone();
        let sampling = ctx.sampling;
        let series = SeriesId::new(&self.meta().id, METRIC);
        self.runtime = Some(ctx.runtime.clone());
        self.task = Some(ctx.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(sampling);
            let mut phase = 0.0_f64;
            loop {
                ticker.tick().await;
                phase += frequency * PHASE_STEP_PER_SAMPLE;
                let value = amplitude * phase.sin();
                sink.push(
                    series.clone(),
                    Sample {
                        timestamp: pano_core::adapter::now(),
                        value: SampleValue::Number(value),
                    },
                );
                tracing::trace!(target: "pano::adapters::example_sine", series = %series, value, "推送样本");
            }
        }));

        tracing::info!(target: "pano::adapters::example_sine", "适配器已启动");
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
        tracing::info!(target: "pano::adapters::example_sine", "适配器已停止");
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
    fn resolve_config_defaults() {
        let empty = HashMap::new();
        assert_eq!(ExampleSine::resolve_config(&empty).unwrap(), (1.0, 1.0));
    }

    #[test]
    fn resolve_config_valid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_AMPLITUDE.to_string(), ConfigValue::Number(2.0));
        cfg.insert(KEY_FREQUENCY.to_string(), ConfigValue::Number(0.5));
        assert_eq!(ExampleSine::resolve_config(&cfg).unwrap(), (2.0, 0.5));
    }

    #[test]
    fn resolve_config_rejects_invalid() {
        let mut cfg = HashMap::new();
        cfg.insert(KEY_AMPLITUDE.to_string(), ConfigValue::Number(-1.0));
        assert!(matches!(
            ExampleSine::resolve_config(&cfg),
            Err(AdapterError::Config(_))
        ));

        let mut cfg = HashMap::new();
        cfg.insert(KEY_FREQUENCY.to_string(), ConfigValue::Number(0.0));
        assert!(matches!(
            ExampleSine::resolve_config(&cfg),
            Err(AdapterError::Config(_))
        ));

        let mut cfg = HashMap::new();
        cfg.insert(KEY_AMPLITUDE.to_string(), ConfigValue::Text("x".into()));
        assert!(matches!(
            ExampleSine::resolve_config(&cfg),
            Err(AdapterError::Config(_))
        ));
    }

    #[test]
    fn conformance() {
        let mut adapter = ExampleSine::new();
        let mut valid = HashMap::new();
        valid.insert(KEY_AMPLITUDE.to_string(), ConfigValue::Number(1.0));
        valid.insert(KEY_FREQUENCY.to_string(), ConfigValue::Number(1.0));
        let mut invalid = HashMap::new();
        invalid.insert(KEY_FREQUENCY.to_string(), ConfigValue::Number(0.0));
        pano_core::test_harness::run_all(&mut adapter, valid, Duration::from_millis(50), invalid)
            .expect("一致性测试失败");
    }
}
