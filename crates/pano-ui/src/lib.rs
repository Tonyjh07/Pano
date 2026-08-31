//! Pano 图形界面（Tauri v2）：Rust 命令层 + 事件桥接 + 前端（Svelte）。
//!
//! 依赖方向铁律：`pano-ui` 只读 `pano-core` 的公共 API，不知道具体适配器；
//! 窗口控制一律经 [`pano_window::WindowService`]，不直接操作 Tauri 窗口类型。
//! UI 通过 `UISpec` 声明所需能力与组件清单，由 `pano-app` 启动时校验（架构 §7）。

use pano_core::adapter::{AdapterId, SeriesId};
use pano_core::capability::{Capability, ComponentSpec, UISpec, WindowSpec};

pub mod bridge;
pub mod commands;
pub mod dto;
pub mod state;

/// 本 UI 声明所需的能力与组件清单（架构 §7）。
///
/// 能力校验由 `pano-app` 启动时执行；组件窗口按 `components` 逐个创建
/// （仅当组件的 series 存在数据源，见架构 §7「建窗时机」）。
pub fn uispec() -> UISpec {
    UISpec {
        requires: vec![Capability::new(Capability::TIME_SERIES)],
        components: vec![
            ComponentSpec {
                id: "counter-chart".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.counter"), "value")],
                window: WindowSpec {
                    title: "示例计数器".into(),
                    size: (480.0, 320.0),
                    ..WindowSpec::default()
                },
            },
            ComponentSpec {
                id: "sine-chart".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.sine"), "value")],
                window: WindowSpec {
                    title: "正弦曲线".into(),
                    size: (640.0, 400.0),
                    ..WindowSpec::default()
                },
            },
        ],
    }
}

/// 管理窗口的窗口声明（pano-app 固定创建，ui.md §2；不属于 `components`）。
pub fn manager_window_spec() -> WindowSpec {
    WindowSpec {
        title: "Pano 管理".into(),
        size: (1100.0, 720.0),
        ..WindowSpec::default()
    }
}
