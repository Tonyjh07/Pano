//! Pano 窗口服务（M1.2，架构 §13 / ui.md §2）。
//!
//! **组件与窗口分离**（R1）：组件只声明内容（`ComponentSpec`）与窗口需求
//! （[`pano_core::capability::WindowSpec`]），窗口的创建 / 控制 / 持久化全部
//! 由本服务（pano-app 装配）负责；UI 组件只持 [`WindowHandle`] 调 API（R2），
//! **不接触任何窗口实现**。
//!
//! 依赖方向（M1.2 审查定稿）：
//! - 不依赖 core 的**运行逻辑**（lifecycle / sample_store / config 解析），
//!   允许引用 core 的**纯数据声明**（`WindowSpec` / `WindowLayout`）；
//! - 不依赖 adapters / ui；由 pano-app 装配（默认 Tauri v2 实现，见 `tauri` 模块）。

pub mod monitor;
pub mod persist;

pub use monitor::{MonitorId, MonitorInfo};

use pano_core::capability::WindowSpec;
use pano_core::config::WindowLayout;

#[cfg(feature = "tauri")]
pub mod tauri;

/// 窗口服务错误（spec §6）：库内不 panic。
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    /// 引用不存在的窗口。
    #[error("窗口不存在：{0}")]
    WindowNotFound(String),

    /// 显示器不可用（id 无法解析到当前任意显示器）。
    #[error("显示器不可用：{0}")]
    MonitorUnavailable(String),

    /// 试图创建已存在的窗口。
    #[error("窗口已存在：{0}")]
    AlreadyExists(String),

    /// IO 层错误。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    /// 窗口操作失败（平台 / 实现层错误）。
    #[error("窗口操作失败：{0}")]
    Other(String),
}

/// 窗口几何信息（布局持久化读取，见 `persist` 模块）。
#[derive(Debug, Clone, PartialEq)]
pub struct WindowGeometry {
    /// 位置（逻辑像素）。
    pub position: (f64, f64),
    /// 大小（逻辑像素）。
    pub size: (f64, f64),
    /// 所在显示器。
    pub monitor: MonitorId,
}

/// 窗口控制句柄：组件 / 主程序经此控制窗口（不接触窗口实现）。
///
/// 全部调用**收敛到本 API**：命令层与前端不直接操作 Tauri 窗口类型，
/// 换窗口后端组件零改动（架构 §13 / ui.md §9）。
pub trait WindowHandle: Send + Sync {
    /// 窗口标识（组件 id）。
    fn label(&self) -> &str;

    /// 全屏切换。
    fn set_fullscreen(&self, enabled: bool) -> Result<(), WindowError>;

    /// 置顶切换。
    fn set_always_on_top(&self, enabled: bool) -> Result<(), WindowError>;

    /// 绑定到指定显示器（配合位置 / 大小；显示器不可用 →
    /// [`WindowError::MonitorUnavailable`]）。
    fn set_monitor(&self, monitor: &MonitorId) -> Result<(), WindowError>;

    /// 无边框切换（M2.4）：`false` = 去掉系统边框 / 标题栏（内容区拖拽移动）。
    fn set_decorations(&self, enabled: bool) -> Result<(), WindowError>;

    /// 当前是否带系统边框（管理页「无边框」开关显示 / 前端拖拽判定）。
    fn is_decorated(&self) -> Result<bool, WindowError>;

    /// 设置位置（逻辑像素）。
    fn set_position(&self, x: f64, y: f64) -> Result<(), WindowError>;

    /// 设置大小（逻辑像素）。
    fn set_size(&self, width: f64, height: f64) -> Result<(), WindowError>;

    /// 聚焦。
    fn focus(&self) -> Result<(), WindowError>;

    /// 显示。
    fn show(&self) -> Result<(), WindowError>;

    /// 隐藏。
    fn hide(&self) -> Result<(), WindowError>;

    /// 关闭 = 隐藏（不退出进程；退出仅经托盘「退出」，M1.1 语义延续）。
    fn close(&self) -> Result<(), WindowError>;

    /// 销毁窗口（彻底关闭并从窗口管理器移除；区别于 `close` = 隐藏）。
    ///
    /// 销毁后该窗口的 `handle` / `exists` 不再可用；当前窗口列表
    /// （管理页 / 托盘）由上层注册表同步移除。
    fn destroy(&self) -> Result<(), WindowError>;

    /// 是否可见（管理页窗口列表展示隐藏 / 显示状态）。
    fn is_visible(&self) -> Result<bool, WindowError>;

    /// 读取当前几何（供布局持久化记录）。
    fn geometry(&self) -> Result<WindowGeometry, WindowError>;
}

/// 窗口服务：窗口生命周期、控制、显示器枚举（架构 §13）。
///
/// 由 pano-app 装配（默认 Tauri v2 实现）；UI 侧经命令层调用。
pub trait WindowService: Send + Sync {
    /// 按声明创建窗口（1 组件 = 1 窗口，窗口标识 = 组件 id）。
    ///
    /// `layout` 为 `[window.<id>]` 持久化布局（`None` = 无记忆，用声明默认）。
    /// 窗口已存在 → [`WindowError::AlreadyExists`]。
    fn create_window(
        &self,
        id: &str,
        spec: &WindowSpec,
        layout: Option<&WindowLayout>,
    ) -> Result<Box<dyn WindowHandle>, WindowError>;

    /// 取已创建窗口的控制句柄。
    fn handle(&self, id: &str) -> Result<Box<dyn WindowHandle>, WindowError>;

    /// 窗口是否已创建。
    fn exists(&self, id: &str) -> bool;

    /// 隐藏窗口（关闭语义）。
    fn hide(&self, id: &str) -> Result<(), WindowError>;

    /// 显示并聚焦窗口（托盘「打开」命令）。
    fn show(&self, id: &str) -> Result<(), WindowError>;

    /// 销毁窗口（彻底关闭并从窗口管理器移除；区别于 `hide` = 隐藏）。
    fn destroy(&self, id: &str) -> Result<(), WindowError>;

    /// 窗口是否可见。
    fn is_visible(&self, id: &str) -> Result<bool, WindowError>;

    /// 枚举全部显示器。
    fn monitors(&self) -> Result<Vec<MonitorInfo>, WindowError>;

    /// 主显示器信息。
    fn primary_monitor(&self) -> Result<MonitorInfo, WindowError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小化假实现：验证 trait 可作为 trait-object 使用。
    struct FakeHandle {
        label: String,
    }

    impl WindowHandle for FakeHandle {
        fn label(&self) -> &str {
            &self.label
        }
        fn set_fullscreen(&self, _enabled: bool) -> Result<(), WindowError> {
            Ok(())
        }
        fn set_always_on_top(&self, _enabled: bool) -> Result<(), WindowError> {
            Ok(())
        }
        fn set_monitor(&self, _monitor: &MonitorId) -> Result<(), WindowError> {
            Ok(())
        }
        fn set_decorations(&self, _enabled: bool) -> Result<(), WindowError> {
            Ok(())
        }
        fn is_decorated(&self) -> Result<bool, WindowError> {
            Ok(true)
        }
        fn set_position(&self, _x: f64, _y: f64) -> Result<(), WindowError> {
            Ok(())
        }
        fn set_size(&self, _width: f64, _height: f64) -> Result<(), WindowError> {
            Ok(())
        }
        fn focus(&self) -> Result<(), WindowError> {
            Ok(())
        }
        fn show(&self) -> Result<(), WindowError> {
            Ok(())
        }
        fn hide(&self) -> Result<(), WindowError> {
            Ok(())
        }
        fn close(&self) -> Result<(), WindowError> {
            Ok(())
        }
        fn destroy(&self) -> Result<(), WindowError> {
            Ok(())
        }
        fn is_visible(&self) -> Result<bool, WindowError> {
            Ok(true)
        }
        fn geometry(&self) -> Result<WindowGeometry, WindowError> {
            Ok(WindowGeometry {
                position: (0.0, 0.0),
                size: (800.0, 600.0),
                monitor: MonitorId::primary(),
            })
        }
    }

    struct FakeService {
        windows: std::sync::Mutex<std::collections::HashMap<String, Box<dyn WindowHandle>>>,
    }

    impl WindowService for FakeService {
        fn create_window(
            &self,
            id: &str,
            _spec: &WindowSpec,
            _layout: Option<&WindowLayout>,
        ) -> Result<Box<dyn WindowHandle>, WindowError> {
            let mut map = self.windows.lock().unwrap_or_else(|e| e.into_inner());
            if map.contains_key(id) {
                return Err(WindowError::AlreadyExists(id.into()));
            }
            map.insert(id.to_string(), Box::new(FakeHandle { label: id.into() }));
            Ok(Box::new(FakeHandle { label: id.into() }))
        }
        fn handle(&self, id: &str) -> Result<Box<dyn WindowHandle>, WindowError> {
            if self.exists(id) {
                Ok(Box::new(FakeHandle { label: id.into() }))
            } else {
                Err(WindowError::WindowNotFound(id.into()))
            }
        }
        fn exists(&self, id: &str) -> bool {
            self.windows
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(id)
        }
        fn hide(&self, id: &str) -> Result<(), WindowError> {
            if self.exists(id) {
                Ok(())
            } else {
                Err(WindowError::WindowNotFound(id.into()))
            }
        }
        fn show(&self, id: &str) -> Result<(), WindowError> {
            self.hide(id)
        }
        fn destroy(&self, id: &str) -> Result<(), WindowError> {
            let mut map = self.windows.lock().unwrap_or_else(|e| e.into_inner());
            if map.remove(id).is_some() {
                Ok(())
            } else {
                Err(WindowError::WindowNotFound(id.into()))
            }
        }
        fn is_visible(&self, id: &str) -> Result<bool, WindowError> {
            if self.exists(id) {
                Ok(true)
            } else {
                Err(WindowError::WindowNotFound(id.into()))
            }
        }
        fn monitors(&self) -> Result<Vec<MonitorInfo>, WindowError> {
            Ok(vec![monitor_info()])
        }
        fn primary_monitor(&self) -> Result<MonitorInfo, WindowError> {
            Ok(monitor_info())
        }
    }

    fn monitor_info() -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::primary(),
            name: "fake".into(),
            is_primary: true,
            position: (0, 0),
            size: (1920, 1080),
            scale_factor: 1.0,
        }
    }

    #[test]
    fn trait_objects_work() {
        let service: Box<dyn WindowService> = Box::new(FakeService {
            windows: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        assert!(!service.exists("chart-a"));

        let handle = service
            .create_window("chart-a", &WindowSpec::default(), None)
            .expect("创建成功");
        assert_eq!(handle.label(), "chart-a");
        assert!(service.exists("chart-a"));

        // 重复创建 → AlreadyExists
        let err = service
            .create_window("chart-a", &WindowSpec::default(), None)
            .err()
            .expect("重复创建应失败");
        assert!(matches!(err, WindowError::AlreadyExists(_)));

        // handle 查询与隐藏
        let h2 = service.handle("chart-a").expect("存在");
        assert_eq!(h2.label(), "chart-a");
        service.hide("chart-a").expect("隐藏成功");
        let err = service.handle("missing").err().expect("缺失窗口应失败");
        assert!(matches!(err, WindowError::WindowNotFound(_)));

        // 可见性 / 销毁（区别于隐藏：destroy 后窗口不存在）
        assert!(service.is_visible("chart-a").expect("可见"));
        service.destroy("chart-a").expect("销毁成功");
        assert!(!service.exists("chart-a"), "销毁后窗口不再存在");
        assert!(matches!(
            service.handle("chart-a").err().expect("应失败"),
            WindowError::WindowNotFound(_)
        ));

        let _monitors = service.monitors().unwrap();
        let _primary = service.primary_monitor().unwrap();
    }
}
