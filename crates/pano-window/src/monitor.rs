//! 显示器标识与信息（窗口领域，pano-window 定义）。
//!
//! `pano-core` 侧（`WindowSpec` / `WindowLayout`）只使用**字符串序列化形式**
//! （如 `"primary"` / 自定义编号），由本模块解析为 [`MonitorId`]。

use std::fmt;

/// 主显示器标识。
pub const PRIMARY_MONITOR_ID: &str = "primary";

/// 显示器标识（窗口领域；core 只用字符串序列化形式，见架构 §7）。
///
/// id 形式：主显示器 = `"primary"`；其余 = 平台名称（如 Windows 的
/// `\\.\DISPLAY1`）；无名称时 = `monitor@<x>,<y>`（物理位置编码）。
/// 亦允许 API / 配置传入自定义编号（如 `"monitor-1"`）。
/// 同名显示器（同型号多台）可能产生相同 id，属已知边界（解析恒取第一台）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonitorId(String);

impl MonitorId {
    /// 主显示器标识。
    pub fn primary() -> Self {
        Self::new(PRIMARY_MONITOR_ID)
    }

    /// 构造标识。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 原始字符串视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否为主显示器。
    pub fn is_primary(&self) -> bool {
        self.0 == PRIMARY_MONITOR_ID
    }
}

impl fmt::Display for MonitorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for MonitorId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

impl From<String> for MonitorId {
    fn from(id: String) -> Self {
        Self::new(id)
    }
}

/// 显示器信息（枚举 / 持久化展示用）。
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    /// 显示器标识（序列化形式，可写入 `[window.<id>].monitor`）。
    pub id: MonitorId,
    /// 显示器名称（平台原生名，可能为空）。
    pub name: String,
    /// 是否主显示器。
    pub is_primary: bool,
    /// 物理像素位置（桌面坐标系，原点在主显示器左上角）。
    pub position: (i32, i32),
    /// 物理像素尺寸。
    pub size: (u32, u32),
    /// 缩放因子（物理像素 / 逻辑像素）。
    pub scale_factor: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_id_semantics() {
        let id = MonitorId::primary();
        assert!(id.is_primary());
        assert_eq!(id.as_str(), "primary");
        assert_eq!(id.to_string(), "primary");
    }

    #[test]
    fn id_equality_and_conversions() {
        assert_eq!(MonitorId::new("monitor-1"), MonitorId::from("monitor-1"));
        assert_eq!(
            MonitorId::new(String::from("monitor-1")),
            MonitorId::new("monitor-1")
        );
        assert_ne!(MonitorId::primary(), MonitorId::new("monitor-1"));
    }

    #[test]
    fn monitor_info_shape() {
        let info = MonitorInfo {
            id: MonitorId::primary(),
            name: "内置显示器".into(),
            is_primary: true,
            position: (0, 0),
            size: (3840, 2160),
            scale_factor: 2.0,
        };
        assert!(info.is_primary);
        assert_eq!(info.size, (3840, 2160));
    }
}
