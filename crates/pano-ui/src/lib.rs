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

/// 本 UI 声明所需的能力与组件目录（架构 §7）。
///
/// `components` 即**组件目录**：每种「窗口内容形态」（组件类型）自带固定 series
/// 与默认窗口规格；监控窗口为其实例（绑定 `component` id，series 由此解析）。
/// 能力校验由 `pano-app` 启动时执行。
///
/// 默认目录基于 sys 适配器（默认 feature 可用）；示例适配器组件保留（默认
/// feature 未编译其适配器时，`list_components` 标记不可用、置灰不可选）。
pub fn uispec() -> UISpec {
    UISpec {
        requires: vec![Capability::new(Capability::TIME_SERIES)],
        components: vec![
            ComponentSpec {
                id: "sys-cpu".into(),
                name: "CPU 使用率".into(),
                series: vec![SeriesId::new(&AdapterId::new("sys.cpu"), "usage")],
                window: WindowSpec {
                    title: "CPU 使用率".into(),
                    size: (520.0, 360.0),
                    ..WindowSpec::default()
                },
            },
            ComponentSpec {
                id: "sys-mem".into(),
                name: "内存使用率".into(),
                series: vec![SeriesId::new(&AdapterId::new("sys.mem"), "used_percent")],
                window: WindowSpec {
                    title: "内存使用率".into(),
                    size: (520.0, 360.0),
                    ..WindowSpec::default()
                },
            },
            ComponentSpec {
                id: "sys-disk".into(),
                name: "磁盘使用率".into(),
                series: vec![SeriesId::new(&AdapterId::new("sys.disk"), "used_percent")],
                window: WindowSpec {
                    title: "磁盘使用率".into(),
                    size: (520.0, 360.0),
                    ..WindowSpec::default()
                },
            },
            ComponentSpec {
                id: "sys-net".into(),
                name: "网络速率".into(),
                series: vec![
                    SeriesId::new(&AdapterId::new("sys.net"), "recv_bps"),
                    SeriesId::new(&AdapterId::new("sys.net"), "sent_bps"),
                ],
                window: WindowSpec {
                    title: "网络速率".into(),
                    size: (640.0, 400.0),
                    ..WindowSpec::default()
                },
            },
            // 资源仪表盘（M2.3）：汽车仪表盘式总览，专用渲染器 SysDashboard.svelte。
            // M2.3.1：磁盘仪表显示最忙盘符，故组件订阅 sys.disk.busiest_disk。
            ComponentSpec {
                id: "sys-dashboard".into(),
                name: "资源仪表盘".into(),
                series: vec![
                    SeriesId::new(&AdapterId::new("sys.cpu"), "usage"),
                    SeriesId::new(&AdapterId::new("sys.mem"), "used_percent"),
                    SeriesId::new(&AdapterId::new("sys.disk"), "active_percent"),
                    SeriesId::new(&AdapterId::new("sys.disk"), "busiest_disk"),
                    SeriesId::new(&AdapterId::new("sys.net"), "utilization"),
                ],
                window: WindowSpec {
                    title: "资源仪表盘".into(),
                    size: (860.0, 540.0),
                    // M2.4：仪表盘窗口默认无边框（小屏 400×100 场景开箱即用；
                    // 内容区拖拽移动，可在「窗口管理」页按窗口覆盖）。
                    decorations: false,
                    ..WindowSpec::default()
                },
            },
            // 示例适配器组件（默认 feature 下适配器未注册 → 目录中置灰不可选）
            ComponentSpec {
                id: "counter-chart".into(),
                name: "示例计数器".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.counter"), "value")],
                window: WindowSpec {
                    title: "示例计数器".into(),
                    size: (480.0, 320.0),
                    ..WindowSpec::default()
                },
            },
            ComponentSpec {
                id: "sine-chart".into(),
                name: "正弦曲线".into(),
                series: vec![SeriesId::new(&AdapterId::new("example.sine"), "value")],
                window: WindowSpec {
                    title: "正弦曲线".into(),
                    size: (640.0, 400.0),
                    ..WindowSpec::default()
                },
            },
            // DeepSeek 额度（M2.5）：专用渲染器 DeepSeekBalance.svelte，数据源 =
            // 远程适配器 deepseek.balance（DeepSeek API 余额）。默认 feature 下
            // 适配器未注册 → 目录中置灰不可选（需 --features adapter-deepseek-balance）。
            ComponentSpec {
                id: "deepseek-balance".into(),
                name: "DeepSeek 额度".into(),
                series: vec![
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "total_cny"),
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "granted_cny"),
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "topped_up_cny"),
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "total_usd"),
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "granted_usd"),
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "topped_up_usd"),
                    SeriesId::new(&AdapterId::new("deepseek.balance"), "is_available"),
                ],
                window: WindowSpec {
                    title: "DeepSeek 额度".into(),
                    size: (520.0, 420.0),
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
