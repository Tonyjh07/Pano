//! Pano 图形界面（egui）。
//!
//! 依赖方向铁律：`pano-ui` 只读 `pano-core` 的公共 API，不知道具体适配器。
//! UI 通过 `UISpec` 声明所需能力，由 `pano-app` 启动时校验（架构 §7）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pano_core::capability::{Capability, UISpec};
use pano_core::lifecycle::Lifecycle;
use pano_core::sample_store::SampleStore;

pub mod app;
pub mod pages;
pub mod theme;
pub mod widgets;

/// 本 UI 声明所需的能力（架构 §7）。
///
/// M1 界面需要时间序列能力；能力校验由 `pano-app` 启动时执行。
pub fn uispec() -> UISpec {
    UISpec::requires([Capability::new(Capability::TIME_SERIES)])
}

/// 构建 UI 应用。
pub fn build_app(
    core: Arc<Mutex<Lifecycle>>,
    store: Arc<SampleStore>,
    config_path: PathBuf,
) -> app::PanoApp {
    app::PanoApp::new(core, store, config_path)
}
