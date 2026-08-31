//! pano-app 注入的共享状态与回调（M1.2 命令层）。
//!
//! `AppState` 由 pano-app 装配并放入 Tauri managed state；命令层经
//! `tauri::State<AppState>` 访问。文件写入（pano.toml）留在 pano-app
//! 侧经 `save_config` 回调注入，保证与 core 热重载写文件串行协调
//! （架构 §13：段级合并，避免并发写）。

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

/// pano-app 装配的共享状态。
pub struct AppState {
    /// 核心编排（适配器启停 / 配置 / 状态查询）。
    pub lifecycle: Arc<Mutex<Lifecycle>>,
    /// 样本存储（命令层快照读取）。
    pub store: Arc<SampleStore>,
    /// 窗口服务（窗口控制一律经此，命令层不接触 Tauri 窗口类型）。
    pub window_service: Arc<dyn WindowService>,
    /// 本 UI 的能力与组件声明（组件 → series 映射的唯一来源）。
    pub ui_spec: UISpec,
    /// pano.toml 路径（供展示）。
    pub config_path: PathBuf,
    /// 写回 pano.toml 的回调（热生效成功后调用）。
    pub save_config: SaveConfigFn,
    /// 窗口布局记忆（运行时内存，写回 pano.toml 由 pano-app 串行协调）。
    window_layouts: Arc<Mutex<HashMap<String, WindowLayout>>>,
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
    ) -> Self {
        Self {
            lifecycle,
            store,
            window_service,
            ui_spec,
            config_path,
            save_config,
            window_layouts,
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
}
