//! 生命周期：适配器启停、状态查询、能力校验、配置应用（热生效）。
//!
//! 关闭顺序（架构 §5）：先停适配器任务，再退 UI。

use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Handle;

use crate::adapter::{AdapterContext, AdapterId, AdapterMeta, AdapterStatus, SampleSink};
use crate::capability::UISpec;
use crate::config::{AdapterConfig, PanoConfig};
use crate::error::CoreError;
use crate::registry::Registry;
use crate::sample_store::SampleStore;

/// 适配器生命周期管理器：编排注册表、样本存储与配置。
pub struct Lifecycle {
    registry: Registry,
    store: Arc<SampleStore>,
    config: PanoConfig,
    runtime: Handle,
}

impl Lifecycle {
    /// 由注册表与配置构造；配置引用了未注册的适配器 → [`CoreError::UnknownAdapter`]。
    pub fn new(registry: Registry, config: PanoConfig, runtime: Handle) -> Result<Self, CoreError> {
        for id in config.adapters.keys() {
            if registry.get(&AdapterId::new(id.as_str())).is_none() {
                return Err(CoreError::UnknownAdapter(AdapterId::new(id.as_str())));
            }
        }
        Ok(Self {
            registry,
            store: Arc::new(SampleStore::new()),
            config,
            runtime,
        })
    }

    /// 样本存储（只读 API 供 UI）。
    pub fn store(&self) -> &SampleStore {
        &self.store
    }

    /// 样本存储共享句柄（UI 读快照免锁路径）。
    pub fn store_arc(&self) -> Arc<SampleStore> {
        Arc::clone(&self.store)
    }

    /// 当前配置（供 app 序列化写回 pano.toml）。
    pub fn config(&self) -> &PanoConfig {
        &self.config
    }

    /// 启动全部启用中的适配器；单个失败不阻塞其他（记录日志并转 Error 状态）。
    pub fn start_all(&mut self) {
        let ids = self.registry.ids();
        for id in ids {
            if self.config.is_enabled(id.as_str())
                && let Err(e) = self.start(&id)
            {
                tracing::error!(
                    target: "pano::core",
                    adapter = %id,
                    error = %e,
                    "启动适配器失败"
                );
            }
        }
    }

    /// 停止全部适配器。
    pub fn stop_all(&mut self) {
        let ids = self.registry.ids();
        for id in ids {
            if let Err(e) = self.stop(&id) {
                tracing::debug!(target: "pano::core", adapter = %id, error = %e, "停止适配器跳过（未在运行）");
            }
        }
    }

    /// 启动单个适配器。
    ///
    /// 状态由适配器自持；`start` 失败后适配器应将自身置为
    /// `Error` / `Stopped`（一致性测试基座覆盖该约定）。
    pub fn start(&mut self, id: &AdapterId) -> Result<(), CoreError> {
        let status = self.status_of(id);
        if matches!(status, AdapterStatus::Running | AdapterStatus::Starting) {
            return Err(CoreError::AlreadyRunning(id.clone()));
        }

        let sampling = Duration::from_millis(self.config.sampling_ms_of(id.as_str()));
        let custom = self.config.custom_config(id.as_str())?;
        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let sink = SampleSink::new(move |series, sample| store.push(series, sample));

        let ctx = AdapterContext {
            sink,
            sampling,
            runtime,
            config: custom,
        };

        let adapter = self
            .registry
            .get_mut(id)
            .ok_or_else(|| CoreError::UnknownAdapter(id.clone()))?;
        adapter
            .start(ctx)
            .map_err(|e| CoreError::Config(format!("{e}")))
    }

    /// 停止单个适配器；必须干净退出。
    pub fn stop(&mut self, id: &AdapterId) -> Result<(), CoreError> {
        let status = self.status_of(id);
        if !matches!(
            status,
            AdapterStatus::Running | AdapterStatus::Starting | AdapterStatus::Error { .. }
        ) {
            return Err(CoreError::NotRunning(id.clone()));
        }
        let adapter = self
            .registry
            .get_mut(id)
            .ok_or_else(|| CoreError::UnknownAdapter(id.clone()))?;
        adapter
            .stop()
            .map_err(|e| CoreError::Config(format!("{e}")))
    }

    /// 查询适配器状态；配置未启用 → [`AdapterStatus::Disabled`]。
    pub fn status_of(&self, id: &AdapterId) -> AdapterStatus {
        if !self.config.is_enabled(id.as_str()) {
            return AdapterStatus::Disabled;
        }
        self.registry
            .get(id)
            .map(|a| a.status())
            .unwrap_or(AdapterStatus::Stopped)
    }

    /// 全部已注册适配器 id。
    pub fn adapter_ids(&self) -> Vec<AdapterId> {
        self.registry.ids()
    }

    /// 适配器元信息（供管理页展示）。
    pub fn adapter_meta(&self, id: &AdapterId) -> Option<AdapterMeta> {
        self.registry.get(id).map(|a| a.meta())
    }

    /// 适配器提供的能力。
    pub fn capabilities_of(&self, id: &AdapterId) -> Option<Vec<crate::capability::Capability>> {
        self.registry.get(id).map(|a| a.capabilities())
    }

    /// 适配器自定义配置 schema（供管理页渲染表单）。
    pub fn config_schema_of(&self, id: &AdapterId) -> Option<crate::adapter::ConfigSchema> {
        self.registry.get(id).map(|a| a.config_schema())
    }

    /// 能力校验（架构 §7）：`UISpec.requires ⊆ 已启用适配器的能力并集`。
    pub fn validate_uispec(&self, spec: &UISpec) -> Result<(), CoreError> {
        let mut have: Vec<_> = Vec::new();
        for id in self.registry.ids() {
            if self.config.is_enabled(id.as_str())
                && let Some(adapter) = self.registry.get(&id)
            {
                have.extend(adapter.capabilities());
            }
        }
        let missing: Vec<_> = spec
            .requires
            .iter()
            .filter(|c| !have.contains(c))
            .cloned()
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(CoreError::CapabilityUnsatisfied { missing })
        }
    }

    /// 应用新配置（热生效）：仅重启受影响适配器。
    ///
    /// 流程：停止（若在运行）→ 更新内存配置 → 启动（若启用）。
    /// 调用方（app）负责把新配置写回 `pano.toml`。
    pub fn apply_adapter_config(
        &mut self,
        id: &AdapterId,
        new_cfg: AdapterConfig,
    ) -> Result<(), CoreError> {
        if self.registry.get(id).is_none() {
            return Err(CoreError::UnknownAdapter(id.clone()));
        }
        let running = matches!(
            self.status_of(id),
            AdapterStatus::Running | AdapterStatus::Starting | AdapterStatus::Error { .. }
        );
        if running {
            self.stop(id)?;
        }
        self.config
            .adapters
            .insert(id.as_str().to_string(), new_cfg.clone());
        if new_cfg.enabled {
            self.start(id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{
        Adapter, AdapterError, AdapterMeta, ConfigSchema, Sample, SampleValue, SeriesId,
    };
    use crate::capability::Capability;
    use std::collections::HashMap;
    use std::time::SystemTime;

    struct FakeAdapter {
        id: AdapterId,
        status: AdapterStatus,
        task: Option<tokio::task::JoinHandle<()>>,
        fail_start: bool,
    }

    impl FakeAdapter {
        fn new(id: &str) -> Self {
            Self {
                id: AdapterId::new(id),
                status: AdapterStatus::Stopped,
                task: None,
                fail_start: false,
            }
        }
    }

    impl Adapter for FakeAdapter {
        fn meta(&self) -> AdapterMeta {
            AdapterMeta {
                id: self.id.clone(),
                name: "fake".into(),
                description: String::new(),
                version: "0.1.0".into(),
            }
        }

        fn capabilities(&self) -> Vec<Capability> {
            vec![Capability::new(Capability::TIME_SERIES)]
        }

        fn config_schema(&self) -> ConfigSchema {
            ConfigSchema::empty()
        }

        fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
            if self.fail_start {
                return Err(AdapterError::Config("故意失败".into()));
            }
            self.status = AdapterStatus::Running;
            let sink = ctx.sink.clone();
            let sampling = ctx.sampling;
            let series = SeriesId::new(&self.id, "value");
            self.task = Some(ctx.runtime.spawn(async move {
                let mut ticker = tokio::time::interval(sampling);
                loop {
                    ticker.tick().await;
                    sink.push(
                        series.clone(),
                        Sample {
                            timestamp: SystemTime::now(),
                            value: SampleValue::Number(1.0),
                        },
                    );
                }
            }));
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AdapterError> {
            if let Some(task) = self.task.take() {
                task.abort();
            }
            self.status = AdapterStatus::Stopped;
            Ok(())
        }

        fn status(&self) -> AdapterStatus {
            self.status.clone()
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn config_with(id: &str, enabled: bool, sampling: Option<u64>) -> PanoConfig {
        let mut cfg = AdapterConfig::default_enabled();
        cfg.enabled = enabled;
        cfg.sampling = sampling;
        let mut adapters = HashMap::new();
        adapters.insert(id.to_string(), cfg);
        PanoConfig {
            schema_version: 1,
            core: Default::default(),
            adapters,
        }
    }

    #[test]
    fn unknown_adapter_in_config_rejected() {
        let reg = Registry::new();
        let cfg = config_with("ghost.adapter", true, None);
        let rt = runtime();
        let err = Lifecycle::new(reg, cfg, rt.handle().clone())
            .err()
            .expect("应失败");
        assert!(matches!(
            err,
            CoreError::UnknownAdapter(id) if id.as_str() == "ghost.adapter"
        ));
    }

    #[test]
    fn start_stop_produces_samples() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeAdapter::new("example.fake")))
            .unwrap();
        let rt = runtime();
        let mut lc = Lifecycle::new(
            reg,
            config_with("example.fake", true, Some(20)),
            rt.handle().clone(),
        )
        .unwrap();

        lc.start_all();
        assert!(lc.status_of(&AdapterId::new("example.fake")).is_running());

        std::thread::sleep(Duration::from_millis(90));
        let series = SeriesId::new(&AdapterId::new("example.fake"), "value");
        let samples = lc.store().history(&series, 100);
        assert!(
            samples.len() >= 3,
            "应产出至少 3 个样本，实际 {}",
            samples.len()
        );

        lc.stop_all();
        assert!(!lc.status_of(&AdapterId::new("example.fake")).is_running());
        let before = lc.store().history(&series, 100).len();
        std::thread::sleep(Duration::from_millis(60));
        let after = lc.store().history(&series, 100).len();
        assert_eq!(before, after, "stop 后不应再产出样本");
    }

    #[test]
    fn disabled_adapter_not_started() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeAdapter::new("example.fake")))
            .unwrap();
        let rt = runtime();
        let mut lc = Lifecycle::new(
            reg,
            config_with("example.fake", false, None),
            rt.handle().clone(),
        )
        .unwrap();
        lc.start_all();
        assert_eq!(
            lc.status_of(&AdapterId::new("example.fake")),
            AdapterStatus::Disabled
        );
    }

    #[test]
    fn apply_config_restarts_adapter() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeAdapter::new("example.fake")))
            .unwrap();
        let rt = runtime();
        let mut lc = Lifecycle::new(
            reg,
            config_with("example.fake", true, Some(20)),
            rt.handle().clone(),
        )
        .unwrap();
        lc.start_all();
        assert!(lc.status_of(&AdapterId::new("example.fake")).is_running());

        // 停用 → 热生效
        let mut disabled = AdapterConfig::default_enabled();
        disabled.enabled = false;
        lc.apply_adapter_config(&AdapterId::new("example.fake"), disabled)
            .unwrap();
        assert_eq!(
            lc.status_of(&AdapterId::new("example.fake")),
            AdapterStatus::Disabled
        );

        // 重新启用 → 热生效
        let enabled = AdapterConfig::default_enabled();
        lc.apply_adapter_config(&AdapterId::new("example.fake"), enabled)
            .unwrap();
        assert!(lc.status_of(&AdapterId::new("example.fake")).is_running());
    }

    #[test]
    fn uispec_validation() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeAdapter::new("example.fake")))
            .unwrap();
        let rt = runtime();
        let lc = Lifecycle::new(
            reg,
            config_with("example.fake", true, None),
            rt.handle().clone(),
        )
        .unwrap();

        let ok = UISpec::requires(vec![Capability::new(Capability::TIME_SERIES)]);
        assert!(lc.validate_uispec(&ok).is_ok());

        let bad = UISpec::requires(vec![Capability::new("SystemInfo")]);
        let err = lc.validate_uispec(&bad).unwrap_err();
        match err {
            CoreError::CapabilityUnsatisfied { missing } => {
                assert_eq!(missing.len(), 1);
                assert_eq!(missing[0].as_str(), "SystemInfo");
            }
            other => panic!("期望 CapabilityUnsatisfied，实际 {other:?}"),
        }
    }
}
