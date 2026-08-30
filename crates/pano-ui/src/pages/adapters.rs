//! 适配器管理页（ui.md §3.2）。
//!
//! 表格每行一个适配器：状态徽标 / 名称与 id / 能力 / 采样间隔（可编辑）/
//! 启用开关 / 操作；配置表单由 `config_schema` 驱动渲染。
//! 修改 → 写回 `pano.toml` → 仅重启该适配器（热生效）。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use eframe::egui;
use pano_core::adapter::{
    AdapterId, AdapterMeta, AdapterStatus, ConfigField, ConfigSchema, ConfigValue, FieldKind,
};
use pano_core::capability::Capability;
use pano_core::config::{AdapterConfig, config_value_to_toml, toml_value_to_config};
use pano_core::lifecycle::Lifecycle;

use crate::app::{PanoApp, lock};
use crate::widgets::status_badge;

/// 单行编辑草稿。
#[derive(Clone, Debug)]
pub struct Draft {
    pub enabled: bool,
    pub sampling_ms: u64,
    /// 自定义字段草稿。
    pub custom: HashMap<String, ConfigValue>,
    /// schema 表单是否展开。
    pub form_open: bool,
}

/// 页面状态（跨帧保存于 [`PanoApp`]）。
#[derive(Default)]
pub struct AdaptersState {
    pub drafts: HashMap<String, Draft>,
    /// 操作提示 (成功?, 消息)。
    pub notice: Option<(bool, String)>,
    /// 正在应用中的适配器（防重复点击）。
    pub applying: HashSet<String>,
}

/// 适配器行快照（渲染前一次性收集，避免渲染期间持锁）。
struct AdapterRow {
    id: AdapterId,
    meta: AdapterMeta,
    status: AdapterStatus,
    capabilities: Vec<Capability>,
    schema: ConfigSchema,
    config: AdapterConfig,
    /// `[core].sampling_ms`（未显式配置 sampling 时的实际生效值）。
    default_sampling_ms: u64,
}

/// 绘制适配器管理页。
pub fn show(ui: &mut egui::Ui, app: &mut PanoApp) {
    ui.add_space(8.0);
    ui.heading("适配器");
    ui.separator();

    let rows = collect_rows(&app.core);
    for row in &rows {
        app.adapters_state
            .drafts
            .entry(row.id.as_str().to_string())
            .or_insert_with(|| draft_from_config(&row.config, row.default_sampling_ms));
    }

    // 操作提示
    if let Some((ok, msg)) = &app.adapters_state.notice {
        let color = if *ok {
            crate::theme::OK
        } else {
            crate::theme::ERROR
        };
        ui.colored_label(color, if *ok { "✓ " } else { "✗ " }.to_owned() + msg);
        ui.add_space(4.0);
    }

    if rows.is_empty() {
        ui.label("无已注册适配器（请检查 feature 配置）");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("adapter_table")
            .striped(true)
            .min_col_width(60.0)
            .spacing([24.0, 10.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("状态").strong());
                ui.label(egui::RichText::new("适配器").strong());
                ui.label(egui::RichText::new("能力").strong());
                ui.label(egui::RichText::new("采样间隔 (ms)").strong());
                ui.label(egui::RichText::new("启用").strong());
                ui.label(egui::RichText::new("操作").strong());
                ui.end_row();

                for row in &rows {
                    render_row(ui, app, row);
                }
            });

        // schema 驱动的配置表单（展开的适配器）
        for row in &rows {
            let key = row.id.as_str().to_string();
            let form_open = app
                .adapters_state
                .drafts
                .get(&key)
                .map(|d| d.form_open)
                .unwrap_or(false);
            if form_open {
                ui.add_space(6.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    render_schema_form(ui, app, row);
                });
            }
        }
    });
}

/// 收集全部适配器行快照（锁内完成，渲染不持锁）。
fn collect_rows(core: &Mutex<Lifecycle>) -> Vec<AdapterRow> {
    let lc = lock(core);
    lc.adapter_ids()
        .into_iter()
        .filter_map(|id| {
            let meta = lc.adapter_meta(&id)?;
            let status = lc.status_of(&id);
            let capabilities = lc.capabilities_of(&id).unwrap_or_default();
            let schema = lc.config_schema_of(&id).unwrap_or_default();
            let config = lc
                .config()
                .adapters
                .get(id.as_str())
                .cloned()
                .unwrap_or_else(AdapterConfig::default_enabled);
            Some(AdapterRow {
                id,
                meta,
                status,
                capabilities,
                schema,
                config,
                default_sampling_ms: lc.config().core.sampling_ms,
            })
        })
        .collect()
}

/// 从当前配置初始化草稿；采样间隔缺省用 `[core].sampling_ms` 的实际生效值。
fn draft_from_config(config: &AdapterConfig, default_sampling_ms: u64) -> Draft {
    let mut custom = HashMap::new();
    for (k, v) in &config.custom {
        if let Ok(cv) = toml_value_to_config(v) {
            custom.insert(k.clone(), cv);
        }
    }
    Draft {
        enabled: config.enabled,
        sampling_ms: config.sampling.unwrap_or(default_sampling_ms),
        custom,
        form_open: false,
    }
}

/// 渲染一行。
fn render_row(ui: &mut egui::Ui, app: &mut PanoApp, row: &AdapterRow) {
    let key = row.id.as_str().to_string();
    // 复制草稿编辑（闭包内只改局部，闭包后写回，避免借用冲突）
    let mut draft = match app.adapters_state.drafts.get(&key).cloned() {
        Some(d) => d,
        None => return,
    };
    let applying = app.adapters_state.applying.contains(&key);
    let mut apply_clicked = false;
    let mut restart_clicked = false;
    let mut form_toggle = false;
    let mut draft_changed = false;

    // 状态徽标
    status_badge::show(ui, &row.status);

    // 名称 + id + 描述
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(&row.meta.name).strong());
        ui.label(egui::RichText::new(row.id.as_str()).weak().small());
        if !row.meta.description.is_empty() {
            ui.label(egui::RichText::new(&row.meta.description).weak().small());
        }
    });

    // 能力
    let caps = row
        .capabilities
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    ui.label(egui::RichText::new(caps).weak());

    // 采样间隔（可编辑）
    if ui
        .add(
            egui::DragValue::new(&mut draft.sampling_ms)
                .range(1..=600_000)
                .speed(10),
        )
        .changed()
    {
        draft_changed = true;
    }

    // 启用开关（egui 0.36 无 Switch，用 Checkbox 表达）
    if ui
        .add(egui::Checkbox::new(&mut draft.enabled, ""))
        .changed()
    {
        draft_changed = true;
    }

    // 操作列
    ui.horizontal(|ui| {
        let apply_btn = ui.add_enabled(
            !applying,
            egui::Button::new("应用").fill(crate::theme::ACCENT),
        );
        if apply_btn.clicked() {
            apply_clicked = true;
        }

        // Error 状态可手动重启
        if matches!(row.status, AdapterStatus::Error { .. }) && ui.button("重启").clicked() {
            restart_clicked = true;
        }

        // 配置表单开关
        let label = if draft.form_open {
            "收起配置"
        } else {
            "配置 ▸"
        };
        if ui.selectable_label(draft.form_open, label).clicked() {
            form_toggle = true;
        }
    });

    ui.end_row();

    // 写回草稿
    if form_toggle {
        draft.form_open = !draft.form_open;
    }
    if draft_changed || form_toggle {
        app.adapters_state.drafts.insert(key.clone(), draft);
    }

    // 处理操作（闭包外执行，避免持锁渲染）
    if apply_clicked {
        apply_now(app, row, &key);
    }
    if restart_clicked {
        match lock(&app.core).start(&row.id) {
            Ok(()) => {
                app.adapters_state.notice = Some((true, format!("{} 已重启", row.id)));
            }
            Err(e) => {
                app.adapters_state.notice = Some((false, format!("{} 重启失败：{e}", row.id)));
            }
        }
    }
}

/// 应用当前草稿：热生效（仅重启该适配器）+ 写回 pano.toml。
fn apply_now(app: &mut PanoApp, row: &AdapterRow, key: &str) {
    let draft = app.adapters_state.drafts.get(key).cloned();
    let Some(draft) = draft else { return };
    app.adapters_state.applying.insert(key.to_string());

    let result = (|| -> Result<(), String> {
        let mut lc = lock(&app.core);
        let mut cfg = lc
            .config()
            .adapters
            .get(key)
            .cloned()
            .unwrap_or_else(AdapterConfig::default_enabled);
        cfg.enabled = draft.enabled;
        cfg.sampling = Some(draft.sampling_ms);
        for (k, v) in &draft.custom {
            cfg.custom.insert(k.clone(), config_value_to_toml(v));
        }
        lc.apply_adapter_config(&row.id, cfg)
            .map_err(|e| e.to_string())?;

        // 写回配置文件
        let text = lc.config().to_toml().map_err(|e| e.to_string())?;
        std::fs::write(&app.config_path, text).map_err(|e| format!("写回配置失败：{e}"))
    })();

    app.adapters_state.applying.remove(key);
    app.adapters_state.notice = Some(match result {
        Ok(()) => (true, format!("{} 已应用（热生效）", row.id)),
        Err(e) => (false, format!("{} 应用失败：{e}", row.id)),
    });
}

/// 渲染 schema 驱动的配置表单。
fn render_schema_form(ui: &mut egui::Ui, app: &mut PanoApp, row: &AdapterRow) {
    ui.label(egui::RichText::new("自定义配置").strong());
    let key = row.id.as_str().to_string();
    let draft = app.adapters_state.drafts.get_mut(&key);
    let Some(draft) = draft else { return };

    if row.schema.fields.is_empty() {
        ui.label(egui::RichText::new("该适配器无自定义配置").weak());
        return;
    }

    egui::Grid::new(("schema_form", key.clone()))
        .num_columns(3)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            for field in &row.schema.fields {
                render_field(ui, draft, field);
                ui.end_row();
            }
        });
}

/// 按字段类型渲染一个配置项。
fn render_field(ui: &mut egui::Ui, draft: &mut Draft, field: &ConfigField) {
    ui.label(field.label.clone());
    match &field.kind {
        FieldKind::Number => {
            let mut value = match draft.custom.get(&field.key) {
                Some(ConfigValue::Number(n)) => *n,
                _ => match field.default {
                    ConfigValue::Number(n) => n,
                    _ => 0.0,
                },
            };
            if ui
                .add(egui::DragValue::new(&mut value).speed(0.1))
                .changed()
            {
                draft
                    .custom
                    .insert(field.key.clone(), ConfigValue::Number(value));
            }
        }
        FieldKind::Bool => {
            let mut value = match draft.custom.get(&field.key) {
                Some(ConfigValue::Bool(b)) => *b,
                _ => matches!(field.default, ConfigValue::Bool(true)),
            };
            if ui.checkbox(&mut value, "").changed() {
                draft
                    .custom
                    .insert(field.key.clone(), ConfigValue::Bool(value));
            }
        }
        FieldKind::Text => {
            let mut value = match draft.custom.get(&field.key) {
                Some(ConfigValue::Text(s)) => s.clone(),
                _ => match &field.default {
                    ConfigValue::Text(s) => s.clone(),
                    _ => String::new(),
                },
            };
            if ui.text_edit_singleline(&mut value).changed() {
                draft
                    .custom
                    .insert(field.key.clone(), ConfigValue::Text(value));
            }
        }
        FieldKind::Choice(options) => {
            let current = match draft.custom.get(&field.key) {
                Some(ConfigValue::Text(s)) => s.clone(),
                _ => match &field.default {
                    ConfigValue::Text(s) => s.clone(),
                    _ => options.first().cloned().unwrap_or_default(),
                },
            };
            let mut edited = current.clone();
            egui::ComboBox::from_id_salt(("schema_choice", field.key.clone()))
                .selected_text(current.clone())
                .show_ui(ui, |ui| {
                    for option in options {
                        ui.selectable_value(&mut edited, option.clone(), option);
                    }
                });
            if edited != current {
                draft
                    .custom
                    .insert(field.key.clone(), ConfigValue::Text(edited));
            }
        }
    }
    if let Some(help) = &field.help {
        ui.label(egui::RichText::new(help).weak().small());
    }
}
