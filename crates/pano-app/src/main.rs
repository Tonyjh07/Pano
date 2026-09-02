//! Pano 二进制入口：组装 core + 窗口服务 + ui（Tauri v2）。
//!
//! 启动流程（架构 §7）：
//! 1. 按 feature 构建适配器注册表；
//! 2. 加载 `pano.toml` 配置；
//! 3. 组装 lifecycle（未知适配器 id 在此暴露）；
//! 4. 能力校验：UI 声明 ⊆ 已启用适配器的能力并集；
//! 5. 启动全部启用中的适配器；
//! 6. GUI：装配窗口服务（pano-window）+ Tauri（管理窗口 / 组件窗口 / 托盘 /
//!    事件桥接 / 布局持久化）；headless：无窗口跑固定时长后干净退出。
//!
//! 关闭顺序（架构 §5）：托盘「退出」→ 记录布局 → 写回配置 → 停止适配器 → 退出。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use pano_core::config::{PanoConfig, WindowLayout};
use pano_core::lifecycle::Lifecycle;
use pano_core::registry::Registry;
use pano_ui::state::{AppState, SaveConfigFn};
use pano_window::tauri::TauriWindowService;
use pano_window::{WindowService, persist};
use tauri::Manager;

/// Pano：可插拔监控面板。
#[derive(Parser, Debug)]
#[command(name = "pano", version, about = "Pano：可插拔监控面板")]
struct Cli {
    /// 配置文件路径（缺省为当前目录的 pano.toml）。
    #[arg(long, default_value = "pano.toml")]
    config: PathBuf,

    /// 无界面模式：启动后运行指定时长并退出（供无头验证 / CI）。
    #[arg(long)]
    headless: bool,

    /// headless 模式运行时长（秒）。
    #[arg(long, default_value_t = 3)]
    duration: u64,

    /// 日志追加写入指定文件。
    #[arg(long)]
    log_file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log_file.as_deref())?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("创建 tokio runtime 失败")?;

    // 1. 按 feature 构建适配器注册表
    let mut registry = Registry::new();
    for adapter in pano_adapters::build() {
        registry
            .register(adapter)
            .context("注册适配器失败（id 重复？）")?;
    }

    // 2. 加载配置（默认路径从 cwd 逐级向上解析，兼容 `cargo tauri dev` 的 cwd 差异）
    let config_path = resolve_config_path(&cli.config);
    tracing::debug!(target: "pano::app", path = %config_path.display(), "配置文件解析结果");
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("读取配置失败：{}", config_path.display()))?;
    let config = PanoConfig::parse(&text).context("解析配置失败")?;

    // 3. 组装 core（M1.2 无远程适配器，HttpClient 注入为 None；远程源 M2 装配）
    let lifecycle = Lifecycle::new(registry, config, rt.handle().clone())
        .context("组装 core 失败（配置引用了未注册的适配器？）")?;

    if cli.headless {
        return run_headless(lifecycle, cli.duration);
    }
    run_gui(lifecycle, config_path)
}

/// 解析配置文件路径：
/// - 显式路径（绝对路径或当前目录已存在）→ 原样使用；
/// - 默认 `pano.toml` 在当前目录不存在 → 从 cwd 逐级向上查找
///   （最多 6 个候选目录，含当前目录），兼容 `cargo tauri dev`
///   （cwd 可能落在 crates/ 下）等启动方式。
fn resolve_config_path(path: &std::path::Path) -> PathBuf {
    if path.is_absolute() || path.exists() {
        return path.to_path_buf();
    }
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..6 {
        let candidate = dir.join(path);
        if candidate.is_file() {
            return candidate;
        }
        if !dir.pop() {
            break;
        }
    }
    path.to_path_buf()
}

/// headless 模式：运行指定秒数，周期打印各 series 最新样本，随后干净退出。
fn run_headless(mut lifecycle: Lifecycle, seconds: u64) -> Result<()> {
    lifecycle.start_all();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    tracing::info!(target: "pano::app", seconds, "headless 模式启动");
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        for series in lifecycle.store().series_ids() {
            if let Some(latest) = lifecycle.store().latest(&series) {
                tracing::info!(target: "pano::app", series = %series, value = %latest.value, "样本");
            }
        }
    }
    lifecycle.stop_all();
    tracing::info!(target: "pano::app", "headless 模式结束");
    Ok(())
}

/// GUI 模式：Tauri 装配（能力校验 → 启动 → 窗口服务 → 命令 / 桥接 / 托盘）。
fn run_gui(lifecycle: Lifecycle, config_path: PathBuf) -> Result<()> {
    let ui_spec = pano_ui::uispec();
    let mut lifecycle = lifecycle;

    // 4. 能力校验（含 UI 组件声明自校验）
    lifecycle
        .validate_uispec(&ui_spec)
        .context("UI 能力校验失败")?;

    // 5. 启动全部启用中的适配器
    lifecycle.start_all();

    let core = Arc::new(Mutex::new(lifecycle));
    let setup_core = Arc::clone(&core);
    let store = {
        let lc = core.lock().unwrap_or_else(|e| e.into_inner());
        lc.store_arc()
    };

    // 6. Tauri 装配：命令注册（pano-ui 命令层）→ 窗口事件 → setup 建窗/桥接/托盘
    pano_ui::commands::register(tauri::Builder::default())
        .on_window_event(|window, event| {
            // 状态经 try_state 获取（setup 期间事件可能早于 manage，需容错）
            let Some(state) = window.app_handle().try_state::<AppState>() else {
                return;
            };
            match event {
                // 关闭 = 隐藏（不退出进程；退出仅经托盘「退出」，M1.1 语义延续）
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // 移动 / 缩放 → 更新布局记忆（内存；写回在退出 / 配置保存时）
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    if let Ok(handle) = state.window_service.handle(window.label())
                        && let Ok(geometry) = handle.geometry()
                    {
                        let mut guard = state
                            .window_layouts()
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        guard.insert(
                            window.label().to_string(),
                            persist::layout_from_geometry(&geometry),
                        );
                    }
                }
                _ => {}
            }
        })
        .setup(move |app| {
            // 6a. 装配共享状态与窗口服务（窗口实现只在 pano-window 内，见架构 §13）
            let window_service: Arc<dyn WindowService> =
                Arc::new(TauriWindowService::new(app.handle().clone()));
            let layouts: Arc<Mutex<HashMap<String, WindowLayout>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let save_config = build_save_config(&setup_core, &config_path, &layouts);
            app.manage(AppState::new(
                Arc::clone(&setup_core),
                store,
                Arc::clone(&window_service),
                ui_spec.clone(),
                config_path.clone(),
                save_config.clone(),
                Arc::clone(&layouts),
            ));

            // 6b. 固定创建管理窗口（ui.md §2：1 个，页签 = 适配器 + 窗口 + 设置）
            let manager_layout = setup_core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .config()
                .window_layout("manager")
                .cloned();
            window_service
                .create_window(
                    "manager",
                    &pano_ui::manager_window_spec(),
                    manager_layout.as_ref(),
                )
                .map_err(|e| anyhow::anyhow!("创建管理窗口失败：{e}"))?;
            // 管理窗口计入运行时注册表（窗口管理页 / 托盘「窗口列表」，不可销毁）
            {
                let state = app.state::<AppState>();
                let mut windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
                windows.insert(
                    "manager".to_string(),
                    pano_ui::state::WindowEntry {
                        title: pano_ui::manager_window_spec().title,
                        series: Vec::new(),
                    },
                );
            }

            // 6c. 按 components 创建监控组件窗口（仅当组件的 series 存在数据源，
            //     架构 §7「建窗时机」；无数据源组件不建窗），并计入运行时注册表
            for component in &ui_spec.components {
                let has_source = {
                    let lc = setup_core.lock().unwrap_or_else(|e| e.into_inner());
                    component
                        .series
                        .iter()
                        .all(|s| lc.config().is_enabled(s.adapter_id().as_str()))
                };
                if !has_source {
                    tracing::info!(
                        target: "pano::app",
                        component = %component.id,
                        "组件无数据源（对应适配器未启用），跳过建窗"
                    );
                    continue;
                }
                let layout = setup_core
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .config()
                    .window_layout(&component.id)
                    .cloned();
                window_service
                    .create_window(&component.id, &component.window, layout.as_ref())
                    .map_err(|e| anyhow::anyhow!("创建组件窗口 {} 失败：{e}", component.id))?;
                {
                    let state = app.state::<AppState>();
                    let mut windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
                    windows.insert(
                        component.id.clone(),
                        pano_ui::state::WindowEntry {
                            title: component.window.title.clone(),
                            series: component.series.clone(),
                        },
                    );
                }
            }

            // 6d. 事件桥接：SampleStore → 前端事件（架构 §4 / ui.md §6，无轮询）
            let bridge_store = app.state::<AppState>().store.clone();
            pano_ui::bridge::spawn_bridge(app.handle().clone(), bridge_store);

            // 6e. 托盘（主进程侧）：打开管理窗口 / 窗口列表（动态）/ 退出
            let refresh_tray = build_tray(app, &setup_core)?;
            // 注入托盘重建回调（窗口增删时命令层据此刷新「窗口列表」）
            {
                let state = app.state::<AppState>();
                *state
                    .refresh_tray()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(refresh_tray);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("Tauri 运行失败")?;

    // 正常退出路径兜底（托盘退出已先停适配器）
    if let Ok(mut lc) = core.lock() {
        lc.stop_all();
    }
    tracing::info!(target: "pano::app", "Pano 已退出");
    Ok(())
}

/// 配置持久化回调：段级合并窗口布局 → 序列化 → 写回文件（与热重载串行，架构 §13）。
fn build_save_config(
    core: &Arc<Mutex<Lifecycle>>,
    config_path: &std::path::Path,
    layouts: &Arc<Mutex<HashMap<String, WindowLayout>>>,
) -> SaveConfigFn {
    let core = Arc::clone(core);
    let layouts = Arc::clone(layouts);
    let path = config_path.to_path_buf();
    Arc::new(move || -> Result<(), String> {
        let mut lc = core.lock().map_err(|e| e.to_string())?;
        let snapshot = {
            let guard = layouts.lock().map_err(|e| e.to_string())?;
            guard.clone()
        };
        for (id, layout) in snapshot {
            lc.config_mut().set_window_layout(&id, Some(layout));
        }
        let text = lc.config().to_toml().map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("写回配置失败：{e}"))
    })
}

/// 构建系统托盘（失败降级为无托盘模式，不阻塞启动）。
///
/// 返回「窗口列表」重建回调：窗口增删后（窗口管理命令层）调用，
/// 按运行时注册表重建托盘菜单（Tauri `TrayIcon::set_menu` 热更新）。
fn build_tray(
    app: &tauri::App,
    core: &Arc<Mutex<Lifecycle>>,
) -> Result<Arc<dyn Fn() + Send + Sync>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};

    let open_manager = MenuItemBuilder::with_id("open_manager", "打开管理窗口").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    // 窗口列表（动态：随运行时注册表重建）
    let windows_submenu = build_window_submenu(app)?;

    let menu = MenuBuilder::new(app)
        .item(&open_manager)
        .item(&windows_submenu)
        .separator()
        .item(&quit)
        .build()?;

    let icon = include_bytes!("../../pano-ui/assets/pano_icon.png");
    let icon = tauri::image::Image::from_bytes(icon)
        .map_err(|e| anyhow::anyhow!("托盘图标解码失败：{e}"))?;

    let core = Arc::clone(core);
    let tray = tauri::tray::TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Pano 监控面板")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open_manager" => {
                let state = app.state::<AppState>();
                if let Ok(handle) = state.window_service.handle("manager") {
                    let _ = handle.show();
                    let _ = handle.focus();
                }
            }
            id if id.starts_with("window:") => {
                let label = &id["window:".len()..];
                let state = app.state::<AppState>();
                if let Ok(handle) = state.window_service.handle(label) {
                    let _ = handle.show();
                    let _ = handle.focus();
                }
            }
            "quit" => {
                // 关闭顺序（架构 §5）：记录布局 → 写回配置 → 停适配器 → 退出
                let state = app.state::<AppState>();
                for (label, window) in app.webview_windows() {
                    let _ = window.hide();
                    if let Ok(handle) = state.window_service.handle(&label)
                        && let Ok(geometry) = handle.geometry()
                    {
                        state
                            .window_layouts()
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(label.clone(), persist::layout_from_geometry(&geometry));
                    }
                }
                let _ = (state.save_config)();
                if let Ok(mut lc) = core.lock() {
                    lc.stop_all();
                }
                app.exit(0);
            }
            _ => {}
        })
        .build(app)
        .context("创建托盘失败（将降级为无托盘模式）")?;

    // 重建回调：读运行时注册表重建「窗口列表」子菜单
    let app_handle = app.handle().clone();
    let tray_handle = tray;
    let refresh = Arc::new(move || {
        let state = app_handle.state::<AppState>();
        let submenu = build_window_submenu_from_registry(&app_handle, &state);
        match submenu {
            Ok(submenu) => {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                let open_manager =
                    MenuItemBuilder::with_id("open_manager", "打开管理窗口").build(&app_handle);
                let quit = MenuItemBuilder::with_id("quit", "退出").build(&app_handle);
                let (open_manager, quit) = match (open_manager, quit) {
                    (Ok(o), Ok(q)) => (o, q),
                    _ => return,
                };
                let menu = MenuBuilder::new(&app_handle)
                    .item(&open_manager)
                    .item(&submenu)
                    .separator()
                    .item(&quit)
                    .build();
                if let Ok(menu) = menu {
                    let _ = tray_handle.set_menu(Some(menu));
                }
            }
            Err(error) => {
                tracing::warn!(target: "pano::app", error = %error, "重建托盘窗口列表失败");
            }
        }
    });
    Ok(refresh)
}

/// 按运行时注册表构建「窗口列表」子菜单（启动建托盘 + 动态重建共用）。
fn build_window_submenu(app: &tauri::App) -> tauri::Result<tauri::menu::Submenu<tauri::Wry>> {
    let state = app.state::<AppState>();
    build_window_submenu_from_registry(app.handle(), &state)
}
/// 由注册表构建「窗口列表」子菜单的具体实现（AppHandle 版，动态重建用）。
fn build_window_submenu_from_registry<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &tauri::State<'_, AppState>,
) -> tauri::Result<tauri::menu::Submenu<R>> {
    use tauri::menu::{MenuItemBuilder, SubmenuBuilder};
    let windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
    let mut builder = SubmenuBuilder::new(app, "窗口列表");
    for (id, entry) in windows.iter() {
        builder = builder
            .item(&MenuItemBuilder::with_id(format!("window:{id}"), &entry.title).build(app)?);
    }
    let submenu = builder.build()?;
    Ok(submenu)
}

fn init_tracing(log_file: Option<&std::path::Path>) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt()
        .with_target(true)
        .with_env_filter(filter);
    match log_file {
        Some(path) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("打开日志文件失败：{}", path.display()))?;
            builder.with_writer(file).init();
        }
        None => builder.init(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// set_current_dir 是进程级全局状态：涉及改 cwd 的用例串行执行。
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 在临时目录树中构造：root / a / b，root 下放 pano.toml；
    /// 从 b 目录调用时默认路径应向上解析到 root。
    #[test]
    fn resolve_config_path_walks_up_to_ancestor() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("pano-test-{}", std::process::id()));
        let a = root.join("a");
        let b = a.join("b");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(root.join("pano.toml"), "schema_version = 1\n").unwrap();

        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&b).unwrap();
        let resolved = resolve_config_path(std::path::Path::new("pano.toml"));
        std::env::set_current_dir(prev).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(resolved, root.join("pano.toml"), "应从 b 向上解析到 root");
    }

    #[test]
    fn resolve_config_path_absolute_and_existing_passthrough() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("pano-test-abs-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("cfg.toml");
        std::fs::write(&file, "schema_version = 1\n").unwrap();

        // 绝对路径（无论是否存在）原样返回
        let abs = resolve_config_path(&file);
        assert_eq!(abs, file);
        // 已存在的相对路径原样返回（不向上找）
        let rel = resolve_config_path(std::path::Path::new("Cargo.toml"));
        assert!(rel.is_file(), "cwd 下的 Cargo.toml 应原样命中：{rel:?}");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resolve_config_path_fallback_when_not_found() {
        // 显式指定的不存在路径（非默认）兜底返回原路径
        let p = std::path::Path::new("definitely-not-exists-xyz.toml");
        let resolved = resolve_config_path(p);
        assert_eq!(resolved, p.to_path_buf());
    }
}
