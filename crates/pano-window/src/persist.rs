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
/// 仅影响 `position` / `size` / `monitor` / `decorations`（M2.4：无边框由
/// 窗口级设置覆盖，`layout.decorations` 为 `Some` 时优先，否则用组件声明）；
/// `title` / `always_on_top` / `fullscreen` 始终以声明为准。
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
        decorations: layout.decorations.unwrap_or(spec.decorations),
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
        // 无边框非几何属性：不由布局记忆写回（由窗口设置显式持久化）
        decorations: None,
    }
}

/// 把窗口初始尺寸限制在目标显示器可用工作区内（M2.4 小分辨率支持）。
///
/// 仅当声明尺寸**超出**工作区时缩小（取各维最小值）；不放大、不限制
/// 用户后续的手动缩放（缩放后的几何由布局记忆持久化，恢复时不再 clamp
/// ——见 `create_window` 中对 `layout.size` 的判断）。工作区为逻辑像素。
pub fn fit_size_to_work_area(size: (f64, f64), work_area: (f64, f64)) -> (f64, f64) {
    (size.0.min(work_area.0), size.1.min(work_area.1))
}

/// 更新布局记忆时保留窗口级非几何覆盖（M2.4）。
///
/// 位置 / 大小 / 显示器取自最新几何；`decorations` 保留既有持久化覆盖
/// （否则移动 / 缩放窗口触发的记忆更新会把「无边框」覆盖冲刷为 `None`）。
pub fn layout_with_geometry(
    existing: Option<&WindowLayout>,
    geometry: &WindowGeometry,
) -> WindowLayout {
    WindowLayout {
        position: Some(geometry.position),
        size: Some(geometry.size),
        monitor: Some(geometry.monitor.as_str().to_string()),
        decorations: existing.and_then(|l| l.decorations),
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
            decorations: true,
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
            decorations: None,
        };
        let merged = apply_layout(&spec(), Some(&layout));
        assert_eq!(merged.position, Some((100.0, 200.0)));
        assert_eq!(merged.size, (480.0, 320.0), "size 缺省回落声明值");
        assert_eq!(merged.monitor.as_deref(), Some("monitor-1"));
        // 非几何字段不受布局影响
        assert!(merged.always_on_top);
        assert_eq!(merged.title, "示例");
        // M2.4：布局未覆盖 decorations → 回落组件声明（默认有边框）
        assert!(merged.decorations);
    }

    #[test]
    fn apply_layout_window_decorations_override_wins() {
        // 窗口级无边框覆盖（M2.4）：Some(false) 优先于组件声明（true）
        let layout = WindowLayout {
            position: None,
            size: None,
            monitor: None,
            decorations: Some(false),
        };
        let merged = apply_layout(&spec(), Some(&layout));
        assert!(!merged.decorations);

        // 组件声明无边框 + 布局无覆盖 → 保持无边框
        let frameless_spec = WindowSpec {
            decorations: false,
            ..spec()
        };
        let no_override = WindowLayout {
            decorations: None,
            ..WindowLayout::default()
        };
        assert!(!apply_layout(&frameless_spec, Some(&no_override)).decorations);
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
            decorations: None,
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
            decorations: None,
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
            decorations: None,
        };
        assert_eq!(
            placement_of(&spec_no_position(), Some(&layout)),
            Placement::Default
        );
    }

    #[test]
    fn layout_with_geometry_preserves_window_decorations_override() {
        // M2.4：移动/缩放刷新记忆时保留窗口级无边框覆盖
        let geometry = WindowGeometry {
            position: (1.0, 2.0),
            size: (400.0, 100.0),
            monitor: MonitorId::new("primary"),
        };
        let existing = WindowLayout {
            decorations: Some(false),
            ..WindowLayout::default()
        };
        let layout = layout_with_geometry(Some(&existing), &geometry);
        assert_eq!(layout.position, Some((1.0, 2.0)));
        assert_eq!(layout.size, Some((400.0, 100.0)));
        assert_eq!(layout.decorations, Some(false), "覆盖不被几何刷新冲刷");
        // 无既有覆盖 → None（沿用组件声明）
        assert_eq!(layout_with_geometry(None, &geometry).decorations, None);
    }

    #[test]
    fn fit_size_to_work_area_clamps_only_when_larger() {
        // 声明尺寸超出小屏工作区 → 缩到工作区（400×100 小屏场景）
        assert_eq!(
            fit_size_to_work_area((860.0, 540.0), (400.0, 100.0)),
            (400.0, 100.0)
        );
        // 已小于工作区 → 保持不变（不放大）
        assert_eq!(
            fit_size_to_work_area((320.0, 90.0), (400.0, 100.0)),
            (320.0, 90.0)
        );
        // 单维超出也逐维 clamp
        assert_eq!(
            fit_size_to_work_area((860.0, 80.0), (400.0, 100.0)),
            (400.0, 80.0)
        );
        assert_eq!(
            fit_size_to_work_area((300.0, 540.0), (400.0, 100.0)),
            (300.0, 100.0)
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
            decorations: true,
        }
    }
}
