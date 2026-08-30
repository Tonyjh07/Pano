//! 系统托盘常驻（ui.md §2 / §9）。
//!
//! 托盘菜单事件经 muda 全局 channel 泵送：`build` 时 spawn 一个线程循环
//! `MenuEvent::receiver().recv()`，把命令写入共享队列并唤醒 egui 重绘；
//! 主线程帧内用 [`TrayController::take_commands`] 消费命令。
//! 菜单（含监控组件窗口列表）在组件列表变化时经 [`TrayController::rebuild_menu`] 重建。

use std::io;
use std::sync::{Arc, Mutex};

use eframe::egui;
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::windows::WindowState;

/// 菜单项 id 常量。
const MENU_OPEN_MANAGER: &str = "open_manager";
const MENU_QUIT: &str = "quit";
const MENU_COMPONENT_PREFIX: &str = "component:";
const MENU_COMPONENT_SUBMENU: &str = "components";

/// 托盘菜单命令（由事件线程写入，主线程帧内消费）。
#[derive(Default, Clone, Debug)]
pub struct TrayCommands {
    /// 打开 / 聚焦管理窗口。
    pub open_manager: bool,
    /// 打开 / 聚焦指定 series 的组件窗口。
    pub open_component: Option<String>,
    /// 退出应用。
    pub quit: bool,
}

/// 托盘控制器：持有托盘图标实例（drop 即消失）与命令队列。
pub struct TrayController {
    tray: tray_icon::TrayIcon,
    commands: Arc<Mutex<TrayCommands>>,
}

impl TrayController {
    /// 创建托盘并启动菜单事件泵送线程。
    ///
    /// 失败（无托盘环境等）时由调用方降级为无托盘模式（关闭窗口即退出）。
    pub fn build(
        ctx: &egui::Context,
        window_state: &WindowState,
    ) -> Result<Self, tray_icon::Error> {
        let commands = Arc::new(Mutex::new(TrayCommands::default()));

        let menu = build_menu(&[], window_state);
        let icon = crate::icon::tray_icon()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "托盘图标解码失败"))?;

        let tray = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Pano 监控面板")
            .with_icon(icon)
            .build()?;

        // 菜单事件泵送线程：muda 全局 channel，任意线程可 recv。
        let cmd = Arc::clone(&commands);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().recv() {
                handle_menu_event(&event, &cmd);
                ctx.request_repaint();
            }
        });

        Ok(Self { tray, commands })
    }

    /// 取出并清空待处理命令（主线程帧内调用）。
    pub fn take_commands(&self) -> TrayCommands {
        let mut guard = self.commands.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *guard)
    }

    /// 组件列表变化时重建菜单（监控组件窗口列表动态）。
    pub fn rebuild_menu(&self, series: &[String], window_state: &WindowState) {
        self.tray
            .set_menu(Some(Box::new(build_menu(series, window_state))));
    }
}

/// 构建托盘菜单：打开管理窗口 / 监控组件窗口子菜单 / 退出。
fn build_menu(series: &[String], window_state: &WindowState) -> Menu {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::with_id(
        MENU_OPEN_MANAGER,
        "打开管理窗口",
        true,
        None,
    ));

    if series.is_empty() {
        let _ = menu.append(&MenuItem::with_id(
            "no-components",
            "监控组件窗口（无）",
            false,
            None,
        ));
    } else {
        let submenu = Submenu::with_id(MENU_COMPONENT_SUBMENU, "监控组件窗口", true);
        for sid in series {
            let state = window_state.component(sid);
            let label = if state.visible {
                format!("{sid}（已打开）")
            } else {
                sid.clone()
            };
            let _ = submenu.append(&MenuItem::with_id(
                format!("{MENU_COMPONENT_PREFIX}{sid}"),
                label,
                true,
                None,
            ));
        }
        let _ = menu.append(&submenu);
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(MENU_QUIT, "退出", true, None));
    menu
}

/// 把菜单事件写入共享命令队列。
fn handle_menu_event(event: &tray_icon::menu::MenuEvent, commands: &Arc<Mutex<TrayCommands>>) {
    let id = &event.id().0;
    let mut guard = commands.lock().unwrap_or_else(|e| e.into_inner());
    match id.as_str() {
        MENU_OPEN_MANAGER => guard.open_manager = true,
        MENU_QUIT => guard.quit = true,
        s if s.starts_with(MENU_COMPONENT_PREFIX) => {
            guard.open_component = Some(s[MENU_COMPONENT_PREFIX.len()..].to_string());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tray_icon::menu::MenuEvent;

    fn make_commands() -> Arc<Mutex<TrayCommands>> {
        Arc::new(Mutex::new(TrayCommands::default()))
    }

    #[test]
    fn menu_event_mapping() {
        let cmds = make_commands();
        handle_menu_event(
            &MenuEvent {
                id: MENU_OPEN_MANAGER.into(),
            },
            &cmds,
        );
        handle_menu_event(
            &MenuEvent {
                id: "component:example.counter.value".into(),
            },
            &cmds,
        );
        handle_menu_event(
            &MenuEvent {
                id: MENU_QUIT.into(),
            },
            &cmds,
        );
        let guard = cmds.lock().unwrap_or_else(|e| e.into_inner());
        assert!(guard.open_manager);
        assert_eq!(
            guard.open_component.as_deref(),
            Some("example.counter.value")
        );
        assert!(guard.quit);
    }

    #[test]
    fn unknown_menu_event_ignored() {
        let cmds = make_commands();
        handle_menu_event(
            &MenuEvent {
                id: "something-else".into(),
            },
            &cmds,
        );
        let guard = cmds.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!guard.open_manager);
        assert!(guard.open_component.is_none());
        assert!(!guard.quit);
    }
}
