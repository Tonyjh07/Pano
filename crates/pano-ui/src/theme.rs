//! 主题与视觉规范（ui.md §4）。
//!
//! 暗色默认色板：背景 `#1e1e1e`、面板 `#252526`、边框 `#333`、
//! 强调 `#4f9cf9`、成功 `#3fb950`、警告 `#d29922`、错误 `#f85149`。

use eframe::egui::{self, Color32, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
pub const PANEL: Color32 = Color32::from_rgb(0x25, 0x25, 0x26);
pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
pub const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x9c, 0xf9);
pub const OK: Color32 = Color32::from_rgb(0x3f, 0xb9, 0x50);
pub const WARN: Color32 = Color32::from_rgb(0xd2, 0x99, 0x22);
pub const ERROR: Color32 = Color32::from_rgb(0xf8, 0x51, 0x49);

/// 曲线色板（依序取用）。
pub const SERIES_COLORS: [Color32; 5] = [
    Color32::from_rgb(0x4f, 0x9c, 0xf9), // 蓝
    Color32::from_rgb(0x39, 0xc5, 0xcf), // 青
    Color32::from_rgb(0x3f, 0xb9, 0x50), // 绿
    Color32::from_rgb(0xd2, 0x99, 0x22), // 橙
    Color32::from_rgb(0xa5, 0x71, 0xd9), // 紫
];

/// 应用暗色主题（M1 固定暗色；主题切换属 M3）。
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = BG;
    visuals.selection.bg_fill = ACCENT;
    visuals.hyperlink_color = ACCENT;
    ctx.set_visuals(visuals);
}

/// 加载系统中文字体（ui.md §4：系统默认 + 中文回退）。
pub fn setup_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "C:/Windows/Fonts/msyh.ttc",          // Windows 微软雅黑
        "C:/Windows/Fonts/simhei.ttf",        // Windows 黑体
        "/System/Library/Fonts/PingFang.ttc", // macOS 苹方
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", // Linux Noto
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "cjk".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("cjk".to_owned());
            }
            ctx.set_fonts(fonts);
            tracing::info!(target: "pano::ui", path, "已加载中文字体");
            return;
        }
    }
    tracing::warn!(target: "pano::ui", "未找到中文字体，中文可能无法显示");
}
