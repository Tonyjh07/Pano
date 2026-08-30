//! 应用框架：窗口管理器（ui.md §2）。
//!
//! - 根 viewport = 管理窗口（适配器 + 设置页签，从托盘打开 / 隐藏）；
//! - 每个 series 一个监控组件窗口（deferred viewport，可置顶 / 全屏 / 隐藏）；
//! - 托盘命令消费、数据驱动重绘（跨窗口）、退出流程（先停适配器再退进程）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use eframe::egui;
use pano_core::adapter::SeriesId;
use pano_core::lifecycle::Lifecycle;
use pano_core::sample_store::SampleStore;

use crate::components;
use crate::pages;
use crate::pages::adapters::AdaptersState;
use crate::theme;
use crate::tray::TrayController;
use crate::windows::WindowState;

/// 管理窗口页签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerTab {
    Adapters,
    Settings,
}

/// Pano 应用：窗口管理器 + 托盘常驻。
pub struct PanoApp {
    pub(crate) core: Arc<Mutex<Lifecycle>>,
    pub(crate) store: Arc<SampleStore>,
    pub(crate) config_path: PathBuf,
    /// 共享窗口状态（组件窗口回调与主线程共同读写）。
    window_state: WindowState,
    /// 托盘控制器（`None` = 无托盘环境，此时关闭窗口即退出）。
    tray: Option<TrayController>,
    /// 管理窗口页签。
    manager_tab: ManagerTab,
    /// 管理窗口置顶 / 全屏。
    manager_pinned: bool,
    manager_fullscreen: bool,
    /// 当前 series 列表（检测变化以清理窗口状态）。
    known_series: Vec<SeriesId>,
    /// 上次托盘菜单重建时的组件可见性快照（`(series, visible)`）。
    last_menu_snapshot: Vec<(String, bool)>,
    /// 各 series 上次看到的最新样本时间戳（数据驱动重绘）。
    pub(crate) last_seen: HashMap<SeriesId, SystemTime>,
    /// 距上次请求重绘的时间（节流）。
    pub(crate) last_repaint_request: Option<SystemTime>,
    /// 适配器管理页状态（草稿 / 提示）。
    pub(crate) adapters_state: AdaptersState,
    /// 退出已请求（托盘「退出」→ 帧内关闭全部窗口 → eframe 退出）。
    quit_requested: bool,
}

impl PanoApp {
    /// 构造应用：创建托盘（失败降级为无托盘模式）。
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        core: Arc<Mutex<Lifecycle>>,
        store: Arc<SampleStore>,
        config_path: PathBuf,
    ) -> Self {
        let window_state = WindowState::default();
        let tray = match TrayController::build(&cc.egui_ctx, &window_state) {
            Ok(tray) => Some(tray),
            Err(e) => {
                tracing::warn!(target: "pano::ui", error = %e, "创建托盘失败，降级为无托盘模式（关闭窗口即退出）");
                None
            }
        };
        Self {
            core,
            store,
            config_path,
            window_state,
            tray,
            manager_tab: ManagerTab::Adapters,
            manager_pinned: false,
            manager_fullscreen: false,
            known_series: Vec::new(),
            last_menu_snapshot: Vec::new(),
            last_seen: HashMap::new(),
            last_repaint_request: None,
            adapters_state: AdaptersState::default(),
            quit_requested: false,
        }
    }

    /// 消费托盘命令（打开管理窗口 / 打开组件窗口 / 退出）。
    fn handle_tray_commands(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else { return };
        let cmds = tray.take_commands();

        if cmds.open_manager {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if let Some(sid) = cmds.open_component {
            self.window_state.set_component_visible(&sid, true);
            let viewport = components::viewport_id(&sid);
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
            ctx.request_repaint_after_for(Duration::from_millis(50), viewport);
        }
        if cmds.quit {
            self.quit_requested = true;
            self.window_state.request_quit();
        }
    }

    /// 同步 series 列表：变化时清理窗口状态；series 或组件可见性变化时重建托盘菜单。
    fn sync_series(&mut self) {
        let series = self.store.series_ids();
        let changed = series != self.known_series;
        self.known_series = series;
        let list: Vec<String> = self
            .known_series
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        if changed {
            self.window_state.retain_series(&list);
            // 新出现的 series：默认打开组件窗口（首次运行所见即所得）。
            for s in &list {
                self.window_state.init_component(s);
            }
        }
        let snapshot: Vec<(String, bool)> = list
            .iter()
            .map(|s| (s.clone(), self.window_state.component(s).visible))
            .collect();
        if changed || snapshot != self.last_menu_snapshot {
            self.last_menu_snapshot = snapshot;
            if let Some(tray) = &self.tray {
                tray.rebuild_menu(&list, &self.window_state);
            }
        }
    }

    /// 数据驱动重绘（ui.md §6）：任一 series 有新样本时，对管理窗口与
    /// 全部组件窗口请求重绘；节流阈值 100ms（≈ 常见采样间隔的一半）。
    fn maybe_request_repaint(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        for series in self.store.series_ids() {
            if let Some(latest) = self.store.latest(&series)
                && self.last_seen.get(&series) != Some(&latest.timestamp)
            {
                self.last_seen.insert(series, latest.timestamp);
                changed = true;
            }
        }
        if changed {
            let now = SystemTime::now();
            let throttled = self.last_repaint_request.is_some_and(|t| {
                now.duration_since(t).unwrap_or_default() < Duration::from_millis(100)
            });
            if !throttled {
                self.last_repaint_request = Some(now);
                ctx.request_repaint_after(Duration::from_millis(50));
                for sid in &self.known_series {
                    // 只唤醒打开的组件窗口（ui.md §6）。
                    if self.window_state.component(sid.as_str()).visible {
                        ctx.request_repaint_after_for(
                            Duration::from_millis(50),
                            components::viewport_id(sid.as_str()),
                        );
                    }
                }
            }
        }
    }

    /// 渲染管理窗口（根 viewport）：标题行（置顶 / 全屏 / 隐藏）+ 页签 + 内容。
    fn render_manager(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        // 关闭请求：有托盘 → 隐藏而非退出；无托盘或退出流程 → 放行。
        if ui.input(|i| i.viewport().close_requested())
            && self.tray.is_some()
            && !self.quit_requested
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        self.status_bar(ui);

        egui::Panel::top("manager_header")
            .frame(egui::Frame::side_top_panel(ui.style()).fill(theme::PANEL))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(
                        egui::RichText::new("Pano 管理")
                            .color(theme::ACCENT)
                            .size(18.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let full_label = if self.manager_fullscreen {
                            "退出全屏"
                        } else {
                            "⛶ 全屏"
                        };
                        if ui.button(full_label).clicked() {
                            self.manager_fullscreen = !self.manager_fullscreen;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                self.manager_fullscreen,
                            ));
                        }
                        let pin_label = if self.manager_pinned {
                            "📌 已置顶"
                        } else {
                            "📌 置顶"
                        };
                        if ui
                            .selectable_label(self.manager_pinned, pin_label)
                            .clicked()
                        {
                            self.manager_pinned = !self.manager_pinned;
                            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                if self.manager_pinned {
                                    egui::WindowLevel::AlwaysOnTop
                                } else {
                                    egui::WindowLevel::Normal
                                },
                            ));
                        }
                        if ui
                            .button("— 隐藏")
                            .on_hover_text("隐藏到托盘（托盘菜单重新打开）")
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        }
                    });
                });
            });

        egui::Panel::top("manager_tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_label(self.manager_tab == ManagerTab::Adapters, "⚙️ 适配器")
                    .clicked()
                    .then(|| self.manager_tab = ManagerTab::Adapters);
                ui.selectable_label(self.manager_tab == ManagerTab::Settings, "🛠 设置")
                    .clicked()
                    .then(|| self.manager_tab = ManagerTab::Settings);
            });
        });

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::central_panel(ui.style()).fill(theme::BG))
            .show(ui, |ui| match self.manager_tab {
                ManagerTab::Adapters => pages::adapters::show(ui, self),
                ManagerTab::Settings => pages::settings::show(ui, self),
            });
    }

    /// 声明全部监控组件窗口（deferred viewports）。
    fn render_components(&self, ctx: &egui::Context) {
        let icon = crate::icon::app_icon();
        let store = Arc::clone(&self.store);
        let state = self.window_state.clone();
        let ctx = ctx.clone();
        for (idx, sid) in self.known_series.iter().enumerate() {
            let series = sid.clone();
            let comp = state.component(series.as_str());
            let builder = components::viewport_builder(series.as_str(), comp, &icon);
            let color = theme::SERIES_COLORS[idx % theme::SERIES_COLORS.len()];
            let store_ui = Arc::clone(&store);
            let state_ui = state.clone();
            let ctx_ui = ctx.clone();
            ctx.show_viewport_deferred(
                components::viewport_id(series.as_str()),
                builder,
                move |ui, _class| {
                    components::show(ui, &ctx_ui, &series, &store_ui, &state_ui, color)
                },
            );
        }
    }

    /// 退出流程：关闭全部窗口（组件窗口回调检测到 `quit_requested` 时放行）。
    fn close_all_windows(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        for sid in &self.known_series {
            ctx.send_viewport_cmd_to(
                components::viewport_id(sid.as_str()),
                egui::ViewportCommand::Close,
            );
        }
    }

    /// 底部状态栏：核心运行状态 · 适配器运行数 · 错误数。
    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .frame(egui::Frame::side_top_panel(ui.style()).fill(theme::PANEL))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let core = lock(&self.core);
                    let ids = core.adapter_ids();
                    let mut running = 0;
                    let mut errors = 0;
                    for id in &ids {
                        match core.status_of(id) {
                            pano_core::adapter::AdapterStatus::Running => running += 1,
                            pano_core::adapter::AdapterStatus::Error { .. } => errors += 1,
                            _ => {}
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!("运行中 {running}/{}", ids.len()))
                            .color(theme::ACCENT),
                    );
                    if errors > 0 {
                        ui.label(egui::RichText::new(format!("错误 {errors}")).color(theme::ERROR));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Pano").weak());
                    });
                });
            });
    }
}

/// 获取锁（毒化时沿用旧数据，不 panic）。
pub(crate) fn lock<'a, T>(mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl eframe::App for PanoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::apply(&ctx);

        self.handle_tray_commands(&ctx);
        self.sync_series();
        self.maybe_request_repaint(&ctx);

        // 管理窗口（根 viewport）。
        self.render_manager(ui);
        // 监控组件窗口（deferred viewports）。
        self.render_components(&ctx);

        // 退出流程：关闭全部窗口，让事件循环自然退出。
        if self.quit_requested {
            self.close_all_windows(&ctx);
        }
    }
}
