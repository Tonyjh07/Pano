//! 状态徽标：圆点 + 文字（ui.md §4）；错误状态悬停显示详情。

use eframe::egui::{self, Color32, RichText};
use pano_core::adapter::AdapterStatus;

use crate::theme;

/// 绘制状态徽标，返回响应（Error 状态挂 tooltip 显示最后错误）。
pub fn show(ui: &mut egui::Ui, status: &AdapterStatus) -> egui::Response {
    let (color, label, tooltip) = match status {
        AdapterStatus::Running => (theme::OK, "运行中", None),
        AdapterStatus::Stopped => (Color32::from_gray(160), "已停止", None),
        AdapterStatus::Disabled => (Color32::from_gray(110), "已禁用", None),
        AdapterStatus::Starting => (theme::WARN, "启动中", None),
        AdapterStatus::Error { last_error } => (theme::ERROR, "错误", Some(last_error.clone())),
    };

    let response = ui
        .horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, color);
            ui.label(RichText::new(label).color(color));
        })
        .response;

    match tooltip {
        Some(detail) => response.on_hover_text(format!("{label}：{detail}")),
        None => response,
    }
}
