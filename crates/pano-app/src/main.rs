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
use pano_core::config::{PanoConfig, WindowConfig, WindowLayout};
use pano_core::lifecycle::Lifecycle;
use pano_core::registry::Registry;
use pano_ui::state::{AppState, SaveConfigFn, WindowEntry};
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

    // 3. 组装 core（远程适配器注入 HttpClient，架构 §14 / roadmap M2.5）：
    //    `adapter-deepseek-balance` 等远程适配器需要注入的 HTTP 客户端
    //    （R3 预留首次落地，pano-app 装配层按 feature 构造）；其余分支保持
    //    http=None，行为与既有版本一致。
    #[cfg(feature = "adapter-deepseek-balance")]
    let http: Option<Arc<dyn pano_core::http::HttpClient>> = Some(Arc::new(
        pano_adapters::remote::http_poll::ReqwestHttpClient::new()
            .context("创建 HTTP 客户端失败（adapter-deepseek-balance）")?,
    ));
    #[cfg(not(feature = "adapter-deepseek-balance"))]
    let http: Option<Arc<dyn pano_core::http::HttpClient>> = None;

    let lifecycle = Lifecycle::with_http(registry, config, rt.handle().clone(), http)
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
                // M2.4：保留窗口级无边框覆盖（layout_with_geometry）。
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    if let Ok(handle) = state.window_service.handle(window.label())
                        && let Ok(geometry) = handle.geometry()
                    {
                        let mut guard = state
                            .window_layouts()
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        // M2.4：保留窗口级无边框覆盖（layout_with_geometry）
                        let existing = guard.get(window.label()).cloned();
                        guard.insert(
                            window.label().to_string(),
                            persist::layout_with_geometry(existing.as_ref(), &geometry),
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
            // 运行时窗口注册表（「当前存在哪些窗口」唯一来源，M2.1/M2.2）
            let windows: Arc<Mutex<HashMap<String, WindowEntry>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let save_config = build_save_config(&setup_core, &config_path, &layouts, &windows);
            app.manage(AppState::new(
                Arc::clone(&setup_core),
                store,
                Arc::clone(&window_service),
                ui_spec.clone(),
                config_path.clone(),
                save_config.clone(),
                Arc::clone(&layouts),
                Arc::clone(&windows),
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
            if let Some(layout) = &manager_layout {
                remember_layout(&layouts, "manager", layout);
            }
            {
                let state = app.state::<AppState>();
                let mut windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
                windows.insert(
                    "manager".to_string(),
                    WindowEntry {
                        title: pano_ui::manager_window_spec().title,
                        component: String::new(),
                    },
                );
            }

            // 6c. 监控窗口：按持久化窗口集合创建（[ui].windows，M2.2）；
            //     段缺失（首次运行）→ 按组件目录播种（仅对应适配器已启用的组件），
            //     随后写回窗口集合；已存在则按 id 恢复（组件绑定 + 布局 + 标题）。
            let first_run = {
                let lc = setup_core.lock().unwrap_or_else(|e| e.into_inner());
                lc.config().ui_windows().is_none()
            };
            let persisted_ids: Vec<String> = {
                let lc = setup_core.lock().unwrap_or_else(|e| e.into_inner());
                lc.config().ui_windows().map(|v| v.to_vec()).unwrap_or_default()
            };

            if first_run {
                // 首次运行：按组件目录播种（仅组件 series 对应适配器已启用）
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
                            "组件无数据源（对应适配器未启用），首次播种跳过"
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
                        // M2.4 B1：预填布局记忆（保留窗口级覆盖不被几何刷新冲刷）
                        if let Some(layout) = &layout {
                            remember_layout(state.window_layouts(), &component.id, layout);
                        }
                        let mut windows =
                            state.windows().lock().unwrap_or_else(|e| e.into_inner());
                        windows.insert(
                            component.id.clone(),
                            WindowEntry {
                                title: component.window.title.clone(),
                                component: component.id.clone(),
                            },
                        );
                    }
                }
                // 写回窗口集合（[ui].windows + [window.<id>] 组件绑定）
                save_config().map_err(|e| anyhow::anyhow!("首次播种写回窗口集合失败：{e}"))?;
            } else {
                // 持久化集合恢复：逐 id 建窗（组件须仍在目录中，否则跳过 + warn）
                for id in &persisted_ids {
                    let (component, title) = {
                        let lc = setup_core.lock().unwrap_or_else(|e| e.into_inner());
                        let cfg = lc.config().window_config(id).cloned();
                        (
                            cfg.as_ref().and_then(|c| c.component.clone()),
                            cfg.as_ref().and_then(|c| c.title.clone()),
                        )
                    };
                    let Some(component) = component else {
                        tracing::warn!(target: "pano::app", window = %id, "持久化窗口缺组件绑定，跳过");
                        continue;
                    };
                    let Some(comp) = ui_spec.components.iter().find(|c| c.id == component) else {
                        tracing::warn!(target: "pano::app", window = %id, component = %component,
                            "持久化窗口绑定的组件不在组件目录，跳过");
                        continue;
                    };
                    // 未注册（feature 未编译）的组件不建窗：与命令层 / 列表置灰语义一致
                    let available = {
                        let lc = setup_core.lock().unwrap_or_else(|e| e.into_inner());
                        pano_ui::commands::component_available(comp, |aid| {
                            lc.adapter_meta(aid).is_some()
                        })
                    };
                    if !available {
                        tracing::warn!(target: "pano::app", window = %id, component = %component,
                            "持久化窗口绑定的组件适配器未注册（feature 关闭？），跳过");
                        continue;
                    }
                    let layout = setup_core
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .config()
                        .window_layout(id)
                        .cloned();
                    let title = title.unwrap_or_else(|| comp.window.title.clone());
                    let spec = pano_core::capability::WindowSpec {
                        title: title.clone(),
                        ..comp.window.clone()
                    };
                    window_service
                        .create_window(id, &spec, layout.as_ref())
                        .map_err(|e| anyhow::anyhow!("恢复监控窗口 {id} 失败：{e}"))?;
                    {
                        let state = app.state::<AppState>();
                        // M2.4 B1：预填布局记忆（保留窗口级覆盖不被几何刷新冲刷）
                        if let Some(layout) = &layout {
                            remember_layout(state.window_layouts(), id, layout);
                        }
                        let mut windows =
                            state.windows().lock().unwrap_or_else(|e| e.into_inner());
                        windows.insert(id.clone(), WindowEntry { title, component });
                    }
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

/// 建窗后把配置布局预填进运行时布局记忆（M2.4，B1 修复）。
///
/// 使「保留窗口级覆盖」（`persist::layout_with_geometry` 的 `existing`）重启后
/// 恒有值——否则内存布局记忆以空表启动，重启后首次几何刷新（移动 / 缩放 /
/// 退出）会以 `existing=None` 生成 `decorations: None`，把 `[window.<id>].
/// decorations` 覆盖从配置中冲刷掉（手动设置的无边框丢失，见审查 B1）。
fn remember_layout(
    layouts: &Arc<Mutex<HashMap<String, WindowLayout>>>,
    id: &str,
    layout: &WindowLayout,
) {
    layouts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.to_string(), layout.clone());
}

/// 配置持久化回调：段级合并窗口集合（注册表：组件绑定 + 标题）与布局 → 序列化 → 写回文件。
///
/// 与热重载写文件串行协调（架构 §13）；运行时窗口增删 / 切换组件后由命令层
/// 经此写回 `[ui].windows` 与 `[window.<id>]` 段（M2.2 窗口集合持久化）。
fn build_save_config(
    core: &Arc<Mutex<Lifecycle>>,
    config_path: &std::path::Path,
    layouts: &Arc<Mutex<HashMap<String, WindowLayout>>>,
    windows: &Arc<Mutex<HashMap<String, WindowEntry>>>,
) -> SaveConfigFn {
    let core = Arc::clone(core);
    let layouts = Arc::clone(layouts);
    let windows = Arc::clone(windows);
    let path = config_path.to_path_buf();
    Arc::new(move || -> Result<(), String> {
        let mut lc = core.lock().map_err(|e| e.to_string())?;
        let layouts_snapshot = {
            let guard = layouts.lock().map_err(|e| e.to_string())?;
            guard.clone()
        };
        let windows_snapshot = {
            let guard = windows.lock().map_err(|e| e.to_string())?;
            guard.clone()
        };
        // 窗口段以运行时注册表为权威：组件绑定 + 标题 + 布局
        for (id, entry) in &windows_snapshot {
            let layout = layouts_snapshot.get(id).cloned().unwrap_or_default();
            lc.config_mut().set_window_config(
                id,
                Some(WindowConfig {
                    component: (!entry.component.is_empty()).then(|| entry.component.clone()),
                    title: Some(entry.title.clone()),
                    layout,
                }),
            );
        }
        // 清理配置中已不存在的窗口段（已销毁窗口的死配置，防脏数据残留）
        let stale_ids: Vec<String> = lc
            .config()
            .windows
            .keys()
            .filter(|id| !windows_snapshot.contains_key(*id))
            .cloned()
            .collect();
        for id in stale_ids {
            lc.config_mut().set_window_config(&id, None);
        }
        // [ui].windows = 监控窗口 id 列表（不含固定管理窗口）
        let mut ids: Vec<String> = windows_snapshot
            .keys()
            .filter(|k| k.as_str() != "manager")
            .cloned()
            .collect();
        ids.sort();
        lc.config_mut().set_ui_windows(ids);
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
                        let mut layouts = state
                            .window_layouts()
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        // M2.4：保留窗口级无边框覆盖（layout_with_geometry）
                        let existing = layouts.get(&label).cloned();
                        layouts.insert(
                            label.clone(),
                            persist::layout_with_geometry(existing.as_ref(), &geometry),
                        );
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
    fn remember_layout_seeds_window_decorations_override() {
        // M2.4 B1 回归：建窗后预填布局记忆，重启后首次几何刷新（layout_with_geometry）
        // 的 existing 恒有值 → `[window.<id>].decorations` 覆盖不被冲刷为 None。
        let layouts: Arc<Mutex<HashMap<String, WindowLayout>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let override_layout = WindowLayout {
            position: Some((1.0, 2.0)),
            size: None,
            monitor: None,
            decorations: Some(false), // 手动切无边框（窗口级覆盖）
        };
        remember_layout(&layouts, "sys-cpu", &override_layout);
        assert_eq!(
            layouts
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get("sys-cpu")
                .and_then(|l| l.decorations),
            Some(false)
        );

        // 几何刷新保留覆盖（等同重启后第一次移动窗口的路径）
        let geometry = pano_window::WindowGeometry {
            position: (9.0, 9.0),
            size: (400.0, 300.0),
            monitor: pano_window::MonitorId::new("primary"),
        };
        let existing = layouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("sys-cpu")
            .cloned();
        let refreshed = pano_window::persist::layout_with_geometry(existing.as_ref(), &geometry);
        assert_eq!(refreshed.decorations, Some(false), "覆盖不被几何刷新冲刷");
        assert_eq!(refreshed.position, Some((9.0, 9.0)), "几何本身更新");
    }

    #[test]
    fn resolve_config_path_fallback_when_not_found() {
        // 显式指定的不存在路径（非默认）兜底返回原路径
        let p = std::path::Path::new("definitely-not-exists-xyz.toml");
        let resolved = resolve_config_path(p);
        assert_eq!(resolved, p.to_path_buf());
    }
}
