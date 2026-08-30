//! Pano 内置适配器集合。
//!
//! 每个适配器一个 feature（`adapter-<名称>`），未启用 feature 的适配器不参与编译。
//! 换适配器 = 改 feature 重新编译（架构 §6）。

#[cfg(feature = "adapter-example-counter")]
mod example_counter;

#[cfg(feature = "adapter-example-sine")]
mod example_sine;

use pano_core::adapter::Adapter;

/// 按已启用的 feature 构建全部适配器实例。
#[allow(clippy::vec_init_then_push)] // cfg 条件下无法用 vec![] 表达
pub fn build() -> Vec<Box<dyn Adapter>> {
    let mut adapters: Vec<Box<dyn Adapter>> = Vec::new();

    #[cfg(feature = "adapter-example-counter")]
    adapters.push(Box::new(example_counter::ExampleCounter::new()));

    #[cfg(feature = "adapter-example-sine")]
    adapters.push(Box::new(example_sine::ExampleSine::new()));

    adapters
}
