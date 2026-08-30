//! 监控组件窗口（ui.md §4）：每个 series 一个独立窗口。
//!
//! 组件自由设置：窗口标题 / 初始尺寸 / 置顶 / 全屏；关闭窗口 = 隐藏（托盘可重开）。
//! 内容：标题 + 最新值 + 最近窗口曲线（egui_plot）+ 窗口控制（置顶 / 全屏 / 隐藏）。

use std::sync::Arc;

use eframe::egui;
use pano_core::adapter::{SampleValue, SeriesId};
use pano_core::sample_store::SampleStore;

use crate::windows::{ComponentState, WindowState};

/// 曲线绘制窗口（样本数，ui.md §5）。
const PLOT_WINDOW: usize = 300;

/// 组件窗口的 viewport id（稳定：由 series id 派生，跨帧一致）。
pub fn viewport_id(series: &str) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("pano-component", series))
}

/// 组件窗口的 viewport builder（首次创建时生效；运行期改动走 `ViewportCommand`）。
pub fn viewport_builder(
    series: &str,
    state: ComponentState,
    icon: &Arc<egui::IconData>,
) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(format!("{series} · Pano"))
        .with_inner_size([420.0, 280.0])
        .with_min_inner_size([300.0, 200.0])
        .with_window_level(if state.pinned {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        })
        .with_fullscreen(state.fullscreen)
        .with_visible(state.visible)
        .with_icon(Arc::clone(icon))
}

/// 渲染组件窗口内容（在对应 viewport 的 UI 回调中调用）。
pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    series: &SeriesId,
    store: &Arc<SampleStore>,
    state: &WindowState,
    color: egui::Color32,
) {
    let sid = series.as_str();
    let viewport = viewport_id(sid);

    // 关闭请求：有托盘时隐藏而非退出；托盘「退出」流程中放行真正关闭。
    if ui.input(|i| i.viewport().close_requested()) {
        if state.quit_requested() {
            return;
        }
        ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Visible(false));
        state.set_component_visible(sid, false);
        return;
    }

    // 窗口控制行：标题 + 置顶 / 全屏 / 隐藏。
    let mut comp = state.component(sid);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(sid).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("✕")
                .on_hover_text("隐藏窗口（托盘可重新打开）")
                .clicked()
            {
                comp.visible = false;
                ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Visible(false));
            }
            let full_label = if comp.fullscreen {
                "退出全屏"
            } else {
                "⛶ 全屏"
            };
            if ui.button(full_label).clicked() {
                comp.fullscreen = !comp.fullscreen;
                ctx.send_viewport_cmd_to(
                    viewport,
                    egui::ViewportCommand::Fullscreen(comp.fullscreen),
                );
            }
            let pin_label = if comp.pinned {
                "📌 已置顶"
            } else {
                "📌 置顶"
            };
            if ui.selectable_label(comp.pinned, pin_label).clicked() {
                comp.pinned = !comp.pinned;
                ctx.send_viewport_cmd_to(
                    viewport,
                    egui::ViewportCommand::WindowLevel(if comp.pinned {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                );
            }
        });
    });
    // 回写（按钮点击已修改 comp）。
    if comp != state.component(sid) {
        state.set_component(sid, comp);
    }
    ui.separator();

    // 最新值 + 曲线。
    ui.horizontal(|ui| {
        if let Some(latest) = store.latest(series) {
            ui.label(egui::RichText::new("最新").weak());
            ui.label(
                egui::RichText::new(latest.value.to_string())
                    .color(color)
                    .size(20.0),
            );
        }
    });
    ui.add_space(4.0);
    plot_series(ui, store, series, color);
}

/// 绘制最近窗口的折线（相对时间，从窗口起点开始）。
fn plot_series(
    ui: &mut egui::Ui,
    store: &Arc<SampleStore>,
    series: &SeriesId,
    color: egui::Color32,
) {
    let history = store.history(series, PLOT_WINDOW);
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

    egui_plot::Plot::new(series.as_str())
        .height(140.0)
        .allow_scroll(false)
        .allow_drag(false)
        .allow_zoom(false)
        .show_axes([true, true])
        .show(ui, |plot_ui| {
            if !points.is_empty() {
                plot_ui.line(
                    egui_plot::Line::new(series.as_str(), egui_plot::PlotPoints::from(points))
                        .color(color),
                );
            }
        });
}
