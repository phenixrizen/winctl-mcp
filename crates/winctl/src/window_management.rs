use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(windows)]
use crate::list_windows;
use crate::WindowInfo;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowManagementAction {
    Move,
    Resize,
    Minimize,
    Maximize,
    Restore,
    Close,
    ForegroundDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowManagementResult {
    pub action: WindowManagementAction,
    pub hwnd: isize,
    pub hwnd_hex: String,
    pub pid: u32,
    pub before: WindowStateSnapshot,
    pub after: Option<WindowStateSnapshot>,
    pub replay_metadata: WindowReplayMetadata,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WindowStateSnapshot {
    pub hwnd_hex: String,
    pub pid: u32,
    pub process_name: Option<String>,
    pub exe_path: Option<String>,
    pub title: String,
    pub class_name: String,
    pub rect: WindowRect,
    pub visible: bool,
    pub minimized: bool,
    pub cloaked: bool,
    pub foreground: bool,
    pub top_level: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WindowReplayMetadata {
    pub virtual_desktop_rect_before: WindowRect,
    pub virtual_desktop_rect_after: Option<WindowRect>,
    pub requested_rect: Option<WindowRect>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct WindowManagementError {
    pub code: String,
    pub message: String,
}

pub fn move_window_to(
    window: &WindowInfo,
    x: i32,
    y: i32,
) -> Result<WindowManagementResult, WindowManagementError> {
    #[cfg(windows)]
    {
        windows_impl::set_window_rect(
            window,
            WindowManagementAction::Move,
            x,
            y,
            window.width,
            window.height,
        )
    }

    #[cfg(not(windows))]
    {
        let _ = (window, x, y);
        unsupported("windows.move")
    }
}

pub fn resize_window_to(
    window: &WindowInfo,
    width: i32,
    height: i32,
) -> Result<WindowManagementResult, WindowManagementError> {
    #[cfg(windows)]
    {
        windows_impl::set_window_rect(
            window,
            WindowManagementAction::Resize,
            window.x,
            window.y,
            width,
            height,
        )
    }

    #[cfg(not(windows))]
    {
        let _ = (window, width, height);
        unsupported("windows.resize")
    }
}

pub fn minimize_window(
    window: &WindowInfo,
) -> Result<WindowManagementResult, WindowManagementError> {
    show_window(window, WindowManagementAction::Minimize)
}

pub fn maximize_window(
    window: &WindowInfo,
) -> Result<WindowManagementResult, WindowManagementError> {
    show_window(window, WindowManagementAction::Maximize)
}

pub fn restore_window(
    window: &WindowInfo,
) -> Result<WindowManagementResult, WindowManagementError> {
    show_window(window, WindowManagementAction::Restore)
}

pub fn close_window(window: &WindowInfo) -> Result<WindowManagementResult, WindowManagementError> {
    #[cfg(windows)]
    {
        windows_impl::close_window(window)
    }

    #[cfg(not(windows))]
    {
        let _ = window;
        unsupported("windows.close")
    }
}

pub fn foreground_diagnostics(
    window: &WindowInfo,
) -> Result<WindowManagementResult, WindowManagementError> {
    Ok(result_from_snapshots(
        WindowManagementAction::ForegroundDiagnostics,
        window,
        None,
        None,
        vec![],
    ))
}

fn show_window(
    window: &WindowInfo,
    action: WindowManagementAction,
) -> Result<WindowManagementResult, WindowManagementError> {
    #[cfg(windows)]
    {
        windows_impl::show_window(window, action)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, action);
        unsupported("windows.show_window")
    }
}

fn result_from_snapshots(
    action: WindowManagementAction,
    window: &WindowInfo,
    requested_rect: Option<WindowRect>,
    after: Option<WindowInfo>,
    warnings: Vec<String>,
) -> WindowManagementResult {
    let before = WindowStateSnapshot::from(window);
    let after = after.as_ref().map(WindowStateSnapshot::from);
    WindowManagementResult {
        action,
        hwnd: window.hwnd,
        hwnd_hex: window.hwnd_hex.clone(),
        pid: window.pid,
        replay_metadata: WindowReplayMetadata {
            virtual_desktop_rect_before: before.rect.clone(),
            virtual_desktop_rect_after: after.as_ref().map(|snapshot| snapshot.rect.clone()),
            requested_rect,
        },
        before,
        after,
        warnings,
    }
}

#[cfg(windows)]
fn find_current_window(window: &WindowInfo) -> Option<WindowInfo> {
    list_windows()
        .into_iter()
        .find(|candidate| candidate.hwnd_hex.eq_ignore_ascii_case(&window.hwnd_hex))
}

#[cfg(not(windows))]
fn unsupported(tool: &str) -> Result<WindowManagementResult, WindowManagementError> {
    Err(WindowManagementError {
        code: "unsupported_platform".into(),
        message: format!("{tool} requires Windows runtime"),
    })
}

impl From<&WindowInfo> for WindowStateSnapshot {
    fn from(window: &WindowInfo) -> Self {
        Self {
            hwnd_hex: window.hwnd_hex.clone(),
            pid: window.pid,
            process_name: window.process_name.clone(),
            exe_path: window.exe_path.clone(),
            title: window.title.clone(),
            class_name: window.class_name.clone(),
            rect: WindowRect {
                x: window.x,
                y: window.y,
                width: window.width,
                height: window.height,
            },
            visible: window.visible,
            minimized: window.minimized,
            cloaked: window.cloaked,
            foreground: window.foreground,
            top_level: window.top_level,
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::thread;
    use std::time::Duration;

    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        PostMessageW, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SW_MAXIMIZE, SW_MINIMIZE,
        SW_RESTORE, WM_CLOSE,
    };

    use super::{
        find_current_window, result_from_snapshots, WindowManagementAction, WindowManagementError,
        WindowManagementResult, WindowRect,
    };
    use crate::WindowInfo;

    pub(super) fn set_window_rect(
        window: &WindowInfo,
        action: WindowManagementAction,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<WindowManagementResult, WindowManagementError> {
        let requested = WindowRect {
            x,
            y,
            width,
            height,
        };
        let ok = unsafe {
            SetWindowPos(
                HWND(window.hwnd as *mut c_void),
                Some(HWND_TOP),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            )
        };
        if ok.is_err() {
            return Err(WindowManagementError {
                code: "set_window_pos_failed".into(),
                message: format!("SetWindowPos failed for {}", window.hwnd_hex),
            });
        }
        thread::sleep(Duration::from_millis(50));
        Ok(result_from_snapshots(
            action,
            window,
            Some(requested),
            find_current_window(window),
            vec![],
        ))
    }

    pub(super) fn show_window(
        window: &WindowInfo,
        action: WindowManagementAction,
    ) -> Result<WindowManagementResult, WindowManagementError> {
        let command = match action {
            WindowManagementAction::Minimize => SW_MINIMIZE,
            WindowManagementAction::Maximize => SW_MAXIMIZE,
            WindowManagementAction::Restore => SW_RESTORE,
            _ => SW_RESTORE,
        };
        let _ = unsafe { ShowWindow(HWND(window.hwnd as *mut c_void), command) };
        thread::sleep(Duration::from_millis(50));
        Ok(result_from_snapshots(
            action,
            window,
            None,
            find_current_window(window),
            vec![],
        ))
    }

    pub(super) fn close_window(
        window: &WindowInfo,
    ) -> Result<WindowManagementResult, WindowManagementError> {
        let ok = unsafe {
            PostMessageW(
                Some(HWND(window.hwnd as *mut c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            )
        };
        if ok.is_err() {
            return Err(WindowManagementError {
                code: "post_close_failed".into(),
                message: format!("failed to post WM_CLOSE to {}", window.hwnd_hex),
            });
        }
        thread::sleep(Duration::from_millis(50));
        Ok(result_from_snapshots(
            WindowManagementAction::Close,
            window,
            None,
            find_current_window(window),
            vec![],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_preserves_replay_rect() {
        let window = WindowInfo {
            id: "hwnd:0x1".into(),
            hwnd: 1,
            hwnd_hex: "0x0000000000000001".into(),
            pid: 42,
            tid: 7,
            process_name: Some("App.exe".into()),
            exe_path: Some("C:/App.exe".into()),
            title: "App".into(),
            class_name: "AppWindow".into(),
            x: -20,
            y: 10,
            width: 640,
            height: 480,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        };

        let result = foreground_diagnostics(&window).unwrap();
        assert_eq!(result.replay_metadata.virtual_desktop_rect_before.x, -20);
        assert_eq!(result.before.pid, 42);
    }
}
