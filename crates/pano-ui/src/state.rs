//! pano-app 注入的共享状态与回调（M1.2 命令层）。
//!
//! `AppState` 由 pano-app 装配并放入 Tauri managed state；命令层经
//! `tauri::State<AppState>` 访问。文件写入（pano.toml）留在 pano-app
//! 侧经 `save_config` 回调注入，保证与 core 热重载写文件串行协调
//! （架构 §13：段级合并，避免并发写）。
//!
//! 窗口管理（M2.1 + M2.2）：运行时窗口注册表 [`AppState::windows`] 是「当前
//! 存在哪些监控窗口」的唯一来源。启动时由 pano-app 从组件目录 / 持久化集合
//! 播种（连同固定管理窗口），随后「窗口管理」命令层在此增删改；托盘「窗口
//! 列表」经 [`RefreshTrayFn`] 回调据此重建。每个窗口为**组件类型实例**：
//! 只存 `{ title, component }`，展示的 series 由组件目录（`UISpec.components`）
//! 解析（M2.2，架构 §7）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pano_core::capability::UISpec;
use pano_core::config::WindowLayout;
use pano_core::lifecycle::Lifecycle;
use pano_core::sample_store::SampleStore;
use pano_window::WindowService;

/// 配置持久化回调：序列化当前配置并写回 `pano.toml`（pano-app 注入）。
pub type SaveConfigFn = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// 托盘「窗口列表」重建回调（pano-app 注入；窗口增删后调用）。
pub type RefreshTrayFn = Arc<dyn Fn() + Send + Sync>;

/// 运行时窗口注册表条目：窗口 id → 内容（标题 + 绑定的组件类型 id）。
#[derive(Debug, Clone)]
pub struct WindowEntry {
    /// 窗口标题（独立属性，切换组件不改变）。
    pub title: String,
    /// 绑定的 UI 组件类型 id（`UISpec.components` 之一；管理窗口为空串）。
    pub component: String,
}

/// pano-app 装配的共享状态。
pub struct AppState {
    /// 核心编排（适配器启停 / 配置 / 状态查询）。
    pub lifecycle: Arc<Mutex<Lifecycle>>,
    /// 样本存储（命令层快照读取）。
    pub store: Arc<SampleStore>,
    /// 窗口服务（窗口控制一律经此，命令层不接触 Tauri 窗口类型）。
    pub window_service: Arc<dyn WindowService>,
    /// 本 UI 的能力与组件目录（`components` = 组件类型清单；窗口 series 由此解析）。
    pub ui_spec: UISpec,
    /// pano.toml 路径（供展示）。
    pub config_path: PathBuf,
    /// 写回 pano.toml 的回调（热生效 / 窗口集合变化后调用）。
    pub save_config: SaveConfigFn,
    /// 窗口布局记忆（运行时内存，写回 pano.toml 由 pano-app 串行协调）。
    window_layouts: Arc<Mutex<HashMap<String, WindowLayout>>>,
    /// 运行时窗口注册表（管理窗口 + 监控窗口；「窗口管理」页与托盘共用）。
    windows: Arc<Mutex<HashMap<String, WindowEntry>>>,
    /// 托盘「窗口列表」重建回调（pano-app 在 build_tray 后注入）。
    refresh_tray: Arc<Mutex<Option<RefreshTrayFn>>>,
}

impl AppState {
    /// 构造共享状态。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lifecycle: Arc<Mutex<Lifecycle>>,
        store: Arc<SampleStore>,
        window_service: Arc<dyn WindowService>,
        ui_spec: UISpec,
        config_path: PathBuf,
        save_config: SaveConfigFn,
        window_layouts: Arc<Mutex<HashMap<String, WindowLayout>>>,
        windows: Arc<Mutex<HashMap<String, WindowEntry>>>,
    ) -> Self {
        Self {
            lifecycle,
            store,
            window_service,
            ui_spec,
            config_path,
            save_config,
            window_layouts,
            windows,
            refresh_tray: Arc::new(Mutex::new(None)),
        }
    }

    /// 锁定生命周期（毒锁降级，不 panic）。
    pub fn lifecycle(&self) -> std::sync::MutexGuard<'_, Lifecycle> {
        self.lifecycle.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 窗口布局记忆映射（pano-app / 命令层读写）。
    pub fn window_layouts(&self) -> &Arc<Mutex<HashMap<String, WindowLayout>>> {
        &self.window_layouts
    }

    /// 运行时窗口注册表（pano-app 启动播种 + 命令层增删改）。
    pub fn windows(&self) -> &Arc<Mutex<HashMap<String, WindowEntry>>> {
        &self.windows
    }

    /// 托盘「窗口列表」重建回调槽位（pano-app 注入；调用时若无注入则无操作）。
    pub fn refresh_tray(&self) -> &Arc<Mutex<Option<RefreshTrayFn>>> {
        &self.refresh_tray
    }

    /// 触发托盘「窗口列表」重建（无注入回调时静默跳过，如 headless 测试）。
    pub fn refresh_tray_now(&self) {
        if let Some(f) = self
            .refresh_tray
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            f();
        }
    }
}
