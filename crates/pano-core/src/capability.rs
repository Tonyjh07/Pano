//! 能力标签与 UI 声明（UISpec）：UI 按能力声明需求，core 按能力校验。
//!
//! `UISpec` / `ComponentSpec` / `WindowSpec` 均为**纯数据声明**（架构 §7）：
//! `pano-window` 允许引用这些类型，但不依赖 core 的任何运行逻辑。

use std::fmt;

use crate::adapter::SeriesId;
use crate::error::CoreError;

/// 能力标签（上驼峰语义化），如 `TimeSeries`、`SystemInfo`。
///
/// 采用开放字符串而非封闭枚举：新增能力只需声明新标签，便于未来插件化扩展。
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Capability(String);

impl Capability {
    /// 时间序列能力：适配器按周期向环形缓冲推送样本。
    pub const TIME_SERIES: &'static str = "TimeSeries";

    /// 系统信息能力（M2 起使用）。
    pub const SYSTEM_INFO: &'static str = "SystemInfo";

    /// 远程数据源能力（M1.2 预留，R3）：适配器经 HTTP / WebSocket 获取数据。
    pub const REMOTE_SOURCE: &'static str = "RemoteSource";

    /// 构造能力标签。
    pub fn new(label: impl Into<String>) -> Self {
        Self(label.into())
    }

    /// 原始字符串视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug 输出裸字符串，便于错误消息中呈现，如 `["TimeSeries"]`。
        f.write_str(&self.0)
    }
}

impl From<&'static str> for Capability {
    fn from(label: &'static str) -> Self {
        Self::new(label)
    }
}

impl From<String> for Capability {
    fn from(label: String) -> Self {
        Self::new(label)
    }
}

/// UI 声明所需能力与组件清单（见架构 §7）。
///
/// `pano-app` 启动时校验：
/// - `requires ⊆ 已启用适配器的能力并集`，不满足则明确报错退出；
/// - `components` 逐个建监控组件窗口（仅当组件的 series 存在数据源）。
#[derive(Debug, Clone, Default)]
pub struct UISpec {
    /// 本 UI 必须满足的能力。
    pub requires: Vec<Capability>,
    /// 监控组件及其窗口需求（管理窗口不在此列，由 pano-app 固定创建）。
    pub components: Vec<ComponentSpec>,
}

impl UISpec {
    /// 声明需要的能力（无组件）。
    pub fn requires(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            requires: capabilities.into_iter().collect(),
            components: Vec::new(),
        }
    }

    /// 校验声明：
    /// - 组件 id 满足全小写 ASCII、连字符分隔且全局唯一；
    /// - 组件显示名非空；
    /// - 组件消费的 series 格式合法（适配器 id 前缀 + 非空指标名）。
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut seen = std::collections::HashSet::new();
        for comp in &self.components {
            if !is_valid_component_id(&comp.id) {
                return Err(CoreError::Config(format!(
                    "组件 id 格式非法（应为全小写 ASCII、连字符分隔）：{}",
                    comp.id
                )));
            }
            if !seen.insert(comp.id.clone()) {
                return Err(CoreError::Config(format!("组件 id 重复：{}", comp.id)));
            }
            if comp.name.trim().is_empty() {
                return Err(CoreError::Config(format!(
                    "组件 {} 的显示名不能为空",
                    comp.id
                )));
            }
            for series in &comp.series {
                if !is_valid_series_id(series) {
                    return Err(CoreError::Config(format!(
                        "组件 {} 的 series 非法：{series}",
                        comp.id
                    )));
                }
            }
        }
        Ok(())
    }
}

/// 单个监控组件的声明：内容消费的指标 + 窗口需求（架构 §7，R1）。
///
/// M2.2：`UISpec.components` 即**组件目录**，每个 `ComponentSpec` 描述一种
/// 「窗口内容形态」（组件类型）；监控窗口为其实例，绑定 `component`（组件 id），
/// series 由本声明解析（窗口不再直接存 series）。
#[derive(Debug, Clone)]
pub struct ComponentSpec {
    /// 组件类型 id：全小写 ASCII、连字符分隔，全局唯一（如 `sys-cpu`）。
    pub id: String,
    /// 组件显示名（如 "CPU 使用率"）。
    pub name: String,
    /// 本组件固定消费的指标序列。
    pub series: Vec<SeriesId>,
    /// 默认窗口需求声明（纯数据；新建 / 播种窗口的默认规格）。
    pub window: WindowSpec,
}

/// 窗口需求声明（纯数据，架构 §7 / §13）。
///
/// `monitor` 为**字符串序列化形式**（如 `"primary"` / 自定义编号）；
/// 由 pano-ui / pano-window 侧解析为 `MonitorId`（定义在 pano-window，core 不引用）。
#[derive(Debug, Clone)]
pub struct WindowSpec {
    /// 窗口标题。
    pub title: String,
    /// 初始宽高（逻辑像素）。
    pub size: (f64, f64),
    /// 初始位置（缺省 = 居中 / 布局持久化恢复）。
    pub position: Option<(f64, f64)>,
    /// 初始置顶。
    pub always_on_top: bool,
    /// 初始全屏。
    pub fullscreen: bool,
    /// 显示器偏好（序列化标识；缺省 = 主显示器 / 上次所在）。
    pub monitor: Option<String>,
    /// 有无系统边框/标题栏（M2.4）：`false` = 无边框窗口，内容区拖拽移动
    /// （前端 `data-pano-drag` 区域 + `startDragging`）。窗口级可经
    /// `[window.<id>].decorations` 覆盖（`WindowLayout.decorations`）。
    pub decorations: bool,
}

impl Default for WindowSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            size: (800.0, 600.0),
            position: None,
            always_on_top: false,
            fullscreen: false,
            monitor: None,
            decorations: true,
        }
    }
}

/// 校验组件 id 格式（全小写 ASCII、连字符分隔）。
pub fn is_valid_component_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 校验适配器 id 格式（`<域>.<名称>`，全小写 ASCII、连字符分隔）。
///
/// 注册与配置解析两处复用（M1 审查待办）。
pub fn is_valid_adapter_id(id: &str) -> bool {
    let Some((domain, name)) = id.split_once('.') else {
        return false;
    };
    !domain.is_empty()
        && !name.is_empty()
        && !name.contains('.')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

/// 校验 series id 的适配器前缀（供 `UISpec::validate` 等使用）。
pub fn is_valid_series_id(series: &SeriesId) -> bool {
    is_valid_adapter_id(series.adapter_id().as_str())
        && series
            .as_str()
            .rsplit_once('.')
            .is_some_and(|(_, metric)| !metric.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{AdapterId, SeriesId};

    #[test]
    fn adapter_id_validation() {
        assert!(is_valid_adapter_id("example.counter"));
        assert!(is_valid_adapter_id("sys.cpu"));
        assert!(is_valid_adapter_id("a.b"));
        assert!(!is_valid_adapter_id(""));
        assert!(!is_valid_adapter_id("nodot"));
        assert!(!is_valid_adapter_id(".x"));
        assert!(!is_valid_adapter_id("x."));
        assert!(!is_valid_adapter_id("Bad.Id"));
        assert!(!is_valid_adapter_id("example.counter.value")); // 指标名不该出现在适配器 id 中
        assert!(!is_valid_adapter_id("a..b"));
    }

    #[test]
    fn component_id_validation() {
        assert!(is_valid_component_id("counter-chart"));
        assert!(is_valid_component_id("a1"));
        assert!(!is_valid_component_id(""));
        assert!(!is_valid_component_id("Bad_Id"));
        assert!(!is_valid_component_id("a.b")); // 组件 id 不用点分隔
    }

    #[test]
    fn series_id_adapter_prefix() {
        let series = SeriesId::new(&AdapterId::new("example.counter"), "value");
        assert_eq!(series.adapter_id().as_str(), "example.counter");
        assert!(is_valid_series_id(&series));
    }

    #[test]
    fn uispec_validation_rules() {
        let good = UISpec {
            requires: vec![Capability::new(Capability::TIME_SERIES)],
            components: vec![ComponentSpec {
                id: "counter-chart".into(),
                name: "计数器".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.counter"), "value")],
                window: WindowSpec {
                    title: "计数器".into(),
                    size: (480.0, 320.0),
                    always_on_top: true,
                    ..WindowSpec::default()
                },
            }],
        };
        assert!(good.validate().is_ok());

        // 重复组件 id
        let dup = {
            let mut g = good.clone();
            g.components.push(g.components[0].clone());
            g
        };
        assert!(dup.validate().is_err());

        // 非法组件 id
        let bad_id = {
            let mut g = good;
            g.components[0].id = "Bad".into();
            g
        };
        assert!(bad_id.validate().is_err());
    }

    #[test]
    fn window_spec_defaults() {
        let spec = WindowSpec::default();
        assert_eq!(spec.size, (800.0, 600.0));
        assert!(!spec.always_on_top && !spec.fullscreen);
        assert!(spec.position.is_none() && spec.monitor.is_none());
        // M2.4：默认有边框（decorations = true），无边框由组件声明 / 窗口设置显式开启
        assert!(spec.decorations);
    }

    #[test]
    fn window_spec_can_declare_frameless() {
        // sys-dashboard 等专用组件可声明无边框（M2.4，小屏仪表盘窗口）
        let spec = WindowSpec {
            decorations: false,
            ..WindowSpec::default()
        };
        assert!(!spec.decorations);
    }

    #[test]
    fn uispec_rejects_empty_metric_series() {
        // series 形如 `example.counter.`（空指标名）必须被拒。
        let bad = UISpec {
            requires: vec![],
            components: vec![ComponentSpec {
                id: "chart-a".into(),
                name: "图表".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.counter"), "")],
                window: WindowSpec::default(),
            }],
        };
        assert!(bad.validate().is_err());

        // 显示名为空必须被拒
        let no_name = UISpec {
            requires: vec![],
            components: vec![ComponentSpec {
                id: "chart-b".into(),
                name: "  ".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.counter"), "value")],
                window: WindowSpec::default(),
            }],
        };
        assert!(no_name.validate().is_err());
    }
}
