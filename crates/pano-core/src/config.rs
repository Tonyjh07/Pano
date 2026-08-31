//! 配置：`pano.toml` 的解析、校验与序列化。
//!
//! 结构（见 spec §7 / AGENTS §2.6）：
//! - `schema_version`：配置 schema 版本；
//! - `[core]`：全局默认（如默认采样周期）；
//! - `[adapters.<id>]`：固定字段 `enabled`、`sampling`，其余为该适配器自定义字段；
//! - `[window.<id>]`：布局持久化（位置 / 大小 / 所在显示器，由窗口服务读写）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::adapter::ConfigValue;
use crate::capability::{is_valid_adapter_id, is_valid_component_id};
use crate::error::CoreError;

/// 当前支持的配置 schema 版本。
pub const SCHEMA_VERSION: u32 = 1;

/// 默认采样周期（毫秒）。
pub const DEFAULT_SAMPLING_MS: u64 = 1000;

/// Pano 根配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanoConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub core: CoreConfig,
    #[serde(default)]
    pub adapters: HashMap<String, AdapterConfig>,
    /// 窗口布局持久化（`[window.<id>]` 段，架构 §13）。
    #[serde(default, rename = "window")]
    pub windows: HashMap<String, WindowLayout>,
}

/// 单个窗口的布局持久化段（`[window.<id>]`）。
///
/// `monitor` 为字符串序列化形式（如 `"primary"` / 自定义编号），
/// 与 [`crate::capability::WindowSpec`] 的 `monitor` 字段一致。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowLayout {
    /// 窗口位置（逻辑像素）；缺省 = 居中。
    pub position: Option<(f64, f64)>,
    /// 窗口大小（逻辑像素）；缺省 = WindowSpec 初始尺寸。
    pub size: Option<(f64, f64)>,
    /// 所在显示器（序列化标识）。
    pub monitor: Option<String>,
}

/// `[core]` 全局配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    /// 默认采样周期（毫秒）。
    #[serde(default = "default_sampling")]
    pub sampling_ms: u64,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            sampling_ms: DEFAULT_SAMPLING_MS,
        }
    }
}

/// 单个适配器的配置段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// 是否启用。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 采样周期（毫秒）；缺省时用 `[core].sampling_ms`。
    pub sampling: Option<u64>,
    /// 自定义字段（`enabled` / `sampling` 之外的所有键）。
    #[serde(flatten)]
    pub custom: HashMap<String, toml::Value>,
}

impl AdapterConfig {
    /// 默认配置（启用，采样周期沿用 core 默认）。
    pub fn default_enabled() -> Self {
        Self {
            enabled: true,
            sampling: None,
            custom: HashMap::new(),
        }
    }
}

fn default_sampling() -> u64 {
    DEFAULT_SAMPLING_MS
}

fn default_enabled() -> bool {
    true
}

impl PanoConfig {
    /// 解析并校验配置文本。
    pub fn parse(text: &str) -> Result<Self, CoreError> {
        let config: Self = toml::from_str(text)
            .map_err(|e| CoreError::Config(format!("pano.toml 解析失败：{e}")))?;
        config.validate()?;
        Ok(config)
    }

    /// 校验配置：
    /// - `schema_version` 匹配；
    /// - 采样周期必须大于 0；
    /// - 适配器 id 满足 `<域>.<名称>` 格式（全小写 ASCII、连字符分隔）。
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(CoreError::Config(format!(
                "schema_version 不匹配：期望 {SCHEMA_VERSION}，实际 {}",
                self.schema_version
            )));
        }
        if self.core.sampling_ms == 0 {
            return Err(CoreError::Config(
                "[core].sampling_ms 必须大于 0".to_string(),
            ));
        }
        for (id, cfg) in &self.adapters {
            if !is_valid_adapter_id(id) {
                return Err(CoreError::InvalidAdapterId(id.clone()));
            }
            if cfg.sampling == Some(0) {
                return Err(CoreError::Config(format!(
                    "适配器 {id} 的 sampling 必须大于 0"
                )));
            }
        }
        for id in self.windows.keys() {
            if !is_valid_component_id(id) {
                return Err(CoreError::Config(format!(
                    "[window.<id>] 的窗口 id 格式非法（应为全小写 ASCII、连字符分隔）：{id}"
                )));
            }
        }
        Ok(())
    }

    /// 序列化为 toml 文本（热重载写回文件用）。
    pub fn to_toml(&self) -> Result<String, CoreError> {
        toml::to_string(self).map_err(|e| CoreError::Config(format!("配置序列化失败：{e}")))
    }

    /// 取某适配器的采样周期（毫秒）：适配器配置优先，缺省用 core 默认。
    pub fn sampling_ms_of(&self, id: &str) -> u64 {
        self.adapters
            .get(id)
            .and_then(|c| c.sampling)
            .unwrap_or(self.core.sampling_ms)
    }

    /// 某适配器是否启用（未配置的适配器视为未启用）。
    pub fn is_enabled(&self, id: &str) -> bool {
        self.adapters.get(id).map(|c| c.enabled).unwrap_or(false)
    }

    /// 把某适配器配置中的自定义字段转为 [`ConfigValue`] 映射。
    pub fn custom_config(&self, id: &str) -> Result<HashMap<String, ConfigValue>, CoreError> {
        let mut out = HashMap::new();
        if let Some(cfg) = self.adapters.get(id) {
            for (key, value) in &cfg.custom {
                out.insert(key.clone(), toml_value_to_config(value)?);
            }
        }
        Ok(out)
    }

    /// 读取某窗口的持久化布局（无记录返回 `None`）。
    pub fn window_layout(&self, id: &str) -> Option<&WindowLayout> {
        self.windows.get(id)
    }

    /// 写入（或移除）某窗口的持久化布局。
    pub fn set_window_layout(&mut self, id: &str, layout: Option<WindowLayout>) {
        match layout {
            Some(l) => {
                self.windows.insert(id.to_string(), l);
            }
            None => {
                self.windows.remove(id);
            }
        }
    }
}

/// `toml::Value` → [`ConfigValue`]；不支持的类型（数组 / 表）视为配置错误。
pub fn toml_value_to_config(value: &toml::Value) -> Result<ConfigValue, CoreError> {
    match value {
        toml::Value::Integer(i) => Ok(ConfigValue::Number(*i as f64)),
        toml::Value::Float(f) => Ok(ConfigValue::Number(*f)),
        toml::Value::Boolean(b) => Ok(ConfigValue::Bool(*b)),
        toml::Value::String(s) => Ok(ConfigValue::Text(s.clone())),
        other => Err(CoreError::Config(format!(
            "不支持的自定义配置值类型：{other:?}"
        ))),
    }
}

/// [`ConfigValue`] → `toml::Value`（写回 pano.toml 用）。
pub fn config_value_to_toml(value: &ConfigValue) -> toml::Value {
    match value {
        ConfigValue::Number(n) => toml::Value::Float(*n),
        ConfigValue::Bool(b) => toml::Value::Boolean(*b),
        ConfigValue::Text(s) => toml::Value::String(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config() {
        let text = r#"
schema_version = 1

[core]
sampling_ms = 500

[adapters."example.counter"]
enabled = true
sampling = 100
step = 2
label = "计数"
"#;
        let config = PanoConfig::parse(text).expect("解析成功");
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.core.sampling_ms, 500);
        assert!(config.is_enabled("example.counter"));
        assert_eq!(config.sampling_ms_of("example.counter"), 100);
        assert_eq!(config.sampling_ms_of("example.missing"), 500);
        let custom = config.custom_config("example.counter").unwrap();
        assert_eq!(custom.get("step"), Some(&ConfigValue::Number(2.0)));
        assert_eq!(
            custom.get("label"),
            Some(&ConfigValue::Text("计数".to_string()))
        );
    }

    #[test]
    fn reject_wrong_schema_version() {
        let text = "schema_version = 99\n";
        let err = PanoConfig::parse(text).unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
    }

    #[test]
    fn reject_zero_sampling() {
        let text = r#"
schema_version = 1
[core]
sampling_ms = 0
"#;
        let err = PanoConfig::parse(text).unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
    }

    #[test]
    fn reject_invalid_adapter_id() {
        let text = r#"
schema_version = 1
[adapters."badid"]
enabled = true
"#;
        let err = PanoConfig::parse(text).unwrap_err();
        assert!(matches!(err, CoreError::InvalidAdapterId(_)));
    }

    #[test]
    fn roundtrip_to_toml() {
        let text = r#"
schema_version = 1
[core]
sampling_ms = 300
[adapters."example.sine"]
enabled = true
sampling = 50
amplitude = 1.5
"#;
        let config = PanoConfig::parse(text).unwrap();
        let out = config.to_toml().unwrap();
        let reparsed = PanoConfig::parse(&out).unwrap();
        assert_eq!(reparsed.schema_version, 1);
        assert!(reparsed.is_enabled("example.sine"));
        assert_eq!(reparsed.sampling_ms_of("example.sine"), 50);
        assert_eq!(
            reparsed
                .custom_config("example.sine")
                .unwrap()
                .get("amplitude"),
            Some(&ConfigValue::Number(1.5))
        );
    }

    #[test]
    fn window_layout_segment_parses_and_roundtrips() {
        let text = r#"
schema_version = 1
[window."counter-chart"]
position = [120.0, 80.0]
size = [480.0, 320.0]
monitor = "primary"
"#;
        let config = PanoConfig::parse(text).unwrap();
        let layout = config.window_layout("counter-chart").expect("有布局");
        assert_eq!(layout.position, Some((120.0, 80.0)));
        assert_eq!(layout.size, Some((480.0, 320.0)));
        assert_eq!(layout.monitor.as_deref(), Some("primary"));

        // 写回 → 重新解析一致
        let mut edited = config.clone();
        edited.set_window_layout(
            "counter-chart",
            Some(WindowLayout {
                position: Some((1.0, 2.0)),
                size: None,
                monitor: None,
            }),
        );
        let reparsed = PanoConfig::parse(&edited.to_toml().unwrap()).unwrap();
        let layout = reparsed.window_layout("counter-chart").unwrap();
        assert_eq!(layout.position, Some((1.0, 2.0)));
        assert_eq!(layout.size, None);

        // 移除布局
        edited.set_window_layout("counter-chart", None);
        assert!(edited.window_layout("counter-chart").is_none());
    }

    #[test]
    fn reject_invalid_adapter_id_format() {
        let text = r#"
schema_version = 1
[adapters."Bad.Id"]
enabled = true
"#;
        let err = PanoConfig::parse(text).unwrap_err();
        assert!(matches!(err, CoreError::InvalidAdapterId(_)));
    }

    #[test]
    fn reject_invalid_window_id() {
        // 窗口 id 与组件 id 同规则（全小写 ASCII、连字符分隔）。
        let text = r#"
schema_version = 1
[window."Bad.Id"]
position = [1.0, 2.0]
"#;
        let err = PanoConfig::parse(text).unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
    }
}
