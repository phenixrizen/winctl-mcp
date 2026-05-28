use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::WindowInfo;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    ScreenPixels,
    WindowPixels,
    ClientPixels,
    NormalizedWindow,
    NormalizedClient,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClickRequest {
    pub bound_id: String,
    pub x: f64,
    pub y: f64,
    pub coordinate_space: CoordinateSpace,
    pub button: Option<String>,
    pub fail_if_outside_bound: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TypeTextRequest {
    pub bound_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputActionErrorCode {
    UnsupportedPlatform,
    InvalidButton,
    FocusFailed,
    DispatchFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct InputActionError {
    pub code: InputActionErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FocusResult {
    pub hwnd: isize,
    pub attempts: u32,
    pub foreground: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClickResult {
    pub screen_x: i32,
    pub screen_y: i32,
    pub button: MouseButton,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TypeTextResult {
    pub utf16_units: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedPoint {
    pub screen_x: i32,
    pub screen_y: i32,
    pub coordinate_space: CoordinateSpace,
}

#[derive(Debug, Clone, Error)]
pub enum CoordinateResolutionError {
    #[error("coordinate space requires a bound window")]
    WindowRequired,
}

pub fn resolve_screen_point(
    window: Option<&WindowInfo>,
    x: f64,
    y: f64,
    coordinate_space: &CoordinateSpace,
) -> Result<ResolvedPoint, CoordinateResolutionError> {
    let (screen_x, screen_y) = match coordinate_space {
        CoordinateSpace::ScreenPixels => (x.round() as i32, y.round() as i32),
        CoordinateSpace::WindowPixels | CoordinateSpace::ClientPixels => {
            let window = window.ok_or(CoordinateResolutionError::WindowRequired)?;
            (window.x + x.round() as i32, window.y + y.round() as i32)
        }
        CoordinateSpace::NormalizedWindow | CoordinateSpace::NormalizedClient => {
            let window = window.ok_or(CoordinateResolutionError::WindowRequired)?;
            (
                window.x + (x * window.width as f64).round() as i32,
                window.y + (y * window.height as f64).round() as i32,
            )
        }
    };

    Ok(ResolvedPoint {
        screen_x,
        screen_y,
        coordinate_space: coordinate_space.clone(),
    })
}

pub fn parse_mouse_button(button: Option<&str>) -> Result<MouseButton, InputActionError> {
    match button.unwrap_or("left").to_ascii_lowercase().as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        other => Err(InputActionError {
            code: InputActionErrorCode::InvalidButton,
            message: format!("unsupported mouse button: {other}"),
        }),
    }
}

pub fn unicode_input_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub fn focus_window(window: &WindowInfo) -> Result<FocusResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::focus_window(window)
    }

    #[cfg(not(windows))]
    {
        let _ = window;
        Err(unsupported_platform())
    }
}

pub fn click_screen(
    point: &ResolvedPoint,
    button: MouseButton,
) -> Result<ClickResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::click_screen(point, button)
    }

    #[cfg(not(windows))]
    {
        let _ = (point, button);
        Err(unsupported_platform())
    }
}

pub fn type_text_unicode(text: &str) -> Result<TypeTextResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::type_text_unicode(text)
    }

    #[cfg(not(windows))]
    {
        let _ = text;
        Err(unsupported_platform())
    }
}

#[cfg(not(windows))]
fn unsupported_platform() -> InputActionError {
    InputActionError {
        code: InputActionErrorCode::UnsupportedPlatform,
        message: "input actions require Windows runtime".into(),
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::thread::sleep;
    use std::time::Duration;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, SetCursorPos, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    use crate::input::{
        unicode_input_units, ClickResult, FocusResult, InputActionError, InputActionErrorCode,
        MouseButton, ResolvedPoint, TypeTextResult,
    };
    use crate::WindowInfo;

    pub(super) fn focus_window(window: &WindowInfo) -> Result<FocusResult, InputActionError> {
        let hwnd = hwnd(window);
        if hwnd.0.is_null() {
            return Err(dispatch_error("window HWND is null"));
        }

        if window.minimized {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }

        for attempts in 1..=5 {
            unsafe {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
            }
            sleep(Duration::from_millis(50));
            let foreground = unsafe { GetForegroundWindow() == hwnd };
            if foreground {
                return Ok(FocusResult {
                    hwnd: window.hwnd,
                    attempts,
                    foreground,
                });
            }
        }

        Err(InputActionError {
            code: InputActionErrorCode::FocusFailed,
            message: "failed to make bound window foreground".into(),
        })
    }

    pub(super) fn click_screen(
        point: &ResolvedPoint,
        button: MouseButton,
    ) -> Result<ClickResult, InputActionError> {
        unsafe { SetCursorPos(point.screen_x, point.screen_y) }
            .map_err(|_| dispatch_error("SetCursorPos failed"))?;

        let (down, up) = match button {
            MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        };
        send_inputs(&[mouse_input(down), mouse_input(up)])?;

        Ok(ClickResult {
            screen_x: point.screen_x,
            screen_y: point.screen_y,
            button,
        })
    }

    pub(super) fn type_text_unicode(text: &str) -> Result<TypeTextResult, InputActionError> {
        let units = unicode_input_units(text);
        let mut inputs = Vec::with_capacity(units.len() * 2);
        for unit in &units {
            inputs.push(key_input(*unit, KEYEVENTF_UNICODE));
            inputs.push(key_input(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }

        send_inputs(&inputs)?;
        Ok(TypeTextResult {
            utf16_units: units.len(),
        })
    }

    fn hwnd(window: &WindowInfo) -> HWND {
        HWND(window.hwnd as *mut c_void)
    }

    fn send_inputs(inputs: &[INPUT]) -> Result<(), InputActionError> {
        let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err(dispatch_error("SendInput sent fewer events than requested"))
        }
    }

    fn key_input(
        unit: u16,
        flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
    ) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn mouse_input(flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn dispatch_error(message: &str) -> InputActionError {
        InputActionError {
            code: InputActionErrorCode::DispatchFailed,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowInfo;

    fn window() -> WindowInfo {
        WindowInfo {
            id: "hwnd:0x1".into(),
            hwnd: 1,
            hwnd_hex: "0x1".into(),
            pid: 1,
            tid: 1,
            process_name: Some("app.exe".into()),
            exe_path: Some("C:/app.exe".into()),
            title: "App".into(),
            class_name: "App".into(),
            x: 10,
            y: 20,
            width: 200,
            height: 400,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        }
    }

    #[test]
    fn normalized_window_coordinates_resolve_to_screen_pixels() {
        let point = resolve_screen_point(
            Some(&window()),
            0.5,
            0.25,
            &CoordinateSpace::NormalizedWindow,
        )
        .unwrap();

        assert_eq!(point.screen_x, 110);
        assert_eq!(point.screen_y, 120);
    }

    #[test]
    fn unicode_input_units_preserve_surrogate_pairs() {
        assert_eq!(unicode_input_units("A💖"), vec![0x0041, 0xd83d, 0xdc96]);
    }
}
