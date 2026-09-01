//! Pano 内置适配器集合。
//!
//! 每个适配器一个 feature（`adapter-<名称>`），未启用 feature 的适配器不参与编译。
//! 换适配器 = 改 feature 重新编译（架构 §6）。
//!
//! - `sys`（M2）：系统监控适配器（`sys.cpu` / `sys.mem` / `sys.disk` / `sys.net`，
//!   按平台条件编译：Windows / Linux / macOS，共用 `sysinfo`）；
//! - `remote`：远程数据源共享基座（M1.2 R3 预留，架构 §14）：HTTP 轮询 /
//!   WebSocket 推送模板（M2 起含具体示例适配器）。

pub mod remote;

/// 系统监控适配器（M2，仅 Windows / Linux / macOS；其余平台不注册）。
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
mod sys;

#[cfg(feature = "adapter-example-counter")]
mod example_counter;

#[cfg(feature = "adapter-example-sine")]
mod example_sine;

use pano_core::adapter::Adapter;

/// 按已启用的 feature 构建全部适配器实例。
#[allow(clippy::vec_init_then_push)] // cfg 条件下无法用 vec![] 表达
pub fn build() -> Vec<Box<dyn Adapter>> {
    let mut adapters: Vec<Box<dyn Adapter>> = Vec::new();

    #[cfg(all(
        feature = "adapter-sys-cpu",
        any(target_os = "windows", target_os = "linux", target_os = "macos")
    ))]
    adapters.push(Box::new(sys::cpu::SysCpu::new()));

    #[cfg(all(
        feature = "adapter-sys-mem",
        any(target_os = "windows", target_os = "linux", target_os = "macos")
    ))]
    adapters.push(Box::new(sys::mem::SysMem::new()));

    #[cfg(all(
        feature = "adapter-sys-disk",
        any(target_os = "windows", target_os = "linux", target_os = "macos")
    ))]
    adapters.push(Box::new(sys::disk::SysDisk::new()));

    #[cfg(all(
        feature = "adapter-sys-net",
        any(target_os = "windows", target_os = "linux", target_os = "macos")
    ))]
    adapters.push(Box::new(sys::net::SysNet::new()));

    #[cfg(feature = "adapter-example-counter")]
    adapters.push(Box::new(example_counter::ExampleCounter::new()));

    #[cfg(feature = "adapter-example-sine")]
    adapters.push(Box::new(example_sine::ExampleSine::new()));

    adapters
}
