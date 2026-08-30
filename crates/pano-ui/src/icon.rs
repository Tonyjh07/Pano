//! 图标资源（ui.md §9）：窗口图标 + 托盘图标。
//!
//! 源图 `assets/pano_icon.png`（512×512 RGBA）经 `include_bytes!` 嵌入二进制，
//! 运行期解码：窗口图标用原尺寸（由系统缩放），托盘图标缩放到 32×32。

use std::sync::{Arc, OnceLock};

use eframe::egui;

/// 托盘图标目标尺寸（tray-icon 建议 32×32）。
const TRAY_ICON_SIZE: u32 = 32;

/// 应用窗口图标（原尺寸；失败时回退默认图标）。
///
/// PNG 解码只做一次（`OnceLock` 缓存），避免每帧重复解码 512×512 位图。
pub fn app_icon() -> Arc<egui::IconData> {
    static CACHE: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    CACHE
        .get_or_init(|| match decode_rgba() {
            Some((rgba, width, height)) => Arc::new(egui::IconData {
                rgba,
                width,
                height,
            }),
            None => Arc::new(egui::IconData::default()),
        })
        .clone()
}

/// 托盘图标（缩放到 32×32；仅创建托盘时调用一次，无需缓存——
/// `tray_icon::Icon` 持有平台原生句柄，非 `Send`/`Sync`，不能入 static）。
pub fn tray_icon() -> Option<tray_icon::Icon> {
    let (rgba, width, height) = decode_rgba()?;
    let img = image::RgbaImage::from_raw(width, height, rgba)?;
    let small = image::imageops::resize(
        &img,
        TRAY_ICON_SIZE,
        TRAY_ICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    tray_icon::Icon::from_rgba(small.to_vec(), TRAY_ICON_SIZE, TRAY_ICON_SIZE).ok()
}

/// 解码嵌入的 PNG 为 RGBA 像素。
fn decode_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let bytes = include_bytes!("../assets/pano_icon.png");
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            Some((rgba.to_vec(), rgba.width(), rgba.height()))
        }
        Err(e) => {
            tracing::error!(target: "pano::ui", error = %e, "解码应用图标失败");
            None
        }
    }
}
