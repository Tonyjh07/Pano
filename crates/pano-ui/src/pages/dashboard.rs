//! 仪表盘页（ui.md §3.1）：示例面板——数值卡 + 实时曲线。
//!
//! M1 用于验证数据链路：每个 series 一个面板（标题 + 最新值 + 最近窗口曲线）。
//! 曲线只取最近 300 个样本绘制（ui.md §5），避免每帧重算全量历史。

use eframe::egui;
use pano_core::adapter::{SampleValue, SeriesId};

use crate::app::PanoApp;
use crate::theme;

/// 曲线绘制窗口（样本数）。
const PLOT_WINDOW: usize = 300;
/// 面板网格列数。
const GRID_COLUMNS: usize = 2;

/// 绘制仪表盘页。
pub fn show(ui: &mut egui::Ui, app: &mut PanoApp) {
    ui.add_space(8.0);
    ui.heading("仪表盘");
    ui.separator();

    let series = app.store.series_ids();
    if series.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("无数据").strong());
            ui.label(
                egui::RichText::new("尚无适配器产出样本。请到「适配器」页检查启用状态。").weak(),
            );
        });
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("dashboard_panels")
            .num_columns(GRID_COLUMNS)
            .spacing([16.0, 16.0])
            .show(ui, |ui| {
                for (idx, sid) in series.iter().enumerate() {
                    let color = theme::SERIES_COLORS[idx % theme::SERIES_COLORS.len()];
                    render_panel(ui, app, sid, color);
                    if (idx + 1) % GRID_COLUMNS == 0 {
                        ui.end_row();
                    }
                }
            });
    });
}

/// 渲染一个 series 面板：标题 + 最新值 + 曲线。
fn render_panel(ui: &mut egui::Ui, app: &PanoApp, sid: &SeriesId, color: egui::Color32) {
    egui::Frame::group(ui.style())
        .fill(theme::PANEL)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(320.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(sid.as_str()).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(latest) = app.store.latest(sid) {
                        ui.label(
                            egui::RichText::new(format!("最新 {}", latest.value))
                                .color(color)
                                .size(20.0),
                        );
                    }
                });
            });
            ui.add_space(4.0);
            plot_series(ui, app, sid, color);
        });
}

/// 绘制最近窗口的折线（相对时间，从窗口起点开始）。
fn plot_series(ui: &mut egui::Ui, app: &PanoApp, sid: &SeriesId, color: egui::Color32) {
    let history = app.store.history(sid, PLOT_WINDOW);
    let t0 = history
        .first()
        .and_then(|s| s.timestamp.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64());

    let points: Vec<[f64; 2]> = history
        .iter()
        .filter_map(|sample| {
            let t = sample
                .timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs_f64())?;
            let v = match sample.value {
                SampleValue::Number(n) => n,
                SampleValue::Bool(b) => {
                    if b {
                        1.0
                    } else {
                        0.0
                    }
                }
                SampleValue::Text(_) | SampleValue::Json(_) => return None,
            };
            Some([t - t0.unwrap_or(0.0), v])
        })
        .collect();

    egui_plot::Plot::new(sid.as_str())
        .height(140.0)
        .allow_scroll(false)
        .allow_drag(false)
        .allow_zoom(false)
        .show_axes([true, true])
        .show(ui, |plot_ui| {
            if !points.is_empty() {
                plot_ui.line(
                    egui_plot::Line::new(sid.as_str(), egui_plot::PlotPoints::from(points))
                        .color(color),
                );
            }
        });
}
