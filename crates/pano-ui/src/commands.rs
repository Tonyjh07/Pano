//! Tauri 命令层（ui.md §9）：前端 `invoke` → 命令 → core / 窗口服务。
//!
//! 约束：
//! - 窗口控制一律经 [`pano_window::WindowService`]，**不直接操作 Tauri 窗口类型**；
//! - 启停 / 配置修改走 **async 命令**（运行于 tauri 异步运行时，不卡 UI 主线程）；
//! - 配置修改流程：core 热生效（失败回滚）→ 成功才经 `save_config` 写回 pano.toml。

use pano_core::adapter::{AdapterId, SeriesId};
use pano_core::config::{AdapterConfig, SCHEMA_VERSION};
use tauri::State;

use crate::dto::{AdapterInfo, FieldInfo, MonitorDto, SampleEventDto, StatusDto};
use crate::state::AppState;

/// 适配器列表（管理页表格，schema 驱动表单渲染）。
#[tauri::command]
pub fn list_adapters(state: State<'_, AppState>) -> Vec<AdapterInfo> {
    let lc = state.lifecycle();
    let mut out = Vec::new();
    for id in lc.adapter_ids() {
        let meta = lc.adapter_meta(&id);
        let caps = lc.capabilities_of(&id).unwrap_or_default();
        let schema = lc.config_schema_of(&id).unwrap_or_default();
        let status = lc.status_of(&id);
        let sampling = lc.config().sampling_ms_of(id.as_str());
        let enabled = lc.config().is_enabled(id.as_str());
        out.push(AdapterInfo::new(
            id.as_str(),
            meta.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
            meta.as_ref()
                .map(|m| m.description.clone())
                .unwrap_or_default(),
            meta.as_ref().map(|m| m.version.clone()).unwrap_or_default(),
            caps.iter().map(|c| c.to_string()).collect(),
            &status,
            enabled,
            sampling,
            schema.fields.iter().map(FieldInfo::from).collect(),
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

/// 某组件消费的 series 列表（前端组件窗口按窗口 label 查询，ui.md §4）。
#[tauri::command]
pub fn component_series(state: State<'_, AppState>, id: String) -> Result<Vec<String>, String> {
    state
        .ui_spec
        .components
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.series.iter().map(|s| s.as_str().to_string()).collect())
        .ok_or_else(|| format!("未知组件：{id}"))
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
#[tauri::command]
pub fn config_preview(state: State<'_, AppState>) -> Result<String, String> {
    let lc = state.lifecycle();
    lc.config()
        .to_toml()
        .map_err(|e| format!("配置序列化失败：{e}"))
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
        component_series,
        window_set_fullscreen,
        window_set_always_on_top,
        window_set_monitor,
        window_set_position,
        window_set_size,
        window_focus,
        window_hide,
        monitors,
        config_preview,
        config_schema_version,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

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
