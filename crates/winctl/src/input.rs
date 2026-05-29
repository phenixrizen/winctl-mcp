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
pub struct MouseMoveRequest {
    pub bound_id: String,
    pub x: f64,
    pub y: f64,
    pub coordinate_space: CoordinateSpace,
    pub fail_if_outside_bound: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DoubleClickRequest {
    pub bound_id: String,
    pub x: f64,
    pub y: f64,
    pub coordinate_space: CoordinateSpace,
    pub button: Option<String>,
    pub interval_ms: Option<u64>,
    pub fail_if_outside_bound: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DragRequest {
    pub bound_id: String,
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub coordinate_space: CoordinateSpace,
    pub button: Option<String>,
    pub duration_ms: Option<u64>,
    pub fail_if_outside_bound: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScrollRequest {
    pub bound_id: String,
    pub x: f64,
    pub y: f64,
    pub coordinate_space: CoordinateSpace,
    pub delta_x: Option<i32>,
    pub delta_y: Option<i32>,
    pub fail_if_outside_bound: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyRequest {
    pub bound_id: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShortcutRequest {
    pub bound_id: String,
    pub keys: Vec<String>,
    pub hold_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DelayRequest {
    pub duration_ms: u64,
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
    InvalidKey,
    InvalidShortcut,
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
pub struct MouseMoveResult {
    pub screen_x: i32,
    pub screen_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DoubleClickResult {
    pub screen_x: i32,
    pub screen_y: i32,
    pub button: MouseButton,
    pub interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DragResult {
    pub start_screen_x: i32,
    pub start_screen_y: i32,
    pub end_screen_x: i32,
    pub end_screen_y: i32,
    pub button: MouseButton,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScrollResult {
    pub screen_x: i32,
    pub screen_y: i32,
    pub delta_x: i32,
    pub delta_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct VirtualKeySpec {
    pub key: String,
    pub code: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyResult {
    pub key: String,
    pub virtual_key: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShortcutResult {
    pub keys: Vec<VirtualKeySpec>,
    pub hold_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DelayResult {
    pub requested_duration_ms: u64,
    pub elapsed_ms: u64,
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

pub fn parse_virtual_key(key: &str) -> Result<VirtualKeySpec, InputActionError> {
    let normalized = key.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(invalid_key(key));
    }

    let code = match normalized.as_str() {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" | "menu" => 0x12,
        "win" | "windows" | "meta" => 0x5b,
        "enter" | "return" => 0x0d,
        "esc" | "escape" => 0x1b,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2e,
        "insert" | "ins" => 0x2d,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "page_up" => 0x21,
        "pagedown" | "page_down" => 0x22,
        "left" | "arrowleft" | "arrow_left" => 0x25,
        "up" | "arrowup" | "arrow_up" => 0x26,
        "right" | "arrowright" | "arrow_right" => 0x27,
        "down" | "arrowdown" | "arrow_down" => 0x28,
        other if other.len() == 1 => {
            let ch = other.as_bytes()[0];
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase() as u16
            } else {
                return Err(invalid_key(key));
            }
        }
        other if other.starts_with('f') => {
            let number = other[1..].parse::<u16>().map_err(|_| invalid_key(key))?;
            if (1..=24).contains(&number) {
                0x70 + number - 1
            } else {
                return Err(invalid_key(key));
            }
        }
        _ => return Err(invalid_key(key)),
    };

    Ok(VirtualKeySpec {
        key: key.into(),
        code,
    })
}

pub fn parse_shortcut_keys(keys: &[String]) -> Result<Vec<VirtualKeySpec>, InputActionError> {
    if keys.is_empty() {
        return Err(InputActionError {
            code: InputActionErrorCode::InvalidShortcut,
            message: "shortcut requires at least one key".into(),
        });
    }
    keys.iter().map(|key| parse_virtual_key(key)).collect()
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

pub fn move_mouse_screen(point: &ResolvedPoint) -> Result<MouseMoveResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::move_mouse_screen(point)
    }

    #[cfg(not(windows))]
    {
        let _ = point;
        Err(unsupported_platform())
    }
}

pub fn double_click_screen(
    point: &ResolvedPoint,
    button: MouseButton,
    interval_ms: u64,
) -> Result<DoubleClickResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::double_click_screen(point, button, interval_ms)
    }

    #[cfg(not(windows))]
    {
        let _ = (point, button, interval_ms);
        Err(unsupported_platform())
    }
}

pub fn drag_screen(
    start: &ResolvedPoint,
    end: &ResolvedPoint,
    button: MouseButton,
    duration_ms: u64,
) -> Result<DragResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::drag_screen(start, end, button, duration_ms)
    }

    #[cfg(not(windows))]
    {
        let _ = (start, end, button, duration_ms);
        Err(unsupported_platform())
    }
}

pub fn scroll_screen(
    point: &ResolvedPoint,
    delta_x: i32,
    delta_y: i32,
) -> Result<ScrollResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::scroll_screen(point, delta_x, delta_y)
    }

    #[cfg(not(windows))]
    {
        let _ = (point, delta_x, delta_y);
        Err(unsupported_platform())
    }
}

pub fn key_down_virtual(key: &VirtualKeySpec) -> Result<KeyResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::key_down_virtual(key)
    }

    #[cfg(not(windows))]
    {
        let _ = key;
        Err(unsupported_platform())
    }
}

pub fn key_up_virtual(key: &VirtualKeySpec) -> Result<KeyResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::key_up_virtual(key)
    }

    #[cfg(not(windows))]
    {
        let _ = key;
        Err(unsupported_platform())
    }
}

pub fn shortcut_virtual(
    keys: &[VirtualKeySpec],
    hold_ms: u64,
) -> Result<ShortcutResult, InputActionError> {
    #[cfg(windows)]
    {
        windows_impl::shortcut_virtual(keys, hold_ms)
    }

    #[cfg(not(windows))]
    {
        let _ = (keys, hold_ms);
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

fn invalid_key(key: &str) -> InputActionError {
    InputActionError {
        code: InputActionErrorCode::InvalidKey,
        message: format!("unsupported virtual key: {key}"),
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
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_HWHEEL,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, SetCursorPos, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    use crate::input::{
        unicode_input_units, ClickResult, DoubleClickResult, DragResult, FocusResult,
        InputActionError, InputActionErrorCode, KeyResult, MouseButton, MouseMoveResult,
        ResolvedPoint, ScrollResult, ShortcutResult, TypeTextResult, VirtualKeySpec,
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

    pub(super) fn move_mouse_screen(
        point: &ResolvedPoint,
    ) -> Result<MouseMoveResult, InputActionError> {
        unsafe { SetCursorPos(point.screen_x, point.screen_y) }
            .map_err(|_| dispatch_error("SetCursorPos failed"))?;
        Ok(MouseMoveResult {
            screen_x: point.screen_x,
            screen_y: point.screen_y,
        })
    }

    pub(super) fn double_click_screen(
        point: &ResolvedPoint,
        button: MouseButton,
        interval_ms: u64,
    ) -> Result<DoubleClickResult, InputActionError> {
        unsafe { SetCursorPos(point.screen_x, point.screen_y) }
            .map_err(|_| dispatch_error("SetCursorPos failed"))?;
        let (down, up) = mouse_button_flags(button);
        send_inputs(&[mouse_input(down), mouse_input(up)])?;
        sleep(Duration::from_millis(interval_ms.min(2_000)));
        send_inputs(&[mouse_input(down), mouse_input(up)])?;
        Ok(DoubleClickResult {
            screen_x: point.screen_x,
            screen_y: point.screen_y,
            button,
            interval_ms,
        })
    }

    pub(super) fn drag_screen(
        start: &ResolvedPoint,
        end: &ResolvedPoint,
        button: MouseButton,
        duration_ms: u64,
    ) -> Result<DragResult, InputActionError> {
        unsafe { SetCursorPos(start.screen_x, start.screen_y) }
            .map_err(|_| dispatch_error("SetCursorPos failed"))?;
        let (down, up) = mouse_button_flags(button);
        send_inputs(&[mouse_input(down)])?;

        let steps = ((duration_ms / 16).clamp(1, 120)) as i32;
        let sleep_ms = (duration_ms / steps as u64).min(100);
        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            let x = start.screen_x as f64 + (end.screen_x - start.screen_x) as f64 * t;
            let y = start.screen_y as f64 + (end.screen_y - start.screen_y) as f64 * t;
            unsafe { SetCursorPos(x.round() as i32, y.round() as i32) }
                .map_err(|_| dispatch_error("SetCursorPos failed"))?;
            if sleep_ms > 0 {
                sleep(Duration::from_millis(sleep_ms));
            }
        }

        send_inputs(&[mouse_input(up)])?;
        Ok(DragResult {
            start_screen_x: start.screen_x,
            start_screen_y: start.screen_y,
            end_screen_x: end.screen_x,
            end_screen_y: end.screen_y,
            button,
            duration_ms,
        })
    }

    pub(super) fn scroll_screen(
        point: &ResolvedPoint,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<ScrollResult, InputActionError> {
        unsafe { SetCursorPos(point.screen_x, point.screen_y) }
            .map_err(|_| dispatch_error("SetCursorPos failed"))?;
        let mut inputs = Vec::new();
        if delta_y != 0 {
            inputs.push(mouse_wheel_input(MOUSEEVENTF_WHEEL, delta_y));
        }
        if delta_x != 0 {
            inputs.push(mouse_wheel_input(MOUSEEVENTF_HWHEEL, delta_x));
        }
        if !inputs.is_empty() {
            send_inputs(&inputs)?;
        }
        Ok(ScrollResult {
            screen_x: point.screen_x,
            screen_y: point.screen_y,
            delta_x,
            delta_y,
        })
    }

    pub(super) fn key_down_virtual(key: &VirtualKeySpec) -> Result<KeyResult, InputActionError> {
        send_inputs(&[virtual_key_input(key.code, false)])?;
        Ok(KeyResult {
            key: key.key.clone(),
            virtual_key: key.code,
        })
    }

    pub(super) fn key_up_virtual(key: &VirtualKeySpec) -> Result<KeyResult, InputActionError> {
        send_inputs(&[virtual_key_input(key.code, true)])?;
        Ok(KeyResult {
            key: key.key.clone(),
            virtual_key: key.code,
        })
    }

    pub(super) fn shortcut_virtual(
        keys: &[VirtualKeySpec],
        hold_ms: u64,
    ) -> Result<ShortcutResult, InputActionError> {
        let mut inputs = Vec::with_capacity(keys.len() * 2);
        for key in keys {
            inputs.push(virtual_key_input(key.code, false));
        }
        send_inputs(&inputs)?;
        sleep(Duration::from_millis(hold_ms.min(2_000)));

        inputs.clear();
        for key in keys.iter().rev() {
            inputs.push(virtual_key_input(key.code, true));
        }
        send_inputs(&inputs)?;
        Ok(ShortcutResult {
            keys: keys.to_vec(),
            hold_ms,
        })
    }

    pub(super) fn type_text_unicode(text: &str) -> Result<TypeTextResult, InputActionError> {
        let units = unicode_input_units(text);
        let mut inputs = Vec::with_capacity(units.len() * 2);
        for unit in &units {
            inputs.push(unicode_key_input(*unit, KEYEVENTF_UNICODE));
            inputs.push(unicode_key_input(
                *unit,
                KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
            ));
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

    fn mouse_button_flags(
        button: MouseButton,
    ) -> (
        windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
        windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    ) {
        match button {
            MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        }
    }

    fn unicode_key_input(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
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

    fn virtual_key_input(code: u16, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(code),
                    wScan: 0,
                    dwFlags: if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
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

    fn mouse_wheel_input(
        flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
        delta: i32,
    ) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: delta as u32,
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

    #[test]
    fn virtual_key_parser_supports_common_shortcut_keys() {
        let keys = parse_shortcut_keys(&["ctrl".into(), "shift".into(), "s".into()]).unwrap();

        assert_eq!(
            keys,
            vec![
                VirtualKeySpec {
                    key: "ctrl".into(),
                    code: 0x11
                },
                VirtualKeySpec {
                    key: "shift".into(),
                    code: 0x10
                },
                VirtualKeySpec {
                    key: "s".into(),
                    code: 0x53
                }
            ]
        );
    }

    #[test]
    fn virtual_key_parser_rejects_unknown_keys() {
        let error = parse_virtual_key("not-a-key").unwrap_err();

        assert_eq!(error.code, InputActionErrorCode::InvalidKey);
    }
}
