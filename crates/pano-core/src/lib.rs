//! Pano 核心编排层。
//!
//! 依赖方向铁律：`pano-core` 不依赖 UI 与任何具体适配器，
//! 只通过 [`adapter::Adapter`] trait 与适配器交互。

pub mod adapter;
pub mod capability;
pub mod config;
pub mod error;
pub mod lifecycle;
pub mod registry;
pub mod sample_store;
pub mod test_harness;
