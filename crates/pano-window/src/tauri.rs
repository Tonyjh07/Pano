//! Tauri v2 窗口服务实现（架构 §13 / ui.md §9）。
//!
//! - 窗口 = `tauri::WebviewWindow`（独立 WebView 渲染进程）；
//! - 全屏 / 置顶 / 位置 / 大小 / 聚焦 / 显示器 = Tauri 原生窗口与 monitor API；
//! - **调用路径一律收敛**：前端经 Tauri 命令 → 命令层 → [`WindowService`] /
//!   [`WindowHandle`]；命令层与组件不直接操作 Tauri 窗口类型（防止 Tauri
//!   类型泄漏进 pano-ui，保证换窗口后端组件零改动）。
//!
//! 依赖方向：本模块依赖 `tauri`（pano-window 的 `tauri` feature），
//! 不依赖 core 运行逻辑 / adapters / ui。

use pano_core::capability::WindowSpec;
use pano_core::config::WindowLayout;
use tauri::Manager;

use crate::monitor::{MonitorId, MonitorInfo, PRIMARY_MONITOR_ID};
use crate::persist;
use crate::{WindowError, WindowGeometry, WindowHandle, WindowService};

/// Tauri v2 实现：持有 `AppHandle`，按标签管理 `WebviewWindow`。
///
/// 所有窗口加载同一前端入口（`entry`，如 `index.html`），WebView 内经
/// `getCurrentWindow().label` 识别自己是管理窗口还是哪个组件窗口。
pub struct TauriWindowService {
    app: tauri::AppHandle,
    /// 前端入口 URL 相对路径。
    entry: String,
}

impl TauriWindowService {
    /// 以默认入口（`index.html`）构造。
    pub fn new(app: tauri::AppHandle) -> Self {
        Self::with_entry(app, "index.html")
    }

    /// 以指定前端入口构造。
    pub fn with_entry(app: tauri::AppHandle, entry: impl Into<String>) -> Self {
        Self {
            app,
            entry: entry.into(),
        }
    }
}

impl WindowService for TauriWindowService {
    fn create_window(
        &self,
        id: &str,
        spec: &WindowSpec,
        layout: Option<&WindowLayout>,
    ) -> Result<Box<dyn WindowHandle>, WindowError> {
        if self.app.get_webview_window(id).is_some() {
            return Err(WindowError::AlreadyExists(id.into()));
        }
        let merged = persist::apply_layout(spec, layout);

        let mut builder = tauri::webview::WebviewWindowBuilder::new(
            &self.app,
            id,
            tauri::WebviewUrl::App(self.entry.clone().into()),
        )
        .title(&merged.title)
        .inner_size(merged.size.0, merged.size.1)
        .always_on_top(merged.always_on_top)
        .fullscreen(merged.fullscreen);

        // 定位决策（persist::placement_of）：记忆位置优先；
        // 无记忆位置但有显示器偏好 → 定位该显示器（不可用回退默认，不硬失败）。
        match persist::placement_of(spec, layout) {
            persist::Placement::Position((x, y)) => {
                builder = builder.position(x, y);
            }
            persist::Placement::Monitor(monitor) => match monitor_origin(&self.app, &monitor) {
                Ok((x, y)) => builder = builder.position(x, y),
                Err(error) => {
                    tracing::warn!(
                        target: "pano::window",
                        id,
                        monitor = %monitor,
                        error = %error,
                        "显示器不可用，回退默认位置"
                    );
                }
            },
            persist::Placement::Default => {}
        }

        let window = builder
            .build()
            .map_err(|e| WindowError::Other(format!("创建窗口 {id} 失败：{e}")))?;
        tracing::info!(target: "pano::window", id, title = %merged.title, "窗口已创建");
        Ok(Box::new(TauriWindowHandle { window }))
    }

    fn handle(&self, id: &str) -> Result<Box<dyn WindowHandle>, WindowError> {
        self.app
            .get_webview_window(id)
            .map(|w| Box::new(TauriWindowHandle { window: w }) as Box<dyn WindowHandle>)
            .ok_or_else(|| WindowError::WindowNotFound(id.into()))
    }

    fn exists(&self, id: &str) -> bool {
        self.app.get_webview_window(id).is_some()
    }

    fn hide(&self, id: &str) -> Result<(), WindowError> {
        tracing::debug!(target: "pano::window", id, "隐藏窗口");
        self.handle(id)?.hide()
    }

    fn show(&self, id: &str) -> Result<(), WindowError> {
        tracing::debug!(target: "pano::window", id, "显示并聚焦窗口");
        let handle = self.handle(id)?;
        handle.show()?;
        handle.focus()
    }

    fn destroy(&self, id: &str) -> Result<(), WindowError> {
        tracing::info!(target: "pano::window", id, "销毁窗口");
        self.handle(id)?.destroy()
    }

    fn is_visible(&self, id: &str) -> Result<bool, WindowError> {
        self.handle(id)?.is_visible()
    }

    fn monitors(&self) -> Result<Vec<MonitorInfo>, WindowError> {
        let monitors: Vec<MonitorInfo> = self
            .app
            .available_monitors()
            .map_err(|e| WindowError::Other(format!("枚举显示器失败：{e}")))?
            .into_iter()
            .map(|m| monitor_info(&m))
            .collect();
        Ok(monitors)
    }

    fn primary_monitor(&self) -> Result<MonitorInfo, WindowError> {
        self.app
            .primary_monitor()
            .map_err(|e| WindowError::Other(format!("查询主显示器失败：{e}")))?
            .map(|m| monitor_info(&m))
            .ok_or_else(|| WindowError::MonitorUnavailable("无主显示器".into()))
    }
}

/// Tauri 版窗口控制句柄：内部持有 `WebviewWindow`，对外只暴露本 crate 的 API。
pub struct TauriWindowHandle {
    window: tauri::WebviewWindow,
}

impl WindowHandle for TauriWindowHandle {
    fn label(&self) -> &str {
        self.window.label()
    }

    fn set_fullscreen(&self, enabled: bool) -> Result<(), WindowError> {
        self.window
            .set_fullscreen(enabled)
            .map_err(|e| WindowError::Other(format!("全屏切换失败：{e}")))
    }

    fn set_always_on_top(&self, enabled: bool) -> Result<(), WindowError> {
        self.window
            .set_always_on_top(enabled)
            .map_err(|e| WindowError::Other(format!("置顶切换失败：{e}")))
    }

    fn set_monitor(&self, monitor: &MonitorId) -> Result<(), WindowError> {
        tracing::debug!(target: "pano::window", id = %self.window.label(), monitor = %monitor, "绑定显示器");
        let (x, y) = monitor_origin(self.window.app_handle(), monitor)?;
        self.window
            .set_position(tauri::LogicalPosition::new(x, y))
            .map_err(|e| WindowError::Other(format!("移动到显示器失败：{e}")))
    }

    fn set_position(&self, x: f64, y: f64) -> Result<(), WindowError> {
        tracing::debug!(target: "pano::window", id = %self.window.label(), x, y, "设置位置");
        self.window
            .set_position(tauri::LogicalPosition::new(x, y))
            .map_err(|e| WindowError::Other(format!("设置位置失败：{e}")))
    }

    fn set_size(&self, width: f64, height: f64) -> Result<(), WindowError> {
        self.window
            .set_size(tauri::LogicalSize::new(width, height))
            .map_err(|e| WindowError::Other(format!("设置大小失败：{e}")))
    }

    fn focus(&self) -> Result<(), WindowError> {
        self.window
            .set_focus()
            .map_err(|e| WindowError::Other(format!("聚焦失败：{e}")))
    }

    fn show(&self) -> Result<(), WindowError> {
        self.window
            .show()
            .map_err(|e| WindowError::Other(format!("显示窗口失败：{e}")))
    }

    fn hide(&self) -> Result<(), WindowError> {
        self.window
            .hide()
            .map_err(|e| WindowError::Other(format!("隐藏窗口失败：{e}")))
    }

    fn close(&self) -> Result<(), WindowError> {
        self.window
            .hide()
            .map_err(|e| WindowError::Other(format!("关闭（隐藏）窗口失败：{e}")))
    }

    fn destroy(&self) -> Result<(), WindowError> {
        self.window
            .destroy()
            .map_err(|e| WindowError::Other(format!("销毁窗口失败：{e}")))
    }

    fn is_visible(&self) -> Result<bool, WindowError> {
        self.window
            .is_visible()
            .map_err(|e| WindowError::Other(format!("读取可见性失败：{e}")))
    }

    fn geometry(&self) -> Result<WindowGeometry, WindowError> {
        let scale = self
            .window
            .scale_factor()
            .map_err(|e| WindowError::Other(format!("读取缩放因子失败：{e}")))?;
        let pos = self
            .window
            .outer_position()
            .map_err(|e| WindowError::Other(format!("读取位置失败：{e}")))?;
        let size = self
            .window
            .outer_size()
            .map_err(|e| WindowError::Other(format!("读取大小失败：{e}")))?;
        let pos_logical = pos.to_logical::<f64>(scale);
        let size_logical = size.to_logical::<f64>(scale);
        let monitor = self
            .window
            .current_monitor()
            .map_err(|e| WindowError::Other(format!("读取显示器失败：{e}")))?
            .map(|m| monitor_to_id(&m))
            .unwrap_or_else(MonitorId::primary);
        Ok(WindowGeometry {
            position: (pos_logical.x, pos_logical.y),
            size: (size_logical.width, size_logical.height),
            monitor,
        })
    }
}

/// 物理显示器 → [`MonitorId`]：主显示器为 `"primary"`，其余优先用名称，
/// 无名称时用物理位置编码。
fn monitor_to_id(mon: &tauri::Monitor) -> MonitorId {
    if is_primary(mon) {
        return MonitorId::primary();
    }
    if let Some(name) = mon.name() {
        return MonitorId::new(name.clone());
    }
    let pos = mon.position();
    MonitorId::new(format!("monitor@{}x{}", pos.x, pos.y))
}

/// 主显示器判定：桌面坐标系原点 (0,0) 位于主显示器左上角（跨平台惯例）。
fn is_primary(mon: &tauri::Monitor) -> bool {
    let pos = mon.position();
    pos.x == 0 && pos.y == 0
}

/// [`MonitorId`] → 物理显示器；解析失败 → [`WindowError::MonitorUnavailable`]。
fn resolve_monitor(app: &tauri::AppHandle, id: &MonitorId) -> Result<tauri::Monitor, WindowError> {
    if id.as_str() == PRIMARY_MONITOR_ID {
        return app
            .primary_monitor()
            .map_err(|e| WindowError::Other(format!("查询主显示器失败：{e}")))?
            .ok_or_else(|| WindowError::MonitorUnavailable(PRIMARY_MONITOR_ID.to_string()));
    }
    let monitors = app
        .available_monitors()
        .map_err(|e| WindowError::Other(format!("枚举显示器失败：{e}")))?;
    monitors
        .into_iter()
        .find(|m| monitor_to_id(m) == *id)
        .ok_or_else(|| WindowError::MonitorUnavailable(id.to_string()))
}

/// 目标显示器桌面原点（逻辑像素），用于把窗口定位到指定显示器。
fn monitor_origin(app: &tauri::AppHandle, id: &MonitorId) -> Result<(f64, f64), WindowError> {
    let mon = resolve_monitor(app, id)?;
    let scale = mon.scale_factor();
    let origin = mon.position().to_logical::<f64>(scale);
    Ok((origin.x, origin.y))
}

/// 物理显示器 → [`MonitorInfo`]。
fn monitor_info(mon: &tauri::Monitor) -> MonitorInfo {
    let pos = mon.position();
    let size = mon.size();
    MonitorInfo {
        id: monitor_to_id(mon),
        name: mon.name().cloned().unwrap_or_default(),
        is_primary: is_primary(mon),
        position: (pos.x, pos.y),
        size: (size.width, size.height),
        scale_factor: mon.scale_factor(),
    }
}
