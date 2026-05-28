use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dpi_scale: f32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VirtualDesktopInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub monitors: Vec<MonitorInfo>,
}

pub fn monitors() -> VirtualDesktopInfo {
    #[cfg(windows)]
    {
        match windows_impl::monitors() {
            Ok(info) => info,
            Err(error) => {
                tracing::warn!(%error, "failed to enumerate monitors");
                virtual_desktop_from_monitors(Vec::new())
            }
        }
    }

    #[cfg(not(windows))]
    {
        virtual_desktop_from_monitors(Vec::new())
    }
}

pub fn virtual_desktop_from_monitors(monitors: Vec<MonitorInfo>) -> VirtualDesktopInfo {
    if monitors.is_empty() {
        return VirtualDesktopInfo {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            monitors,
        };
    }

    let min_x = monitors.iter().map(|monitor| monitor.x).min().unwrap_or(0);
    let min_y = monitors.iter().map(|monitor| monitor.y).min().unwrap_or(0);
    let max_x = monitors
        .iter()
        .map(|monitor| monitor.x + monitor.width)
        .max()
        .unwrap_or(0);
    let max_y = monitors
        .iter()
        .map(|monitor| monitor.y + monitor.height)
        .max()
        .unwrap_or(0);

    VirtualDesktopInfo {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
        monitors,
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::mem::size_of;

    use anyhow::Context;
    use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    use crate::monitors::{virtual_desktop_from_monitors, MonitorInfo, VirtualDesktopInfo};

    pub(super) fn monitors() -> anyhow::Result<VirtualDesktopInfo> {
        let mut monitors = Vec::new();
        let ok = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(enum_monitor_proc),
                LPARAM((&mut monitors as *mut Vec<MonitorInfo>) as isize),
            )
        };
        if !ok.as_bool() {
            anyhow::bail!("EnumDisplayMonitors failed");
        }

        Ok(virtual_desktop_from_monitors(monitors))
    }

    unsafe extern "system" fn enum_monitor_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> windows::core::BOOL {
        let monitors = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
        if let Ok(info) = unsafe { monitor_info(hmonitor) } {
            monitors.push(info);
        }
        TRUE
    }

    unsafe fn monitor_info(hmonitor: HMONITOR) -> anyhow::Result<MonitorInfo> {
        let mut raw = MONITORINFOEXW {
            monitorInfo: MONITORINFO {
                cbSize: size_of::<MONITORINFOEXW>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let ok = unsafe { GetMonitorInfoW(hmonitor, (&mut raw as *mut MONITORINFOEXW).cast()) };
        if !ok.as_bool() {
            anyhow::bail!("GetMonitorInfoW failed");
        }

        let rect = raw.monitorInfo.rcMonitor;
        let name = wide_device_name(&raw.szDevice).context("monitor device name was invalid")?;
        let primary = raw.monitorInfo.dwFlags & 1 == 1;
        let dpi_scale = unsafe { dpi_scale(hmonitor) };

        Ok(MonitorInfo {
            name,
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0),
            height: (rect.bottom - rect.top).max(0),
            dpi_scale,
            primary,
        })
    }

    unsafe fn dpi_scale(hmonitor: HMONITOR) -> f32 {
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        if unsafe { GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err()
        {
            return 1.0;
        }

        let dpi = dpi_x.max(dpi_y).max(1);
        dpi as f32 / 96.0
    }

    fn wide_device_name(value: &[u16]) -> anyhow::Result<String> {
        let len = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
        Ok(String::from_utf16(&value[..len])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_desktop_bounds_include_negative_monitor_origins() {
        let monitors = vec![
            MonitorInfo {
                name: "left".into(),
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                dpi_scale: 1.0,
                primary: false,
            },
            MonitorInfo {
                name: "primary".into(),
                x: 0,
                y: -100,
                width: 2560,
                height: 1440,
                dpi_scale: 1.25,
                primary: true,
            },
        ];

        let desktop = virtual_desktop_from_monitors(monitors);

        assert_eq!(desktop.x, -1920);
        assert_eq!(desktop.y, -100);
        assert_eq!(desktop.width, 4480);
        assert_eq!(desktop.height, 1440);
    }
}
