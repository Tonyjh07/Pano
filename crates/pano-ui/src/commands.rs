//! Tauri 命令层（ui.md §9）：前端 `invoke` → 命令 → core / 窗口服务。
//!
//! 约束：
//! - 窗口控制一律经 [`pano_window::WindowService`]，**不直接操作 Tauri 窗口类型**；
//! - 启停 / 配置修改走 **async 命令**（运行于 tauri 异步运行时，不卡 UI 主线程）；
//! - 配置修改流程：core 热生效（失败回滚）→ 成功才经 `save_config` 写回 pano.toml。

use pano_core::adapter::{AdapterId, SeriesId};
use pano_core::capability::{ComponentSpec, UISpec, is_valid_component_id};
use pano_core::config::{AdapterConfig, SCHEMA_VERSION};
use tauri::State;

use crate::dto::{
    AdapterInfo, ComponentInfo, ConfigValueDto, FieldInfo, MonitorDto, SampleEventDto, StatusDto,
    WindowContentDto, WindowInfoDto,
};
use crate::state::{AppState, WindowEntry};

/// 适配器列表（管理页表格，schema 驱动表单渲染；含当前自定义配置值）。
#[tauri::command]
pub fn list_adapters(state: State<'_, AppState>) -> Vec<AdapterInfo> {
    let lc = state.lifecycle();
    let mut out = Vec::new();
    for id in lc.adapter_ids() {
        let meta = lc.adapter_meta(&id);
        let caps = lc.capabilities_of(&id).unwrap_or_default();
        let series = lc.series_of(&id).unwrap_or_default();
        let schema = lc.config_schema_of(&id).unwrap_or_default();
        let status = lc.status_of(&id);
        let sampling = lc.config().sampling_ms_of(id.as_str());
        let enabled = lc.config().is_enabled(id.as_str());
        // 当前自定义配置值（M2.3：high_threshold / link_mbps 等；custom_config
        // 整体解析失败时忽略该适配器的自定义配置值，仅影响阈值回落默认）
        let mut config: Vec<ConfigValueDto> = lc
            .config()
            .custom_config(id.as_str())
            .unwrap_or_default()
            .into_iter()
            .map(ConfigValueDto::from)
            .collect();
        // 按 key 排序，保证 list_adapters 输出顺序跨调用稳定（前端按 key 查找）
        config.sort_by(|a, b| a.key.cmp(&b.key));
        out.push(AdapterInfo::new(
            id.as_str(),
            meta.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
            meta.as_ref()
                .map(|m| m.description.clone())
                .unwrap_or_default(),
            meta.as_ref().map(|m| m.version.clone()).unwrap_or_default(),
            caps.iter().map(|c| c.to_string()).collect(),
            series.iter().map(|s| s.as_str().to_string()).collect(),
            &status,
            enabled,
            sampling,
            schema.fields.iter().map(FieldInfo::from).collect(),
            config,
        ));
    }
    out
}

/// 单个适配器状态。
#[tauri::command]
pub fn adapter_status(state: State<'_, AppState>, id: String) -> Result<StatusDto, String> {
    let lc = state.lifecycle();
    let adapter_id = AdapterId::new(id.as_str());
    let status = lc.status_of(&adapter_id);
    Ok(StatusDto::from_status(&status))
}

/// 启用 / 停用适配器（热生效 + 写回文件）。
#[tauri::command]
pub async fn set_adapter_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    apply_config(&state, id.as_str(), |cfg| cfg.enabled = enabled)
}

/// 修改采样周期（毫秒，热生效 + 写回文件）。
#[tauri::command]
pub async fn set_adapter_sampling(
    state: State<'_, AppState>,
    id: String,
    sampling_ms: u64,
) -> Result<(), String> {
    apply_config(&state, id.as_str(), |cfg| cfg.sampling = Some(sampling_ms))
}

/// 手动重启适配器（Error 状态恢复用）。
#[tauri::command]
pub async fn restart_adapter(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let adapter_id = AdapterId::new(id.as_str());
    let mut lc = state.lifecycle();
    lc.stop(&adapter_id).map_err(|e| e.to_string())?;
    lc.start(&adapter_id).map_err(|e| e.to_string())
}

/// UI 组件目录（「窗口管理」页选组件用，ui.md §3.2）。
///
/// 返回全部组件并附 `available`：全部 series 所属适配器已注册为可用；
/// 未注册（feature 未编译）的组件前端置灰不可选。
#[tauri::command]
pub fn list_components(state: State<'_, AppState>) -> Vec<ComponentInfo> {
    let lc = state.lifecycle();
    state
        .ui_spec
        .components
        .iter()
        .map(|c| {
            let available = component_available(c, |aid| lc.adapter_meta(aid).is_some());
            ComponentInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                series: c.series.iter().map(|s| s.as_str().to_string()).collect(),
                available,
            }
        })
        .collect()
}

/// 某窗口当前内容（component + series；组件窗口前端按窗口 label 查询，ui.md §4）。
///
/// 窗口的 series 由绑定的组件类型在目录中解析（窗口不再直接存 series，M2.2）。
/// M2.4：附带当前无边框状态（前端据此决定内容区拖拽移动）。
#[tauri::command]
pub fn window_content(state: State<'_, AppState>, id: String) -> Result<WindowContentDto, String> {
    let windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
    let entry = windows
        .get(&id)
        .ok_or_else(|| format!("窗口不存在：{id}"))?;
    let series = component_series_of(&state.ui_spec, &entry.component);
    let decorations = state
        .window_service
        .handle(&id)
        .and_then(|h| h.is_decorated())
        .unwrap_or(true);
    Ok(WindowContentDto {
        component: entry.component.clone(),
        series: series.iter().map(|s| s.as_str().to_string()).collect(),
        decorations,
    })
}

/// 某 series 最近至多 `window` 个样本（组件窗口首次渲染拉取）。
#[tauri::command]
pub fn series_history(
    state: State<'_, AppState>,
    series: String,
    window: Option<usize>,
) -> Vec<SampleEventDto> {
    let sid = SeriesId::parse(series.as_str());
    state
        .store
        .history(&sid, window.unwrap_or(300))
        .iter()
        .map(|s| SampleEventDto::from_sample(&sid, s))
        .collect()
}

/// 某 series 最新样本。
#[tauri::command]
pub fn series_latest(state: State<'_, AppState>, series: String) -> Option<SampleEventDto> {
    let sid = SeriesId::parse(series.as_str());
    state
        .store
        .latest(&sid)
        .map(|s| SampleEventDto::from_sample(&sid, &s))
}

/// 窗口：全屏切换。
#[tauri::command]
pub fn window_set_fullscreen(
    state: State<'_, AppState>,
    label: String,
    enabled: bool,
) -> Result<(), String> {
    tracing::debug!(target: "pano::ui", label = %label, enabled, "窗口全屏切换");
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .set_fullscreen(enabled)
        .map_err(|e| e.to_string())
}

/// 窗口：置顶切换。
#[tauri::command]
pub fn window_set_always_on_top(
    state: State<'_, AppState>,
    label: String,
    enabled: bool,
) -> Result<(), String> {
    tracing::debug!(target: "pano::ui", label = %label, enabled, "窗口置顶切换");
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .set_always_on_top(enabled)
        .map_err(|e| e.to_string())
}

/// 窗口：绑定显示器。
#[tauri::command]
pub fn window_set_monitor(
    state: State<'_, AppState>,
    label: String,
    monitor_id: String,
) -> Result<(), String> {
    tracing::debug!(target: "pano::ui", label = %label, monitor = %monitor_id, "窗口绑定显示器");
    let monitor = pano_window::monitor::MonitorId::new(monitor_id);
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .set_monitor(&monitor)
        .map_err(|e| e.to_string())
}

/// 窗口：无边框切换（M2.4，管理页「无边框」开关）。
///
/// 切换实际边框状态 + 持久化窗口级覆盖（`[window.<id>].decorations`），
/// 并定向 emit 给目标窗口，前端据此更新内容区拖拽判定。
#[tauri::command]
pub async fn window_set_decorations<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    label: String,
    enabled: bool,
) -> Result<(), String> {
    tracing::debug!(target: "pano::ui", label = %label, enabled, "窗口无边框切换");
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .set_decorations(enabled)
        .map_err(|e| e.to_string())?;
    // 持久化窗口级覆盖（布局段，M2.4）
    state
        .window_layouts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(label.clone())
        .or_default()
        .decorations = Some(enabled);
    // 通知该窗口前端更新拖拽判定（先于写回：边框已实际切换，写回失败也不
    // 让前端拖拽判定失步——审查 S2）
    use tauri::Emitter;
    let _ = app.emit_to(
        tauri::EventTarget::labeled(&label),
        crate::bridge::EVENT_WINDOW_DECORATIONS,
        enabled,
    );
    (state.save_config)().map_err(|e| format!("无边框设置写回失败：{e}"))?;
    Ok(())
}

/// 窗口：设置位置（逻辑像素）。
#[tauri::command]
pub fn window_set_position(
    state: State<'_, AppState>,
    label: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .set_position(x, y)
        .map_err(|e| e.to_string())
}

/// 窗口：设置大小（逻辑像素）。
#[tauri::command]
pub fn window_set_size(
    state: State<'_, AppState>,
    label: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .set_size(width, height)
        .map_err(|e| e.to_string())
}

/// 窗口：聚焦。
#[tauri::command]
pub fn window_focus(state: State<'_, AppState>, label: String) -> Result<(), String> {
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .focus()
        .map_err(|e| e.to_string())
}

/// 窗口：隐藏（关闭语义，不退出进程）。
#[tauri::command]
pub fn window_hide(state: State<'_, AppState>, label: String) -> Result<(), String> {
    state
        .window_service
        .handle(&label)
        .map_err(|e| e.to_string())?
        .hide()
        .map_err(|e| e.to_string())
}

/// 窗口：显示并聚焦。
#[tauri::command]
pub fn window_show(state: State<'_, AppState>, label: String) -> Result<(), String> {
    state.window_service.show(&label).map_err(|e| e.to_string())
}

/// 窗口列表（「窗口管理」页 / 托盘「窗口列表」共用）。
///
/// 返回运行时注册表中的全部窗口（含固定管理窗口），附可见状态与绑定的组件
/// （series 由组件目录解析，供前端展示）。
#[tauri::command]
pub fn list_windows(state: State<'_, AppState>) -> Vec<WindowInfoDto> {
    let mut out: Vec<WindowInfoDto> = Vec::new();
    let windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
    for (id, entry) in windows.iter() {
        let series = component_series_of(&state.ui_spec, &entry.component);
        out.push(WindowInfoDto {
            id: id.clone(),
            title: entry.title.clone(),
            component: entry.component.clone(),
            series: series.iter().map(|s| s.as_str().to_string()).collect(),
            is_manager: id == "manager",
            visible: state
                .window_service
                .is_visible(id)
                .unwrap_or_else(|_| state.window_service.exists(id)),
            // M2.4：当前无边框状态（管理页「无边框」开关显示）
            decorations: state
                .window_service
                .handle(id)
                .and_then(|h| h.is_decorated())
                .unwrap_or(true),
        });
    }
    out.sort_by(|a, b| {
        // 管理窗口置顶，其余按 id 排序
        let am = a.is_manager as u8;
        let bm = b.is_manager as u8;
        bm.cmp(&am).then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// 新建监控窗口并绑定一个 UI 组件（「分配组件」，ui.md §3.2）。
///
/// 校验：id 满足全小写 ASCII 连字符、全局唯一（不与管理窗口冲突）；
/// 组件在目录中存在且全部 series 所属适配器已注册（已注册但未启用的适配器
/// 允许建窗，窗口显示空态）。标题缺省取组件默认标题，可经 `title` 覆盖。
/// 成功后将窗口加入运行时注册表、持久化窗口集合并触发托盘「窗口列表」重建。
///
/// **必须为 async 命令**：Windows 上在同步命令/事件处理器中创建 WebView
/// 会死锁（Tauri 文档，[wry#583](https://github.com/tauri-apps/wry/issues/583)）。
#[tauri::command]
pub async fn create_window(
    state: State<'_, AppState>,
    id: String,
    component: String,
    title: Option<String>,
) -> Result<(), String> {
    if !is_valid_component_id(&id) {
        return Err(format!(
            "窗口 id 格式非法（应为全小写 ASCII、连字符分隔）：{id}"
        ));
    }
    if id == "manager" {
        return Err("manager 为保留窗口 id".into());
    }
    let comp = {
        let lc = state.lifecycle();
        validate_component_binding(&state.ui_spec, &component, |aid| {
            lc.adapter_meta(aid).is_some()
        })?
    };
    let title = title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| comp.window.title.clone());
    let spec = pano_core::capability::WindowSpec {
        title: title.clone(),
        ..comp.window.clone()
    };
    let layout = state
        .window_layouts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned();
    state
        .window_service
        .create_window(&id, &spec, layout.as_ref())
        .map_err(|e| e.to_string())?;
    let mut windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
    windows.insert(id.clone(), WindowEntry { title, component });
    drop(windows);
    // 持久化窗口集合（[ui].windows + [window.<id>]）
    (state.save_config)().map_err(|e| format!("窗口集合写回失败：{e}"))?;
    state.refresh_tray_now();
    tracing::info!(target: "pano::ui", id, "新建监控窗口（绑定组件）");
    Ok(())
}

/// 切换窗口绑定的 UI 组件（「切换组件」，ui.md §3.2）。
///
/// 仅换内容 / series（标题与几何不动），同步更新运行时注册表、持久化窗口集合，
/// 并向该窗口 emit 内容变更事件，前端据此重载。
#[tauri::command]
pub async fn set_window_component<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    component: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if id == "manager" {
        return Err("管理窗口不支持切换组件".into());
    }
    {
        let lc = state.lifecycle();
        validate_component_binding(&state.ui_spec, &component, |aid| {
            lc.adapter_meta(aid).is_some()
        })?;
    }
    let mut windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
    let entry = windows
        .get_mut(&id)
        .ok_or_else(|| format!("窗口不存在：{id}"))?;
    entry.component = component.clone();
    drop(windows);
    // 持久化窗口集合
    (state.save_config)().map_err(|e| format!("窗口集合写回失败：{e}"))?;
    // 通知该窗口前端重载内容（若窗口正打开）
    let series = component_series_of(&state.ui_spec, &component);
    use tauri::Emitter;
    let payload = WindowContentDto {
        component: component.clone(),
        series: series.iter().map(|s| s.as_str().to_string()).collect(),
        // M2.4：附带当前无边框状态（切换组件不改变边框）
        decorations: state
            .window_service
            .handle(&id)
            .and_then(|h| h.is_decorated())
            .unwrap_or(true),
    };
    let _ = app.emit_to(
        tauri::EventTarget::labeled(&id),
        crate::bridge::EVENT_WINDOW_COMPONENT,
        payload,
    );
    tracing::info!(target: "pano::ui", id, component = %component, "切换窗口组件");
    Ok(())
}

/// 销毁监控窗口（彻底关闭并从窗口管理器移除；管理窗口不可销毁）。
///
/// 同步移除运行时注册表条目、窗口布局记忆，持久化窗口集合并触发托盘「窗口列表」重建。
#[tauri::command]
pub fn destroy_window(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if id == "manager" {
        return Err("管理窗口不可销毁".into());
    }
    let mut windows = state.windows().lock().unwrap_or_else(|e| e.into_inner());
    if windows.remove(&id).is_none() {
        return Err(format!("窗口不存在：{id}"));
    }
    drop(windows);
    state
        .window_layouts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    state
        .window_service
        .destroy(&id)
        .map_err(|e| e.to_string())?;
    // 持久化窗口集合（移除该窗口）
    (state.save_config)().map_err(|e| format!("窗口集合写回失败：{e}"))?;
    state.refresh_tray_now();
    tracing::info!(target: "pano::ui", id, "销毁监控窗口");
    Ok(())
}

/// 枚举显示器（窗口控制菜单）。
#[tauri::command]
pub fn monitors(state: State<'_, AppState>) -> Result<Vec<MonitorDto>, String> {
    state
        .window_service
        .monitors()
        .map(|list| list.iter().map(MonitorDto::from).collect())
        .map_err(|e| e.to_string())
}

/// 当前配置文本（设置页查看 pano.toml 用）。
///
/// M2.5 安全点：`[adapters.*]` 段内的密钥类键（如 `api_key`）读回前端一律
/// 掩码——与 `list_adapters` 的 [`crate::dto::MASKED_CONFIG_KEYS`] 同一安全
/// 承诺（密钥不进 WebView；`pano.toml` 仍存明文供适配器使用）。
#[tauri::command]
pub fn config_preview(state: State<'_, AppState>) -> Result<String, String> {
    let lc = state.lifecycle();
    let toml_text = lc
        .config()
        .to_toml()
        .map_err(|e| format!("配置序列化失败：{e}"))?;
    Ok(mask_secret_lines(&toml_text))
}

/// 对 pano.toml 文本做密钥类行掩码：仅掩码 `[adapters.*]` 段内的
/// `api_key = "…"` 赋值行（`to_toml` 序列化格式），其余段落原样保留。
fn mask_secret_lines(toml_text: &str) -> String {
    let mut in_adapter_section = false;
    let mut out = String::with_capacity(toml_text.len());
    for line in toml_text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            // 段头（含 `[adapters."deepseek.balance"]` 引号形式）
            in_adapter_section = trimmed.starts_with("[adapters");
            out.push_str(line);
        } else if in_adapter_section && is_secret_key_line(trimmed) {
            out.push_str(&mask_secret_value(line));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// 行是否为密钥类键赋值（`api_key = …`；to_toml 序列化固定带空格）。
fn is_secret_key_line(trimmed: &str) -> bool {
    trimmed.starts_with("api_key =") || trimmed.starts_with("api_key=")
}

/// 掩码 `key = value` 行的值部分：保留键与等号，值替换为掩码串。
fn mask_secret_value(line: &str) -> String {
    match line.find('=') {
        // `line[..=pos]` 形如 `api_key =`；trim_end 去掉尾部空格后补 ` "********"`
        Some(pos) => format!("{} \"********\"", line[..=pos].trim_end()),
        None => line.to_string(),
    }
}

/// 配置 schema 版本（设置页展示）。
#[tauri::command]
pub fn config_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// 应用配置修改：core 热生效（含失败回滚）→ 成功才写回文件。
fn apply_config(
    state: &AppState,
    id: &str,
    mutate: impl FnOnce(&mut AdapterConfig),
) -> Result<(), String> {
    let adapter_id = AdapterId::new(id);
    let lc = state.lifecycle();
    let current = lc.config().adapters.get(id);
    let new_cfg = next_adapter_config(current, mutate);
    drop(lc);
    tracing::debug!(target: "pano::ui", adapter = %id, enabled = new_cfg.enabled, sampling = ?new_cfg.sampling, "应用适配器配置（热生效）");
    let mut lc = state.lifecycle();
    lc.apply_adapter_config(&adapter_id, new_cfg)
        .map_err(|e| e.to_string())?;
    // 热生效成功后才写回文件；失败时 core 已回滚，不得写回（架构 §8）。
    drop(lc);
    (state.save_config)().map_err(|e| format!("配置写回失败：{e}"))
}

/// 基于当前配置生成修改后的配置（纯逻辑，可单测）。
///
/// 未配置的适配器按 `enabled = false` 兜底：仅改采样周期不应连带启用。
fn next_adapter_config(
    current: Option<&AdapterConfig>,
    mutate: impl FnOnce(&mut AdapterConfig),
) -> AdapterConfig {
    let mut cfg = current
        .cloned()
        .unwrap_or_else(AdapterConfig::default_disabled);
    mutate(&mut cfg);
    cfg
}

/// 注册全部命令到 Tauri builder（pano-app 装配时调用）。
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        list_adapters,
        adapter_status,
        set_adapter_enabled,
        set_adapter_sampling,
        restart_adapter,
        series_history,
        series_latest,
        list_components,
        window_content,
        window_set_fullscreen,
        window_set_always_on_top,
        window_set_monitor,
        window_set_decorations,
        window_set_position,
        window_set_size,
        window_focus,
        window_hide,
        window_show,
        list_windows,
        create_window,
        set_window_component,
        destroy_window,
        monitors,
        config_preview,
        config_schema_version,
    ])
}

// ---------------------------------------------------------------------------
// 组件目录辅助（纯函数，可单测；M2.2）

/// 在组件目录中查找组件类型。
pub fn find_component<'a>(spec: &'a UISpec, id: &str) -> Option<&'a ComponentSpec> {
    spec.components.iter().find(|c| c.id == id)
}

/// 按组件类型 id 解析其固定 series（目录中不存在 → 空列表）。
pub fn component_series_of(spec: &UISpec, component: &str) -> Vec<SeriesId> {
    find_component(spec, component)
        .map(|c| c.series.clone())
        .unwrap_or_default()
}

/// 组件是否可用：全部 series 所属适配器已注册（feature 已编译）。
///
/// 注意：**已注册但未启用**的适配器仍视为可用（建窗后显示空态），
/// 仅未注册（feature 未编译）的组件不可选（ui.md §3.2 / 架构 §7）。
pub fn component_available(
    comp: &ComponentSpec,
    adapter_registered: impl Fn(&AdapterId) -> bool,
) -> bool {
    comp.series
        .iter()
        .all(|s| adapter_registered(&s.adapter_id()))
}

/// 校验组件绑定（新建 / 切换组件共用）：
/// 组件在目录中存在 + 全部 series 所属适配器已注册。
fn validate_component_binding<'a>(
    spec: &'a UISpec,
    component: &str,
    adapter_registered: impl Fn(&AdapterId) -> bool,
) -> Result<&'a ComponentSpec, String> {
    let comp =
        find_component(spec, component).ok_or_else(|| format!("未知 UI 组件：{component}"))?;
    if !component_available(comp, adapter_registered) {
        return Err(format!(
            "组件 {component} 依赖的适配器未注册（feature 未编译？）"
        ));
    }
    Ok(comp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pano_core::adapter::AdapterId;
    use pano_core::capability::WindowSpec;

    fn sample_spec() -> UISpec {
        UISpec {
            requires: vec![],
            components: vec![
                ComponentSpec {
                    id: "sys-cpu".into(),
                    name: "CPU".into(),
                    series: vec![SeriesId::new(&AdapterId::new("sys.cpu"), "usage")],
                    window: WindowSpec {
                        title: "CPU".into(),
                        ..WindowSpec::default()
                    },
                },
                ComponentSpec {
                    id: "sys-net".into(),
                    name: "网络".into(),
                    series: vec![
                        SeriesId::new(&AdapterId::new("sys.net"), "recv_bps"),
                        SeriesId::new(&AdapterId::new("sys.net"), "sent_bps"),
                    ],
                    window: WindowSpec::default(),
                },
            ],
        }
    }

    #[test]
    fn find_component_hits_and_misses() {
        let spec = sample_spec();
        assert_eq!(
            find_component(&spec, "sys-cpu").map(|c| c.name.as_str()),
            Some("CPU")
        );
        assert!(find_component(&spec, "ghost").is_none());
    }

    #[test]
    fn component_series_of_resolves_from_catalog() {
        let spec = sample_spec();
        let series = component_series_of(&spec, "sys-net");
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].as_str(), "sys.net.recv_bps");
        // 未知组件 → 空
        assert!(component_series_of(&spec, "ghost").is_empty());
    }

    #[test]
    fn sys_dashboard_catalog_series_includes_busiest_disk() {
        // M2.3.1：磁盘仪表显示最忙盘符，sys-dashboard 组件订阅 busiest_disk
        let spec = crate::uispec();
        let catalog_series = component_series_of(&spec, "sys-dashboard");
        let series: Vec<&str> = catalog_series.iter().map(|s| s.as_str()).collect();
        assert_eq!(series.len(), 5);
        assert!(series.contains(&"sys.disk.active_percent"));
        assert!(series.contains(&"sys.disk.busiest_disk"));
    }

    #[test]
    fn component_available_requires_all_adapters_registered() {
        let spec = sample_spec();
        let cpu = find_component(&spec, "sys-cpu").unwrap();
        let net = find_component(&spec, "sys-net").unwrap();
        // 全部注册
        let all: std::collections::HashSet<&str> = ["sys.cpu", "sys.net"].into_iter().collect();
        assert!(component_available(cpu, |a| all.contains(a.as_str())));
        assert!(component_available(net, |a| all.contains(a.as_str())));
        // 缺 sys.net → cpu 可用、net 不可用
        let only_cpu: std::collections::HashSet<&str> = ["sys.cpu"].into_iter().collect();
        assert!(component_available(cpu, |a| only_cpu.contains(a.as_str())));
        assert!(!component_available(net, |a| only_cpu.contains(a.as_str())));
    }

    #[test]
    fn mask_secret_lines_masks_api_key_in_adapters_section_only() {
        // M2.5 安全点：config_preview 读回前端时掩码 api_key（不进 WebView）
        let toml = r#"schema_version = 1

[core]
sampling_ms = 500

[adapters."sys.cpu"]
enabled = true
sampling = 500

[adapters."deepseek.balance"]
enabled = true
sampling = 60000
api_key = "sk-secret"
base_url = "https://api.deepseek.com"

[window.manager]
title = "Pano 管理"
"#;
        let masked = mask_secret_lines(toml);
        assert!(
            masked.contains(r#"api_key = "********""#),
            "api_key 值应被掩码"
        );
        assert!(!masked.contains("sk-secret"), "明文密钥不得出现在预览中");
        // 非 adapters 段的键、非密钥键不受影响
        assert!(masked.contains("sampling_ms = 500"));
        assert!(masked.contains(r#"title = "Pano 管理""#));
        assert!(masked.contains("base_url = \"https://api.deepseek.com\""));
    }

    #[test]
    fn mask_secret_lines_leaves_non_secret_adapter_keys_untouched() {
        let toml = r#"[adapters."sys.net"]
enabled = true
link_mbps = 1000
"#;
        let masked = mask_secret_lines(toml);
        assert!(masked.contains("enabled = true"));
        assert!(masked.contains("link_mbps = 1000"));
    }

    #[test]
    fn validate_component_binding_rules() {
        let spec = sample_spec();
        let all: std::collections::HashSet<&str> = ["sys.cpu", "sys.net"].into_iter().collect();
        let reg = |a: &AdapterId| all.contains(a.as_str());

        // 未知组件
        assert!(validate_component_binding(&spec, "ghost", reg).is_err());
        // 依赖未注册
        let only_cpu: std::collections::HashSet<&str> = ["sys.cpu"].into_iter().collect();
        assert!(
            validate_component_binding(&spec, "sys-net", |a| only_cpu.contains(a.as_str()))
                .is_err()
        );
        // 合法绑定
        assert!(validate_component_binding(&spec, "sys-cpu", reg).is_ok());
        assert_eq!(
            validate_component_binding(&spec, "sys-net", reg)
                .map(|c| c.id.clone())
                .unwrap(),
            "sys-net"
        );
    }

    #[test]
    fn next_config_edits_existing() {
        let mut existing = AdapterConfig::default_enabled();
        existing.sampling = Some(200);
        let next = next_adapter_config(Some(&existing), |c| c.sampling = Some(500));
        assert!(next.enabled);
        assert_eq!(next.sampling, Some(500));
    }

    #[test]
    fn next_config_missing_adapter_defaults_disabled() {
        // 未配置适配器：改采样不连带启用（default_enabled 兜底会误启用）。
        let next = next_adapter_config(None, |c| c.sampling = Some(100));
        assert!(!next.enabled, "未配置适配器不应被连带启用");
        assert_eq!(next.sampling, Some(100));
    }

    #[test]
    fn next_config_enable_missing_adapter() {
        // 显式启用未配置适配器是合法操作。
        let next = next_adapter_config(None, |c| c.enabled = true);
        assert!(next.enabled);
    }
}
