//! 共享窗口状态（ui.md §2：多窗口 + 托盘）。
//!
//! 组件窗口的 UI 回调是 `show_viewport_deferred` 要求的 `'static` 闭包，
//! 无法直接访问 `PanoApp`，因此窗口可见性 / 置顶 / 全屏等状态放入
//! `Arc<Mutex<..>>` 共享，由主线程（PanoApp）与组件窗口回调共同读写。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 单个组件窗口的运行时状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComponentState {
    /// 窗口是否可见（关闭 = 隐藏，窗口对象保留，可从托盘重新打开）。
    pub visible: bool,
    /// 置顶（`WindowLevel::AlwaysOnTop`）。
    pub pinned: bool,
    /// 全屏。
    pub fullscreen: bool,
}

#[derive(Default)]
struct Inner {
    /// series id → 组件窗口状态。
    components: HashMap<String, ComponentState>,
    /// 托盘「退出」已请求（组件窗口回调据此放行关闭请求）。
    quit_requested: bool,
}

/// 多窗口共享状态句柄（clone 即共享同一份状态）。
#[derive(Clone, Default)]
pub struct WindowState {
    inner: Arc<Mutex<Inner>>,
}

impl WindowState {
    /// 读取指定 series 的组件窗口状态（不存在时返回默认值）。
    pub fn component(&self, series: &str) -> ComponentState {
        self.with(|inner| inner.components.get(series).copied().unwrap_or_default())
    }

    /// 整体写回指定 series 的组件窗口状态。
    pub fn set_component(&self, series: &str, state: ComponentState) {
        self.with_mut(|inner| {
            inner.components.insert(series.to_string(), state);
        });
    }

    /// 仅更新可见性（托盘「打开」命令使用）。
    pub fn set_component_visible(&self, series: &str, visible: bool) {
        self.with_mut(|inner| {
            inner
                .components
                .entry(series.to_string())
                .or_default()
                .visible = visible;
        });
    }

    /// 移除已不存在的 series 条目（series 数量变化时同步）。
    pub fn retain_series(&self, series: &[String]) {
        self.with_mut(|inner| {
            inner
                .components
                .retain(|key, _| series.iter().any(|s| s == key));
        });
    }

    /// 首次出现的 series 默认打开组件窗口（仅在从未初始化时生效；
    /// 用户主动隐藏过的窗口保持隐藏）。
    pub fn init_component(&self, series: &str) {
        self.with_mut(|inner| {
            inner
                .components
                .entry(series.to_string())
                .or_insert_with(|| ComponentState {
                    visible: true,
                    ..Default::default()
                });
        });
    }

    /// 是否已请求退出（托盘「退出」）。
    pub fn quit_requested(&self) -> bool {
        self.with(|inner| inner.quit_requested)
    }

    /// 请求退出（组件窗口回调据此放行关闭请求）。
    pub fn request_quit(&self) {
        self.with_mut(|inner| inner.quit_requested = true);
    }

    fn with<T>(&self, f: impl FnOnce(&Inner) -> T) -> T {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    fn with_mut<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_defaults_hidden() {
        let ws = WindowState::default();
        assert_eq!(ws.component("a.b"), ComponentState::default());
        assert!(!ws.component("a.b").visible);
    }

    #[test]
    fn set_and_read_component() {
        let ws = WindowState::default();
        ws.set_component(
            "a.b",
            ComponentState {
                visible: true,
                pinned: true,
                fullscreen: false,
            },
        );
        let s = ws.component("a.b");
        assert!(s.visible && s.pinned && !s.fullscreen);
    }

    #[test]
    fn init_component_only_first_time() {
        let ws = WindowState::default();
        // 首次初始化 → 可见。
        ws.init_component("a.b");
        assert!(ws.component("a.b").visible);
        // 用户隐藏后再次 init 不得覆盖（保持隐藏）。
        ws.set_component_visible("a.b", false);
        ws.init_component("a.b");
        assert!(!ws.component("a.b").visible);
    }

    #[test]
    fn retain_series_cleans_stale() {
        let ws = WindowState::default();
        ws.init_component("a.1");
        ws.init_component("a.2");
        ws.retain_series(&["a.1".to_string()]);
        assert!(ws.component("a.1").visible);
        assert_eq!(ws.component("a.2"), ComponentState::default());
    }

    #[test]
    fn quit_flag_roundtrip() {
        let ws = WindowState::default();
        assert!(!ws.quit_requested());
        ws.request_quit();
        assert!(ws.quit_requested());
    }
}
