use crate::WindowInfo;

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn hwnd_hex(hwnd: u64) -> String {
    format!("0x{hwnd:016x}")
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn window_id_from_hwnd(hwnd: u64) -> String {
    format!("hwnd:{}", hwnd_hex(hwnd))
}

#[cfg(windows)]
pub fn list_windows() -> Vec<WindowInfo> {
    match windows_impl::list_windows() {
        Ok(windows) => windows,
        Err(error) => {
            tracing::warn!(%error, "failed to enumerate windows");
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
pub fn list_windows() -> Vec<WindowInfo> {
    vec![]
}

pub fn window_by_id(id: &str) -> Option<WindowInfo> {
    list_windows().into_iter().find(|w| w.id == id)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::mem::size_of;

    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, TRUE};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        GA_ROOT,
    };

    use crate::process::basename;
    use crate::window_enum::{hwnd_hex, window_id_from_hwnd};
    use crate::WindowInfo;

    pub(super) fn list_windows() -> anyhow::Result<Vec<WindowInfo>> {
        let mut windows = Vec::new();
        unsafe {
            EnumWindows(
                Some(enum_window_proc),
                LPARAM((&mut windows as *mut Vec<WindowInfo>) as isize),
            )?;
        }
        Ok(windows)
    }

    unsafe extern "system" fn enum_window_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };
        if let Some(info) = unsafe { window_info(hwnd) } {
            windows.push(info);
        }
        TRUE
    }

    unsafe fn window_info(hwnd: HWND) -> Option<WindowInfo> {
        let hwnd_value = hwnd.0 as usize as u64;
        let mut pid = 0;
        let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let rect = unsafe { window_rect(hwnd)? };
        let title = unsafe { window_text(hwnd) };
        let class_name = unsafe { class_name(hwnd) };
        let foreground = unsafe { GetForegroundWindow() == hwnd };
        let top_level = unsafe { GetAncestor(hwnd, GA_ROOT) == hwnd };
        let exe_path = process_exe_path(pid);
        let process_name = exe_path.as_deref().and_then(basename);

        Some(WindowInfo {
            id: window_id_from_hwnd(hwnd_value),
            hwnd: hwnd.0 as isize,
            hwnd_hex: hwnd_hex(hwnd_value),
            pid,
            tid,
            process_name,
            exe_path,
            title,
            class_name,
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0),
            height: (rect.bottom - rect.top).max(0),
            visible: unsafe { IsWindowVisible(hwnd).as_bool() },
            enabled: unsafe { IsWindowEnabled(hwnd).as_bool() },
            foreground,
            minimized: unsafe { IsIconic(hwnd).as_bool() },
            cloaked: unsafe { is_cloaked(hwnd) },
            top_level,
        })
    }

    unsafe fn window_rect(hwnd: HWND) -> Option<RECT> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
        Some(rect)
    }

    unsafe fn window_text(hwnd: HWND) -> String {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return String::new();
        }

        let mut buffer = vec![0u16; len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        wide_to_string(&buffer, copied.max(0) as usize)
    }

    unsafe fn class_name(hwnd: HWND) -> String {
        let mut buffer = vec![0u16; 256];
        let copied = unsafe { GetClassNameW(hwnd, &mut buffer) };
        wide_to_string(&buffer, copied.max(0) as usize)
    }

    unsafe fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked = 0u32;
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast::<c_void>(),
                size_of::<u32>() as u32,
            )
        }
        .is_ok()
            && cloaked != 0
    }

    fn process_exe_path(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }

        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()? };
        let mut buffer = vec![0u16; 32_768];
        let mut size = buffer.len() as u32;
        let result = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };
        let _ = unsafe { CloseHandle(process) };

        result.ok()?;
        Some(wide_to_string(&buffer, size as usize))
    }

    fn wide_to_string(buffer: &[u16], len: usize) -> String {
        let end = len.min(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hwnd_identity_strings_are_stable_width() {
        assert_eq!(hwnd_hex(0x1234), "0x0000000000001234");
        assert_eq!(window_id_from_hwnd(0x1234), "hwnd:0x0000000000001234");
    }
}
