//! 生命周期：适配器启停、状态查询、能力校验、配置应用（热生效）。
//!
//! 关闭顺序（架构 §5）：先停适配器任务，再退 UI。

use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Handle;

use crate::adapter::{AdapterContext, AdapterId, AdapterMeta, AdapterStatus, SampleSink, SeriesId};
use crate::capability::UISpec;
use crate::config::{AdapterConfig, PanoConfig};
use crate::error::CoreError;
use crate::http::HttpClient;
use crate::registry::Registry;
use crate::sample_store::SampleStore;

/// 适配器生命周期管理器：编排注册表、样本存储与配置。
pub struct Lifecycle {
    registry: Registry,
    store: Arc<SampleStore>,
    config: PanoConfig,
    runtime: Handle,
    /// 远程数据源注入（M1.2 R3，架构 §14）：pano-app 按 feature 装配；
    /// 无远程适配器时为 `None`。
    http: Option<Arc<dyn HttpClient>>,
}

impl Lifecycle {
    /// 由注册表与配置构造；配置引用了未注册的适配器 → [`CoreError::UnknownAdapter`]。
    pub fn new(registry: Registry, config: PanoConfig, runtime: Handle) -> Result<Self, CoreError> {
        Self::with_http(registry, config, runtime, None)
    }

    /// 同 [`Lifecycle::new`]，并注入远程数据源客户端（架构 §14）。
    pub fn with_http(
        registry: Registry,
        config: PanoConfig,
        runtime: Handle,
        http: Option<Arc<dyn HttpClient>>,
    ) -> Result<Self, CoreError> {
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
            http,
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

    /// 当前配置（可变；pano-app 写回 pano.toml 前做段级合并，如窗口布局）。
    pub fn config_mut(&mut self) -> &mut PanoConfig {
        &mut self.config
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
            http: self.http.clone(),
        };

        let adapter = self
            .registry
            .get_mut(id)
            .ok_or_else(|| CoreError::UnknownAdapter(id.clone()))?;
        adapter.start(ctx).map_err(CoreError::Adapter)
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
        adapter.stop().map_err(CoreError::Adapter)
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

    /// 适配器输出的全部 series（管理界面「分配适配器」枚举可选指标）。
    pub fn series_of(&self, id: &AdapterId) -> Option<Vec<SeriesId>> {
        self.registry.get(id).map(|a| a.series())
    }

    /// 适配器自定义配置 schema（供管理页渲染表单）。
    pub fn config_schema_of(&self, id: &AdapterId) -> Option<crate::adapter::ConfigSchema> {
        self.registry.get(id).map(|a| a.config_schema())
    }

    /// 能力校验（架构 §7）：`UISpec.requires ⊆ 已启用适配器的能力并集`。
    ///
    /// 同时校验 `UISpec` 自身声明（组件 id 格式 / 唯一性 / series 引用合法）。
    pub fn validate_uispec(&self, spec: &UISpec) -> Result<(), CoreError> {
        spec.validate()?;
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
    /// **失败回滚**（M1 审查待办）：新配置启动失败时恢复旧配置，
    /// 并尽力恢复原运行状态，保证内存配置与运行状态一致。
    /// 调用方（app）负责把新配置写回 `pano.toml`；**收到 `Err` 时不得写回**。
    pub fn apply_adapter_config(
        &mut self,
        id: &AdapterId,
        new_cfg: AdapterConfig,
    ) -> Result<(), CoreError> {
        if self.registry.get(id).is_none() {
            return Err(CoreError::UnknownAdapter(id.clone()));
        }
        if new_cfg.sampling == Some(0) {
            return Err(CoreError::Config(format!(
                "适配器 {id} 的 sampling 必须大于 0"
            )));
        }
        let running = matches!(
            self.status_of(id),
            AdapterStatus::Running | AdapterStatus::Starting | AdapterStatus::Error { .. }
        );
        if running {
            self.stop(id)?;
        }
        let old_cfg = self.config.adapters.get(id.as_str()).cloned();
        self.config
            .adapters
            .insert(id.as_str().to_string(), new_cfg.clone());
        if new_cfg.enabled
            && let Err(e) = self.start(id)
        {
            // 回滚配置与运行状态。
            match old_cfg {
                Some(old) => {
                    self.config.adapters.insert(id.as_str().to_string(), old);
                }
                None => {
                    self.config.adapters.remove(id.as_str());
                }
            }
            if running && let Err(restore_err) = self.start(id) {
                tracing::warn!(
                    target: "pano::core",
                    adapter = %id,
                    error = %restore_err,
                    "回滚后恢复运行失败，适配器保持停止（配置已回滚为旧值）"
                );
            }
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{
        Adapter, AdapterError, AdapterMeta, ConfigSchema, ConfigValue, Sample, SampleValue,
        SeriesId,
    };
    use crate::capability::Capability;
    use std::collections::HashMap;

    struct FakeAdapter {
        id: AdapterId,
        status: AdapterStatus,
        task: Option<tokio::task::JoinHandle<()>>,
        runtime: Option<tokio::runtime::Handle>,
    }

    impl FakeAdapter {
        fn new(id: &str) -> Self {
            Self {
                id: AdapterId::new(id),
                status: AdapterStatus::Stopped,
                task: None,
                runtime: None,
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

        fn series(&self) -> Vec<SeriesId> {
            vec![SeriesId::new(&self.id, "value")]
        }

        fn config_schema(&self) -> ConfigSchema {
            ConfigSchema::empty()
        }

        fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError> {
            // 测试钩子：自定义配置含 fail=true 时拒绝启动（经公开配置路径触发）。
            if ctx.config.get("fail") == Some(&ConfigValue::Bool(true)) {
                return Err(AdapterError::Config("故意失败".into()));
            }
            self.status = AdapterStatus::Running;
            let sink = ctx.sink.clone();
            let sampling = ctx.sampling;
            let series = SeriesId::new(&self.id, "value");
            self.runtime = Some(ctx.runtime.clone());
            self.task = Some(ctx.runtime.spawn(async move {
                let mut ticker = tokio::time::interval(sampling);
                loop {
                    ticker.tick().await;
                    sink.push(
                        series.clone(),
                        Sample {
                            timestamp: crate::adapter::now(),
                            value: SampleValue::Number(1.0),
                        },
                    );
                }
            }));
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AdapterError> {
            // 与各适配器一致：非运行时线程阻塞等待（消除 abort-only 竞态）；
            // 运行时内（async 命令路径）仅 abort，避免 block_on panic。
            if let Some(task) = self.task.take() {
                task.abort();
                if tokio::runtime::Handle::try_current().is_err()
                    && let Some(rt) = &self.runtime
                {
                    let _ = rt.block_on(task);
                }
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
            windows: HashMap::new(),
            ui: None,
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

    #[test]
    fn uispec_rejects_invalid_component_declarations() {
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

        use crate::capability::{ComponentSpec, WindowSpec};

        // 重复组件 id
        let dup = UISpec {
            requires: vec![],
            components: vec![
                ComponentSpec {
                    id: "chart-a".into(),
                    name: "图表A".into(),
                    series: vec![SeriesId::new(&AdapterId::new("example.fake"), "value")],
                    window: WindowSpec::default(),
                },
                ComponentSpec {
                    id: "chart-a".into(),
                    name: "图表A".into(),
                    series: vec![SeriesId::new(&AdapterId::new("example.fake"), "value")],
                    window: WindowSpec::default(),
                },
            ],
        };
        assert!(lc.validate_uispec(&dup).is_err());

        // 非法组件 id
        let bad_id = UISpec {
            requires: vec![],
            components: vec![ComponentSpec {
                id: "Bad_Id".into(),
                name: "图表".into(),
                series: vec![],
                window: WindowSpec::default(),
            }],
        };
        assert!(lc.validate_uispec(&bad_id).is_err());

        // 合法组件声明（含无 series 的空组件也允许，建窗时再过滤数据源）
        let ok = UISpec {
            requires: vec![],
            components: vec![ComponentSpec {
                id: "chart-b".into(),
                name: "图表B".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.fake"), "value")],
                window: WindowSpec {
                    title: "测试".into(),
                    size: (480.0, 320.0),
                    ..WindowSpec::default()
                },
            }],
        };
        assert!(lc.validate_uispec(&ok).is_ok());
    }

    #[test]
    fn apply_config_rejects_zero_sampling() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeAdapter::new("example.fake")))
            .unwrap();
        let rt = runtime();
        let mut lc = Lifecycle::new(
            reg,
            config_with("example.fake", true, None),
            rt.handle().clone(),
        )
        .unwrap();

        let mut bad = AdapterConfig::default_enabled();
        bad.sampling = Some(0);
        let err = lc
            .apply_adapter_config(&AdapterId::new("example.fake"), bad)
            .unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
        // 未触碰运行状态：适配器保持停止（从未启动过），配置保持原样。
        assert_eq!(
            lc.status_of(&AdapterId::new("example.fake")),
            AdapterStatus::Stopped
        );
    }

    #[test]
    fn apply_config_rolls_back_on_start_failure() {
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

        // 使适配器后续 start 失败（经自定义配置 fail=true 触发）
        let mut new_cfg = AdapterConfig::default_enabled();
        new_cfg.sampling = Some(5);
        new_cfg
            .custom
            .insert("fail".to_string(), toml::Value::Boolean(true));
        let err = lc
            .apply_adapter_config(&AdapterId::new("example.fake"), new_cfg)
            .unwrap_err();
        assert!(matches!(err, CoreError::Adapter(AdapterError::Config(_))));

        // 内存配置回滚为旧值，且原运行状态已尽力恢复（旧配置启动成功）
        assert_eq!(
            lc.config().sampling_ms_of("example.fake"),
            20,
            "配置应回滚为旧值"
        );
        assert!(
            lc.status_of(&AdapterId::new("example.fake")).is_running(),
            "回滚后应恢复原运行状态"
        );
        lc.stop_all();
    }
}
