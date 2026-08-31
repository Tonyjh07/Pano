//! 适配器契约：`Adapter` trait 及其关联类型。
//!
//! 本模块只定义契约，不包含任何具体适配器实现；具体适配器在
//! `pano-adapters` 中按 feature 编译。

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::runtime::Handle;

use crate::capability::Capability;
use crate::http::HttpClient;

/// 统一时钟源（架构 §4）：所有适配器样本时间戳一律经此获取，
/// 保证采样时间戳与曲线对齐（M1 审查待办）。
pub fn now() -> SystemTime {
    SystemTime::now()
}

/// 适配器全局唯一 id，格式 `<域>.<名称>`（全小写 ASCII、连字符分隔）。
///
/// 示例：`example.counter`、`sys.cpu`。
///
/// 格式校验见 [`crate::capability::is_valid_adapter_id`]（注册与配置解析处复用）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AdapterId(String);

impl AdapterId {
    /// 构造 id；调用方保证格式 `<域>.<名称>`。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 原始字符串视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AdapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for AdapterId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// 一条指标序列 id，格式 `<adapter_id>.<指标>`。
///
/// 示例：`example.counter.value`。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeriesId(String);

impl SeriesId {
    /// 由适配器 id 与指标名构造。
    pub fn new(adapter: &AdapterId, metric: &str) -> Self {
        Self(format!("{adapter}.{metric}"))
    }

    /// 由完整 series id 字符串构造（前端 / 配置传入，调用方保证
    /// `<adapter_id>.<指标>` 格式）。
    pub fn parse(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 该 series 所属的适配器 id（`<域>.<名称>` 前缀，取最后一个 `.` 之前的部分）。
    pub fn adapter_id(&self) -> AdapterId {
        let prefix = self
            .0
            .rsplit_once('.')
            .map(|(adapter, _)| adapter)
            .unwrap_or(&self.0);
        AdapterId::new(prefix)
    }

    /// 原始字符串视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SeriesId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 适配器元信息，供管理界面展示。
#[derive(Debug, Clone)]
pub struct AdapterMeta {
    pub id: AdapterId,
    pub name: String,
    pub description: String,
    pub version: String,
}

/// 适配器运行状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterStatus {
    /// 配置中未启用。
    Disabled,
    /// 已注册但未启动（或已干净停止）。
    Stopped,
    /// 启动中。
    Starting,
    /// 运行中，正常产出样本。
    Running,
    /// 运行期故障（退避重试中或已放弃）。
    Error { last_error: String },
}

impl AdapterStatus {
    /// 是否处于运行状态。
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

impl fmt::Display for AdapterStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Disabled => "已禁用",
            Self::Stopped => "已停止",
            Self::Starting => "启动中",
            Self::Running => "运行中",
            Self::Error { .. } => "错误",
        };
        f.write_str(label)
    }
}

/// 一个样本：时间戳 + 值。时间戳由 core 统一时钟源提供。
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub timestamp: SystemTime,
    pub value: SampleValue,
}

/// 样本值：数值 / 布尔 / 文本 / JSON。
#[derive(Debug, Clone, PartialEq)]
pub enum SampleValue {
    Number(f64),
    Bool(bool),
    Text(String),
    Json(serde_json::Value),
}

impl fmt::Display for SampleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Text(v) => f.write_str(v),
            Self::Json(v) => write!(f, "{v}"),
        }
    }
}

/// 适配器运行期错误。运行期错误不得崩溃进程：
/// core 将其转为 [`AdapterStatus::Error`] 并记录日志（退避重试，见架构 §9）。
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// 适配器当前不可用（如底层资源缺失）。
    #[error("适配器不可用：{0}")]
    NotAvailable(String),

    /// 当前平台不支持（如某指标在当前 OS 上不存在）。
    #[error("当前平台不支持：{0}")]
    Unsupported(String),

    /// 操作超时。
    #[error("操作超时：{0}")]
    Timeout(String),

    /// 配置无效。
    #[error("配置无效：{0}")]
    Config(String),

    /// IO 错误。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
}

/// 配置值：管理界面表单与适配器自定义配置的取值。
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Number(f64),
    Bool(bool),
    Text(String),
}

impl fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Text(v) => f.write_str(v),
        }
    }
}

/// 表单字段类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Number,
    Bool,
    Text,
    /// 单选，列出可选值。
    Choice(Vec<String>),
}

/// 配置表单字段。
///
/// 仅描述**自定义字段**；`enabled` / `sampling` 为 core 固定字段，不进 schema。
#[derive(Debug, Clone)]
pub struct ConfigField {
    /// 配置键（pano.toml 中的键名）。
    pub key: String,
    /// 界面标签。
    pub label: String,
    /// 字段类型。
    pub kind: FieldKind,
    /// 默认值（与 `kind` 对应）。
    pub default: ConfigValue,
    /// 帮助文案。
    pub help: Option<String>,
}

/// 适配器自定义配置的 schema，供管理界面渲染表单。
#[derive(Debug, Clone, Default)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
}

impl ConfigSchema {
    /// 无自定义字段的 schema。
    pub fn empty() -> Self {
        Self::default()
    }
}

/// 适配器持有的样本推送句柄（可 clone、多线程安全）。
#[derive(Clone)]
pub struct SampleSink {
    inner: Arc<dyn Fn(SeriesId, Sample) + Send + Sync>,
}

impl SampleSink {
    /// 由 core 构造：闭包负责写入对应 series 的环形缓冲。
    pub fn new<F>(push: F) -> Self
    where
        F: Fn(SeriesId, Sample) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(push),
        }
    }

    /// 推送一个样本到指定 series。
    pub fn push(&self, series: SeriesId, sample: Sample) {
        (self.inner)(series, sample);
    }
}

impl fmt::Debug for SampleSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SampleSink").finish_non_exhaustive()
    }
}

/// 适配器启动时由 core 提供的上下文。
#[derive(Clone)]
pub struct AdapterContext {
    /// 样本推送句柄。
    pub sink: SampleSink,
    /// 采样周期（由配置决定）。
    pub sampling: Duration,
    /// tokio 运行时句柄：适配器在 `start` 内据此自建采集任务。
    pub runtime: Handle,
    /// 该适配器的自定义配置（键 → 值，由 pano.toml 解析而来）。
    pub config: HashMap<String, ConfigValue>,
    /// 远程数据源注入点（M1.2 预留，R3，见架构 §14）：
    /// 由 pano-app 按 feature 装配（reqwest 实现）；本地适配器为 `None`。
    pub http: Option<Arc<dyn HttpClient>>,
}

impl fmt::Debug for AdapterContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `dyn HttpClient` 不实现 Debug，仅打印是否注入。
        f.debug_struct("AdapterContext")
            .field("sink", &self.sink)
            .field("sampling", &self.sampling)
            .field("runtime", &self.runtime)
            .field("config", &self.config)
            .field("http", &self.http.is_some())
            .finish()
    }
}

/// 适配器契约：一个信息源。
///
/// 设计要点：
/// - 适配器**自持状态**，core 不假设其内部实现（单任务或多任务均可）；
/// - `start` 返回即视为已开始，适配器负责在 tokio 上自建任务循环；
/// - `stop` 必须干净退出（任务 join、资源释放）。
///
/// `stop` 语义（一致性测试基座依赖）：
/// - `stop` 返回后**不得再产出样本**——实现应在返回前等待任务真正结束
///   （如 `abort` 后 `block_on` join）；
/// - `stop` 会在调用线程**阻塞**至任务结束：调用方不得处于 tokio 异步上下文，
///   且运行时须存活（生命周期保证：`stop_all` 先于 runtime drop）。
pub trait Adapter: Send + Sync {
    /// 元信息（含唯一 id）。
    fn meta(&self) -> AdapterMeta;

    /// 提供的能力标签。
    fn capabilities(&self) -> Vec<Capability>;

    /// 自定义配置的 schema，供管理界面渲染表单。
    fn config_schema(&self) -> ConfigSchema;

    /// 启动适配器。
    fn start(&mut self, ctx: AdapterContext) -> Result<(), AdapterError>;

    /// 停止适配器；必须干净退出。
    fn stop(&mut self) -> Result<(), AdapterError>;

    /// 当前状态。
    fn status(&self) -> AdapterStatus;
}
