//! 命令层与前端之间的类型化 DTO（serde 序列化，前端 TS 类型对应）。
//!
//! 时间戳统一为 unix 毫秒（`u64`）；样本值用 `serde_json::Value` 表达
//! （数值 / 布尔 / 文本 / JSON 一网打尽，前端按类型分支渲染）。

use serde::Serialize;

use pano_core::adapter::{AdapterStatus, ConfigValue, FieldKind, Sample, SampleValue, SeriesId};

/// 适配器管理页的一行信息。
#[derive(Debug, Clone, Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// 能力标签列表（含 `RemoteSource` 标记）。
    pub capabilities: Vec<String>,
    /// 本适配器输出的全部 series（「分配适配器」页枚举可选指标）。
    pub series: Vec<String>,
    /// 状态（`AdapterStatus` 的 Display 文案）。
    pub status: String,
    /// 是否处于运行状态（前端徽标绿色判定）。
    pub running: bool,
    /// 是否启用（配置）。
    pub enabled: bool,
    /// 生效采样周期（毫秒）。
    pub sampling_ms: u64,
    /// 最后错误（Error 状态时）。
    pub last_error: Option<String>,
    /// 配置表单 schema（自定义字段）。
    pub schema: Vec<FieldInfo>,
}

/// 配置表单字段（由 `config_schema` 驱动渲染，前端不硬编码适配器参数）。
#[derive(Debug, Clone, Serialize)]
pub struct FieldInfo {
    pub key: String,
    pub label: String,
    /// "number" | "bool" | "text" | "choice"
    pub kind: String,
    /// 默认值（"number" | "bool" | "text"；choice 时为首选值）。
    pub default: serde_json::Value,
    /// choice 的可选值。
    pub choices: Vec<String>,
    pub help: Option<String>,
}

/// 样本事件 / 快照载荷（core → 前端）。
#[derive(Debug, Clone, Serialize)]
pub struct SampleEventDto {
    pub series: String,
    /// unix 毫秒时间戳。
    pub timestamp_ms: u64,
    pub value: serde_json::Value,
}

/// 适配器状态快照。
#[derive(Debug, Clone, Serialize)]
pub struct StatusDto {
    pub status: String,
    pub running: bool,
    pub last_error: Option<String>,
}

/// 显示器信息（窗口控制菜单展示）。
#[derive(Debug, Clone, Serialize)]
pub struct MonitorDto {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub position: (i32, i32),
    pub size: (u32, u32),
    pub scale_factor: f64,
}

/// 窗口管理页 / 托盘「窗口列表」的一行信息。
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfoDto {
    pub id: String,
    pub title: String,
    /// 绑定的 UI 组件类型 id（管理窗口为空串）。
    pub component: String,
    /// 该窗口展示的 series（由组件目录解析，管理窗口为空）。
    pub series: Vec<String>,
    /// 是否为固定管理窗口（不可销毁）。
    pub is_manager: bool,
    /// 当前是否可见。
    pub visible: bool,
}

/// UI 组件目录项（「窗口管理」页选组件用）。
#[derive(Debug, Clone, Serialize)]
pub struct ComponentInfo {
    pub id: String,
    pub name: String,
    /// 本组件固定消费的 series。
    pub series: Vec<String>,
    /// 是否可用（全部 series 所属适配器已注册）；未注册（feature 未编译）置灰不可选。
    pub available: bool,
}

/// 窗口当前内容（component + series；命令层按窗口 label 查询，ui.md §3.2）。
#[derive(Debug, Clone, Serialize)]
pub struct WindowContentDto {
    pub component: String,
    pub series: Vec<String>,
}

impl From<&pano_window::MonitorInfo> for MonitorDto {
    fn from(info: &pano_window::MonitorInfo) -> Self {
        Self {
            id: info.id.as_str().to_string(),
            name: info.name.clone(),
            is_primary: info.is_primary,
            position: info.position,
            size: info.size,
            scale_factor: info.scale_factor,
        }
    }
}

impl AdapterInfo {
    /// 由 lifecycle 视图数据构造。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        name: String,
        description: String,
        version: String,
        capabilities: Vec<String>,
        series: Vec<String>,
        status: &AdapterStatus,
        enabled: bool,
        sampling_ms: u64,
        schema: Vec<FieldInfo>,
    ) -> Self {
        let (running, last_error) = match status {
            AdapterStatus::Running => (true, None),
            AdapterStatus::Error { last_error } => (false, Some(last_error.clone())),
            _ => (false, None),
        };
        Self {
            id: id.to_string(),
            name,
            description,
            version,
            capabilities,
            series,
            status: status.to_string(),
            running,
            enabled,
            sampling_ms,
            last_error,
            schema,
        }
    }
}

impl From<&pano_core::adapter::ConfigField> for FieldInfo {
    fn from(field: &pano_core::adapter::ConfigField) -> Self {
        let (kind, choices) = field_kind_parts(&field.kind);
        Self {
            key: field.key.clone(),
            label: field.label.clone(),
            kind,
            default: config_value_to_json(&field.default),
            choices,
            help: field.help.clone(),
        }
    }
}

/// `ConfigValue` → JSON（默认值展示）。
pub fn config_value_to_json(value: &ConfigValue) -> serde_json::Value {
    match value {
        ConfigValue::Number(n) => serde_json::Value::from(*n),
        ConfigValue::Bool(b) => serde_json::Value::from(*b),
        ConfigValue::Text(t) => serde_json::Value::from(t.clone()),
    }
}

/// `FieldKind` → （前端 kind 字符串, choice 可选值）。
pub fn field_kind_parts(kind: &FieldKind) -> (String, Vec<String>) {
    match kind {
        FieldKind::Number => ("number".into(), Vec::new()),
        FieldKind::Bool => ("bool".into(), Vec::new()),
        FieldKind::Text => ("text".into(), Vec::new()),
        FieldKind::Choice(choices) => ("choice".into(), choices.clone()),
    }
}

impl SampleEventDto {
    /// 由 core 样本构造（时间戳 → unix 毫秒）。
    pub fn from_sample(series: &SeriesId, sample: &Sample) -> Self {
        Self {
            series: series.as_str().to_string(),
            timestamp_ms: sample
                .timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            value: sample_value_to_json(&sample.value),
        }
    }
}

impl StatusDto {
    /// 由适配器状态构造。
    pub fn from_status(status: &AdapterStatus) -> Self {
        let (running, last_error) = match status {
            AdapterStatus::Running => (true, None),
            AdapterStatus::Error { last_error } => (false, Some(last_error.clone())),
            _ => (false, None),
        };
        Self {
            status: status.to_string(),
            running,
            last_error,
        }
    }
}

/// 样本值 → JSON（前端按类型分支渲染）。
pub fn sample_value_to_json(value: &SampleValue) -> serde_json::Value {
    match value {
        SampleValue::Number(n) => serde_json::Value::from(*n),
        SampleValue::Bool(b) => serde_json::Value::from(*b),
        SampleValue::Text(t) => serde_json::Value::from(t.clone()),
        SampleValue::Json(j) => j.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pano_core::adapter::{
        AdapterId, ConfigField, ConfigValue, FieldKind, Sample, SampleValue, SeriesId,
    };
    use std::time::SystemTime;

    #[test]
    fn config_value_to_json_mappings() {
        assert_eq!(
            config_value_to_json(&ConfigValue::Number(2.5)),
            serde_json::json!(2.5)
        );
        assert_eq!(
            config_value_to_json(&ConfigValue::Bool(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            config_value_to_json(&ConfigValue::Text("x".into())),
            serde_json::json!("x")
        );
    }

    #[test]
    fn field_kind_parts_mappings() {
        assert_eq!(
            field_kind_parts(&FieldKind::Number),
            ("number".into(), Vec::new())
        );
        assert_eq!(
            field_kind_parts(&FieldKind::Bool),
            ("bool".into(), Vec::new())
        );
        assert_eq!(
            field_kind_parts(&FieldKind::Text),
            ("text".into(), Vec::new())
        );
        assert_eq!(
            field_kind_parts(&FieldKind::Choice(vec!["a".into(), "b".into()])),
            ("choice".into(), vec!["a".into(), "b".into()])
        );
    }

    #[test]
    fn field_info_from_config_field() {
        let field = ConfigField {
            key: "step".into(),
            label: "步长".into(),
            kind: FieldKind::Number,
            default: ConfigValue::Number(1.0),
            help: Some("必须 > 0".into()),
        };
        let info = FieldInfo::from(&field);
        assert_eq!(info.key, "step");
        assert_eq!(info.kind, "number");
        assert_eq!(info.default, serde_json::json!(1.0));
        assert_eq!(info.choices, Vec::<String>::new());
    }

    #[test]
    fn sample_event_dto_converts_timestamp_to_unix_ms() {
        let series = SeriesId::new(&AdapterId::new("example.counter"), "value");
        let sample = Sample {
            timestamp: SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1500),
            value: SampleValue::Number(3.5),
        };
        let dto = SampleEventDto::from_sample(&series, &sample);
        assert_eq!(dto.series, "example.counter.value");
        assert_eq!(dto.timestamp_ms, 1500);
        assert_eq!(dto.value, serde_json::json!(3.5));
    }

    #[test]
    fn sample_value_json_roundtrip() {
        let j = serde_json::json!({"a": [1, 2]});
        assert_eq!(sample_value_to_json(&SampleValue::Json(j.clone())), j);
        assert_eq!(
            sample_value_to_json(&SampleValue::Text("t".into())),
            serde_json::json!("t")
        );
    }

    #[test]
    fn adapter_info_status_mapping() {
        let info = AdapterInfo::new(
            "example.counter",
            "计数器".into(),
            String::new(),
            "0.1.0".into(),
            vec!["TimeSeries".into()],
            vec!["example.counter.value".into()],
            &AdapterStatus::Running,
            true,
            200,
            vec![],
        );
        assert!(info.running);
        assert_eq!(info.status, "运行中");
        assert_eq!(info.series, vec!["example.counter.value"]);
        assert!(info.last_error.is_none());

        let err = AdapterInfo::new(
            "x",
            String::new(),
            String::new(),
            String::new(),
            vec![],
            vec![],
            &AdapterStatus::Error {
                last_error: "boom".into(),
            },
            true,
            200,
            vec![],
        );
        assert!(!err.running);
        assert_eq!(err.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn status_dto_mapping() {
        let running = StatusDto::from_status(&AdapterStatus::Running);
        assert!(running.running && running.last_error.is_none());
        let err = StatusDto::from_status(&AdapterStatus::Error {
            last_error: "e".into(),
        });
        assert!(!err.running && err.last_error.as_deref() == Some("e"));
        let stopped = StatusDto::from_status(&AdapterStatus::Stopped);
        assert!(!stopped.running && stopped.last_error.is_none());
    }

    #[test]
    fn monitor_dto_mapping() {
        let info = pano_window::MonitorInfo {
            id: pano_window::MonitorId::primary(),
            name: "主".into(),
            is_primary: true,
            position: (0, 0),
            size: (1920, 1080),
            scale_factor: 1.5,
        };
        let dto = MonitorDto::from(&info);
        assert_eq!(dto.id, "primary");
        assert!(dto.is_primary);
        assert_eq!(dto.size, (1920, 1080));
        assert_eq!(dto.scale_factor, 1.5);
    }
}
