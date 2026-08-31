//! 核心编排层错误。

use crate::adapter::{AdapterError, AdapterId};
use crate::capability::Capability;

/// 核心编排层错误。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// 引用了未注册的适配器。
    #[error("未知适配器：{0}")]
    UnknownAdapter(AdapterId),

    /// 适配器 id 格式非法（应为 `<域>.<名称>`，全小写 ASCII、连字符分隔）。
    #[error("适配器 id 格式非法（应为 <域>.<名称>，全小写 ASCII、连字符分隔）：{0}")]
    InvalidAdapterId(String),

    /// 适配器启动 / 停止失败（错误分类见 spec §6，M1 审查待办）。
    #[error("适配器错误：{0}")]
    Adapter(#[from] AdapterError),

    /// 试图启动已在运行的适配器。
    #[error("适配器已在运行：{0}")]
    AlreadyRunning(AdapterId),

    /// 试图停止未运行的适配器。
    #[error("适配器未在运行：{0}")]
    NotRunning(AdapterId),

    /// 注册表出现重复的适配器 id。
    #[error("重复的适配器 id：{0}")]
    DuplicateAdapter(AdapterId),

    /// 已启用适配器的能力并集不满足 UISpec 声明。
    #[error("能力不满足，缺少：{missing:?}")]
    CapabilityUnsatisfied { missing: Vec<Capability> },

    /// 配置解析 / 校验错误。
    #[error("配置错误：{0}")]
    Config(String),
}
