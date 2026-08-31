//! 布局持久化（架构 §13 / spec §8）：窗口位置 / 大小 / 所在显示器的记忆与恢复。
//!
//! - 恢复：启动建窗时把 `[window.<id>]` 段（[`WindowLayout`]）合并进
//!   [`WindowSpec`]（布局记忆优先于声明默认值）；
//! - 记录：窗口关闭 / 退出时读取几何，生成 [`WindowLayout`] 交 pano-app
//!   写回 `pano.toml`（与热重载写文件由 pano-app 协调串行）。
//!
//! 本模块为纯数据逻辑，不依赖任何窗口实现，可直接单测。

use pano_core::capability::WindowSpec;
use pano_core::config::WindowLayout;

use crate::WindowGeometry;
use crate::monitor::MonitorId;

/// 把持久化布局合并进窗口声明：布局（记忆）优先，缺省回落声明值。
///
/// 仅影响 `position` / `size` / `monitor`；`title` / `always_on_top` /
/// `fullscreen` 始终以声明为准。
pub fn apply_layout(spec: &WindowSpec, layout: Option<&WindowLayout>) -> WindowSpec {
    let Some(layout) = layout else {
        return spec.clone();
    };
    WindowSpec {
        title: spec.title.clone(),
        size: layout.size.unwrap_or(spec.size),
        position: layout.position.or(spec.position),
        always_on_top: spec.always_on_top,
        fullscreen: spec.fullscreen,
        monitor: layout.monitor.clone().or_else(|| spec.monitor.clone()),
    }
}

/// 由运行时几何生成持久化布局（窗口关闭 / 退出时记录）。
///
/// 位置 / 大小 / 显示器一律记录（`position` 始终有意义；大小与显示器
/// 即使与声明一致也写回，保持「所见即所得」的恢复语义）。
pub fn layout_from_geometry(geometry: &WindowGeometry) -> WindowLayout {
    WindowLayout {
        position: Some(geometry.position),
        size: Some(geometry.size),
        monitor: Some(geometry.monitor.as_str().to_string()),
    }
}

/// 解析显示器序列化标识（兼容 core 侧字符串形式）。
///
/// `None` / 空串 → 主显示器（缺省语义，见架构 §7）。
pub fn resolve_monitor_id(serialized: Option<&str>) -> MonitorId {
    match serialized {
        Some(s) if !s.is_empty() => MonitorId::new(s),
        _ => MonitorId::primary(),
    }
}

/// 建窗定位决策（纯逻辑，可单测）。
///
/// - 有记忆位置 → [`Placement::Position`]：直接用记忆位置（「下次启动恢复」
///   语义，架构 §13）；**不得**再用显示器原点覆盖记忆位置；
/// - 无记忆位置但偏好指定显示器 → [`Placement::Monitor`]：实现层解析其原点
///   定位；显示器不可用时**回退默认位置**（warn 日志），不得让建窗失败；
/// - 其余 → [`Placement::Default`]：居中 / 系统默认。
#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    /// 直接使用该位置（逻辑像素）。
    Position((f64, f64)),
    /// 定位到指定显示器原点。
    Monitor(MonitorId),
    /// 无显式定位（居中 / 系统默认）。
    Default,
}

/// 由声明 + 持久化布局得出定位决策（建窗时调用）。
pub fn placement_of(spec: &WindowSpec, layout: Option<&WindowLayout>) -> Placement {
    let merged = apply_layout(spec, layout);
    if let Some(pos) = merged.position {
        return Placement::Position(pos);
    }
    let monitor = resolve_monitor_id(merged.monitor.as_deref());
    if monitor.is_primary() {
        Placement::Default
    } else {
        Placement::Monitor(monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> WindowSpec {
        WindowSpec {
            title: "示例".into(),
            size: (480.0, 320.0),
            position: Some((10.0, 20.0)),
            always_on_top: true,
            fullscreen: false,
            monitor: Some("primary".into()),
        }
    }

    #[test]
    fn apply_layout_without_layout_keeps_spec() {
        let s = spec();
        let merged = apply_layout(&s, None);
        assert_eq!(merged.size, s.size);
        assert_eq!(merged.position, s.position);
        assert_eq!(merged.monitor, s.monitor);
        assert!(merged.always_on_top);
    }

    #[test]
    fn apply_layout_layout_wins_and_falls_back() {
        let layout = WindowLayout {
            position: Some((100.0, 200.0)),
            size: None, // 缺省回落声明值
            monitor: Some("monitor-1".into()),
        };
        let merged = apply_layout(&spec(), Some(&layout));
        assert_eq!(merged.position, Some((100.0, 200.0)));
        assert_eq!(merged.size, (480.0, 320.0), "size 缺省回落声明值");
        assert_eq!(merged.monitor.as_deref(), Some("monitor-1"));
        // 非几何字段不受布局影响
        assert!(merged.always_on_top);
        assert_eq!(merged.title, "示例");
    }

    #[test]
    fn layout_from_geometry_roundtrip() {
        let geometry = WindowGeometry {
            position: (320.0, 240.0),
            size: (800.0, 600.0),
            monitor: MonitorId::new("monitor-2"),
        };
        let layout = layout_from_geometry(&geometry);
        assert_eq!(layout.position, Some((320.0, 240.0)));
        assert_eq!(layout.size, Some((800.0, 600.0)));
        assert_eq!(layout.monitor.as_deref(), Some("monitor-2"));
    }

    #[test]
    fn resolve_monitor_id_defaults_to_primary() {
        assert_eq!(resolve_monitor_id(None), MonitorId::primary());
        assert_eq!(resolve_monitor_id(Some("")), MonitorId::primary());
        assert_eq!(
            resolve_monitor_id(Some("monitor-3")),
            MonitorId::new("monitor-3")
        );
    }

    #[test]
    fn placement_uses_remembered_position_over_monitor() {
        let layout = WindowLayout {
            position: Some((300.0, 200.0)),
            size: Some((800.0, 600.0)),
            monitor: Some("monitor-2".into()),
        };
        // 记忆位置优先于显示器偏好（B1 回归：不得被显示器原点覆盖）。
        assert_eq!(
            placement_of(&spec(), Some(&layout)),
            Placement::Position((300.0, 200.0))
        );
    }

    #[test]
    fn placement_falls_back_to_monitor_without_position() {
        let layout = WindowLayout {
            position: None,
            size: None,
            monitor: Some("monitor-2".into()),
        };
        assert_eq!(
            placement_of(&spec_no_position(), Some(&layout)),
            Placement::Monitor(MonitorId::new("monitor-2"))
        );
    }

    #[test]
    fn placement_default_without_layout_or_primary() {
        assert_eq!(placement_of(&spec_no_position(), None), Placement::Default);
        let layout = WindowLayout {
            position: None,
            size: None,
            monitor: Some("primary".into()),
        };
        assert_eq!(
            placement_of(&spec_no_position(), Some(&layout)),
            Placement::Default
        );
    }

    /// 无位置 / 无显示器偏好的声明（定位决策测试用）。
    fn spec_no_position() -> WindowSpec {
        WindowSpec {
            title: "示例".into(),
            size: (480.0, 320.0),
            position: None,
            always_on_top: false,
            fullscreen: false,
            monitor: None,
        }
    }
}
