//! 能力标签：UI 按能力声明需求（UISpec），core 按能力校验。

use std::fmt;

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

/// UI 声明所需能力的清单（见架构 §7）。
///
/// `pano-app` 启动时校验：`requires ⊆ 已启用适配器的能力并集`，
/// 不满足则明确报错退出（提示缺哪个能力）。
#[derive(Debug, Clone, Default)]
pub struct UISpec {
    /// 本 UI 必须满足的能力。
    pub requires: Vec<Capability>,
}

impl UISpec {
    /// 声明需要的能力。
    pub fn requires(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            requires: capabilities.into_iter().collect(),
        }
    }
}
