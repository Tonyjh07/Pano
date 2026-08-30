//! 设置页（M1 骨架；主题切换、默认采样间隔等属 M3）。

use eframe::egui;

use crate::app::PanoApp;

/// 绘制设置页。
pub fn show(ui: &mut egui::Ui, _app: &mut PanoApp) {
    ui.add_space(8.0);
    ui.heading("设置");
    ui.separator();
    ui.label("主题切换、默认采样间隔、环形缓冲容量（M3 实现）");
    ui.label("查看当前 pano.toml 配置（M3 实现）");
}
