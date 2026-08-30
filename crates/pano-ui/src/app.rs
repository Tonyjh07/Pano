//! 应用框架：PanoApp（侧边栏 + 页面切换 + 状态栏 + 数据驱动重绘）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use eframe::egui;
use pano_core::adapter::SeriesId;
use pano_core::lifecycle::Lifecycle;
use pano_core::sample_store::SampleStore;

use crate::pages;
use crate::pages::adapters::AdaptersState;
use crate::theme;

/// 页面导航。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Adapters,
    Settings,
}

/// Pano 主应用。
pub struct PanoApp {
    pub(crate) core: Arc<Mutex<Lifecycle>>,
    pub(crate) store: Arc<SampleStore>,
    pub(crate) config_path: PathBuf,
    pub(crate) page: Page,
    /// 各 series 上次看到的最新样本时间戳（数据驱动重绘）。
    pub(crate) last_seen: HashMap<SeriesId, SystemTime>,
    /// 距上次请求重绘的时间（节流）。
    pub(crate) last_repaint_request: Option<SystemTime>,
    /// 适配器管理页状态（草稿 / 提示）。
    pub(crate) adapters_state: AdaptersState,
}

impl PanoApp {
    /// 构造应用。
    pub fn new(core: Arc<Mutex<Lifecycle>>, store: Arc<SampleStore>, config_path: PathBuf) -> Self {
        Self {
            core,
            store,
            config_path,
            page: Page::Dashboard,
            last_seen: HashMap::new(),
            last_repaint_request: None,
            adapters_state: AdaptersState::default(),
        }
    }

    /// 数据驱动重绘（ui.md §5）：任一 series 有新样本时请求重绘，
    /// 节流阈值 100ms（≈ 常见采样间隔的一半），避免空转。
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
            }
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

    /// 侧边栏：页面导航 + 底部版本号。
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("sidebar")
            .resizable(false)
            .default_size(160.0)
            .frame(egui::Frame::side_top_panel(ui.style()).fill(theme::PANEL))
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading(egui::RichText::new("Pano").color(theme::ACCENT).size(20.0));
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                let items = [
                    (Page::Dashboard, "📊 仪表盘"),
                    (Page::Adapters, "⚙️ 适配器"),
                    (Page::Settings, "🛠 设置"),
                ];
                for (page, label) in items {
                    let selected = self.page == page;
                    if ui.selectable_label(selected, label).clicked() {
                        self.page = page;
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
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
        self.maybe_request_repaint(&ctx);
        self.status_bar(ui);
        self.sidebar(ui);

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::central_panel(ui.style()).fill(theme::BG))
            .show(ui, |ui| match self.page {
                Page::Dashboard => pages::dashboard::show(ui, self),
                Page::Adapters => pages::adapters::show(ui, self),
                Page::Settings => pages::settings::show(ui, self),
            });
    }
}
