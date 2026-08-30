//! Pano 二进制入口：组装 core + adapters + ui。
//!
//! 启动流程（架构 §7）：
//! 1. 按 feature 构建适配器注册表；
//! 2. 加载 `pano.toml` 配置；
//! 3. 组装 lifecycle（未知适配器 id 在此暴露）；
//! 4. 能力校验：UI 声明 ⊆ 已启用适配器的能力并集；
//! 5. 启动全部启用中的适配器；
//! 6. headless 模式跑固定时长后干净退出；GUI 模式挂载多窗口界面（M1.1）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use pano_core::config::PanoConfig;
use pano_core::lifecycle::Lifecycle;
use pano_core::registry::Registry;

/// Pano：可插拔监控面板。
#[derive(Parser, Debug)]
#[command(name = "pano", version, about = "Pano：可插拔监控面板")]
struct Cli {
    /// 配置文件路径（缺省为当前目录的 pano.toml）。
    #[arg(long, default_value = "pano.toml")]
    config: PathBuf,

    /// 无界面模式：启动后运行指定时长并退出（供无头验证 / CI）。
    #[arg(long)]
    headless: bool,

    /// headless 模式运行时长（秒）。
    #[arg(long, default_value_t = 3)]
    duration: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("创建 tokio runtime 失败")?;

    // 1. 按 feature 构建适配器注册表
    let mut registry = Registry::new();
    for adapter in pano_adapters::build() {
        registry
            .register(adapter)
            .context("注册适配器失败（id 重复？）")?;
    }

    // 2. 加载配置
    let text = std::fs::read_to_string(&cli.config)
        .with_context(|| format!("读取配置失败：{}", cli.config.display()))?;
    let config = PanoConfig::parse(&text).context("解析配置失败")?;

    // 3. 组装 core
    let mut lifecycle = Lifecycle::new(registry, config, rt.handle().clone())
        .context("组装 core 失败（配置引用了未注册的适配器？）")?;

    // 4. 能力校验：UI 声明 vs 已启用适配器
    lifecycle
        .validate_uispec(&pano_ui::uispec())
        .context("UI 能力校验失败")?;

    // 5. 启动
    lifecycle.start_all();

    // 6. 运行（关闭顺序：先停适配器任务，再退进程，见架构 §5）
    if cli.headless {
        run_headless(&mut lifecycle, cli.duration)?;
        lifecycle.stop_all();
    } else {
        run_gui(lifecycle, cli.config)?;
    }
    tracing::info!(target: "pano::app", "Pano 已退出");
    Ok(())
}

/// headless 模式：运行指定秒数，周期打印各 series 最新样本，随后干净退出。
fn run_headless(lifecycle: &mut Lifecycle, seconds: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    tracing::info!(target: "pano::app", seconds, "headless 模式启动");
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        for series in lifecycle.store().series_ids() {
            if let Some(latest) = lifecycle.store().latest(&series) {
                tracing::info!(target: "pano::app", series = %series, value = %latest.value, "样本");
            }
        }
    }
    tracing::info!(target: "pano::app", "headless 模式结束");
    Ok(())
}

/// GUI 模式：挂载 egui 界面（M1.1：多窗口 + 托盘常驻，托盘「退出」才结束进程）。
fn run_gui(lifecycle: Lifecycle, config_path: PathBuf) -> Result<()> {
    let core = Arc::new(Mutex::new(lifecycle));
    let store = core.lock().unwrap_or_else(|e| e.into_inner()).store_arc();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Pano 管理")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(pano_ui::icon::app_icon()),
        ..Default::default()
    };

    let core_for_ui = Arc::clone(&core);
    let result = eframe::run_native(
        "Pano",
        options,
        Box::new(move |cc| {
            pano_ui::theme::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(pano_ui::build_app(
                cc,
                core_for_ui,
                store,
                config_path,
            )))
        }),
    );

    // 托盘「退出」或全部窗口关闭后：停止适配器（先停任务，再退进程）
    if let Ok(mut lc) = core.lock() {
        lc.stop_all();
    }
    result.map_err(|e| anyhow::anyhow!("GUI 启动失败：{e}"))
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_target(true)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}
