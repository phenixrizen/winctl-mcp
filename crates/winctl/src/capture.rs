use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{MonitorInfo, WindowInfo};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotResult {
    pub output_path: String,
    pub region_virtual_desktop: CaptureRegion,
    pub width: u32,
    pub height: u32,
    pub image_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_from: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureErrorCode {
    UnsupportedPlatform,
    NoDisplay,
    IoFailed,
    CaptureFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct CaptureError {
    pub code: CaptureErrorCode,
    pub message: String,
}

pub fn capture_region_from_window(window: &WindowInfo) -> CaptureRegion {
    CaptureRegion {
        x: window.x,
        y: window.y,
        width: window.width,
        height: window.height,
    }
}

pub fn capture_region_from_monitor(monitor: &MonitorInfo) -> CaptureRegion {
    CaptureRegion {
        x: monitor.x,
        y: monitor.y,
        width: monitor.width,
        height: monitor.height,
    }
}

pub fn screenshot_window_to_path(
    window: &WindowInfo,
    output_path: impl Into<String>,
) -> Result<ScreenshotResult, CaptureError> {
    let output_path = output_path.into();
    let region = capture_region_from_window(window);

    #[cfg(windows)]
    {
        windows_impl::capture_window(window, output_path, region)
    }

    #[cfg(not(windows))]
    {
        let _ = (output_path, region);
        Err(unsupported_platform())
    }
}

pub fn screenshot_display_to_path(
    display_index: usize,
    monitor: &MonitorInfo,
    output_path: impl Into<String>,
) -> Result<ScreenshotResult, CaptureError> {
    let output_path = output_path.into();
    let region = capture_region_from_monitor(monitor);

    #[cfg(windows)]
    {
        windows_impl::capture_display(display_index, output_path, region)
    }

    #[cfg(not(windows))]
    {
        let _ = (display_index, output_path, region);
        Err(unsupported_platform())
    }
}

#[cfg(not(windows))]
fn unsupported_platform() -> CaptureError {
    CaptureError {
        code: CaptureErrorCode::UnsupportedPlatform,
        message: "screenshot capture requires Windows runtime".into(),
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::error::Error;
    use std::ffi::c_void;
    use std::fs;
    use std::mem::size_of;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
    use windows_capture::dxgi_duplication_api::{DxgiDuplicationApi, Error as DxgiError};
    use windows_capture::encoder::ImageFormat;
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::monitor::Monitor;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    };
    use windows_capture::window::Window;

    use crate::{CaptureError, CaptureErrorCode, CaptureRegion, ScreenshotResult, WindowInfo};

    const PROVIDER_WGC: &str = "windows_graphics_capture";
    const PROVIDER_DXGI: &str = "dxgi_duplication";
    const PROVIDER_GDI: &str = "gdi_screen_blt";
    const DXGI_FALLBACK_ENV: &str = "WINCTL_CAPTURE_DXGI_FALLBACK";

    #[derive(Clone)]
    struct CaptureFlags {
        output_path: PathBuf,
        result: Arc<Mutex<Option<(u32, u32)>>>,
    }

    struct OneFrameCapture {
        output_path: PathBuf,
        result: Arc<Mutex<Option<(u32, u32)>>>,
    }

    impl GraphicsCaptureApiHandler for OneFrameCapture {
        type Flags = CaptureFlags;
        type Error = Box<dyn Error + Send + Sync>;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            Ok(Self {
                output_path: ctx.flags.output_path,
                result: ctx.flags.result,
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            frame.save_as_image(&self.output_path, ImageFormat::Png)?;
            *self.result.lock().expect("capture result mutex poisoned") =
                Some((frame.width(), frame.height()));
            capture_control.stop();
            Ok(())
        }
    }

    pub(super) fn capture_window(
        window: &WindowInfo,
        output_path: String,
        region: CaptureRegion,
    ) -> Result<ScreenshotResult, CaptureError> {
        let _com = initialize_com_for_capture()?;
        let item = Window::from_raw_hwnd(window.hwnd as *mut c_void);
        match capture_once(
            item,
            output_path.clone(),
            region.clone(),
            PROVIDER_WGC,
            None,
        ) {
            Ok(screenshot) => Ok(screenshot),
            Err(error) if dxgi_fallback_enabled() => {
                tracing::warn!(
                    error = %error.message,
                    hwnd = %window.hwnd_hex,
                    "Windows Graphics Capture window capture failed; trying opt-in GDI fallback"
                );
                capture_window_gdi(window, output_path, region, error.message)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn capture_display(
        display_index: usize,
        output_path: String,
        region: CaptureRegion,
    ) -> Result<ScreenshotResult, CaptureError> {
        let _com = initialize_com_for_capture()?;
        let item = Monitor::from_index(display_index + 1).map_err(|error| CaptureError {
            code: CaptureErrorCode::NoDisplay,
            message: format!("display index {display_index} is unavailable: {error}"),
        })?;
        match capture_once(
            item,
            output_path.clone(),
            region.clone(),
            PROVIDER_WGC,
            None,
        ) {
            Ok(screenshot) => Ok(screenshot),
            Err(error) if dxgi_fallback_enabled() => {
                tracing::warn!(
                    error = %error.message,
                    display_index = display_index,
                    "Windows Graphics Capture display capture failed; trying opt-in DXGI fallback"
                );
                let monitor =
                    Monitor::from_index(display_index + 1).map_err(|error| CaptureError {
                        code: CaptureErrorCode::NoDisplay,
                        message: format!("display index {display_index} is unavailable: {error}"),
                    })?;
                capture_monitor_dxgi(monitor, output_path, region, None, Some(error.message))
            }
            Err(error) => Err(error),
        }
    }

    struct ComApartment {
        should_uninitialize: bool,
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn initialize_com_for_capture() -> Result<ComApartment, CaptureError> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            return Ok(ComApartment {
                should_uninitialize: true,
            });
        }

        if result == RPC_E_CHANGED_MODE {
            tracing::warn!(
                result = ?result,
                "capture thread COM apartment was already initialized with a different model"
            );
            return Ok(ComApartment {
                should_uninitialize: false,
            });
        }

        Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!("failed to initialize COM for capture: {result:?}"),
        })
    }

    fn capture_once<T>(
        item: T,
        output_path: String,
        region: CaptureRegion,
        provider: &str,
        fallback_from: Option<String>,
    ) -> Result<ScreenshotResult, CaptureError>
    where
        T: TryInto<windows_capture::settings::GraphicsCaptureItemType>,
        T: Send + 'static,
    {
        ensure_parent_dir(&output_path)?;

        let result = Arc::new(Mutex::new(None));
        let flags = CaptureFlags {
            output_path: PathBuf::from(&output_path),
            result: result.clone(),
        };
        let settings = Settings::new(
            item,
            CursorCaptureSettings::Default,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            flags,
        );

        let capture_result = catch_unwind(AssertUnwindSafe(|| {
            let control = OneFrameCapture::start_free_threaded(settings)
                .map_err(|error| format!("windows-capture failed to start: {error}"))?;
            control
                .wait()
                .map_err(|error| format!("windows-capture failed: {error}"))
        }));
        match capture_result {
            Ok(Ok(())) => {}
            Ok(Err(message)) => {
                return Err(CaptureError {
                    code: CaptureErrorCode::CaptureFailed,
                    message,
                });
            }
            Err(error) => {
                return Err(CaptureError {
                    code: CaptureErrorCode::CaptureFailed,
                    message: format!("windows-capture panicked: {}", panic_message(error)),
                });
            }
        }

        let (width, height) = result
            .lock()
            .expect("capture result mutex poisoned")
            .ok_or_else(|| CaptureError {
                code: CaptureErrorCode::CaptureFailed,
                message: "windows-capture completed without a frame".into(),
            })?;

        Ok(ScreenshotResult {
            output_path,
            region_virtual_desktop: region,
            width,
            height,
            image_base64: None,
            provider: Some(provider.to_owned()),
            fallback_from,
        })
    }

    fn dxgi_fallback_enabled() -> bool {
        std::env::var(DXGI_FALLBACK_ENV)
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    fn capture_window_gdi(
        _window: &WindowInfo,
        output_path: String,
        region: CaptureRegion,
        fallback_from: String,
    ) -> Result<ScreenshotResult, CaptureError> {
        ensure_parent_dir(&output_path)?;
        let width = region.width.max(1);
        let height = region.height.max(1);
        let width_u32 = width as u32;
        let height_u32 = height as u32;
        let mut pixels = vec![0u8; width_u32 as usize * height_u32 as usize * 4];

        unsafe {
            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return Err(gdi_error("GetDC failed"));
            }
            let memory_dc = CreateCompatibleDC(Some(screen_dc));
            if memory_dc.is_invalid() {
                ReleaseDC(None, screen_dc);
                return Err(gdi_error("CreateCompatibleDC failed"));
            }
            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            if bitmap.is_invalid() {
                let _ = DeleteDC(memory_dc);
                ReleaseDC(None, screen_dc);
                return Err(gdi_error("CreateCompatibleBitmap failed"));
            }
            let old_object = SelectObject(memory_dc, HGDIOBJ::from(bitmap));
            let blit = BitBlt(
                memory_dc,
                0,
                0,
                width,
                height,
                Some(screen_dc),
                region.x,
                region.y,
                SRCCOPY,
            );
            if let Err(error) = blit {
                SelectObject(memory_dc, old_object);
                let _ = DeleteObject(HGDIOBJ::from(bitmap));
                let _ = DeleteDC(memory_dc);
                ReleaseDC(None, screen_dc);
                return Err(gdi_error(&format!("BitBlt failed: {error}")));
            }

            let mut bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let copied = GetDIBits(
                memory_dc,
                bitmap,
                0,
                height_u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            );

            SelectObject(memory_dc, old_object);
            let _ = DeleteObject(HGDIOBJ::from(bitmap));
            let _ = DeleteDC(memory_dc);
            ReleaseDC(None, screen_dc);

            if copied == 0 {
                return Err(gdi_error("GetDIBits failed"));
            }
        }

        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        image::save_buffer(
            &output_path,
            &pixels,
            width_u32,
            height_u32,
            image::ColorType::Rgba8,
        )
        .map_err(|error| CaptureError {
            code: CaptureErrorCode::IoFailed,
            message: format!("failed to save GDI fallback capture: {error}"),
        })?;

        Ok(ScreenshotResult {
            output_path,
            region_virtual_desktop: region,
            width: width_u32,
            height: height_u32,
            image_base64: None,
            provider: Some(PROVIDER_GDI.to_owned()),
            fallback_from: Some(fallback_from),
        })
    }

    fn capture_monitor_dxgi(
        monitor: Monitor,
        output_path: String,
        region: CaptureRegion,
        crop: Option<(u32, u32, u32, u32)>,
        fallback_from: Option<String>,
    ) -> Result<ScreenshotResult, CaptureError> {
        ensure_parent_dir(&output_path)?;
        let mut duplication = DxgiDuplicationApi::new(monitor).map_err(dxgi_error)?;
        let mut last_timeout = None;
        for _ in 0..10 {
            match duplication.acquire_next_frame(100) {
                Ok(mut frame) => {
                    let (width, height) = if let Some((start_x, start_y, end_x, end_y)) = crop {
                        let mut buffer = frame
                            .buffer_crop(start_x, start_y, end_x, end_y)
                            .map_err(dxgi_error)?;
                        let width = buffer.width();
                        let height = buffer.height();
                        buffer
                            .save_as_image(&output_path, ImageFormat::Png)
                            .map_err(dxgi_error)?;
                        (width, height)
                    } else {
                        let width = frame.width();
                        let height = frame.height();
                        frame
                            .save_as_image(&output_path, ImageFormat::Png)
                            .map_err(dxgi_error)?;
                        (width, height)
                    };
                    return Ok(ScreenshotResult {
                        output_path,
                        region_virtual_desktop: region,
                        width,
                        height,
                        image_base64: None,
                        provider: Some(PROVIDER_DXGI.to_owned()),
                        fallback_from,
                    });
                }
                Err(DxgiError::Timeout) => {
                    last_timeout = Some(DxgiError::Timeout);
                }
                Err(error) => return Err(dxgi_error(error)),
            }
        }
        Err(dxgi_error(
            last_timeout.unwrap_or_else(|| DxgiError::Timeout),
        ))
    }

    fn dxgi_error(error: DxgiError) -> CaptureError {
        CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!("DXGI duplication capture failed: {error}"),
        }
    }

    fn gdi_error(message: &str) -> CaptureError {
        CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!("GDI fallback capture failed: {message}"),
        }
    }

    fn ensure_parent_dir(path: &str) -> Result<(), CaptureError> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| CaptureError {
                    code: CaptureErrorCode::IoFailed,
                    message: format!("failed to create capture directory: {error}"),
                })?;
            }
        }

        Ok(())
    }

    fn panic_message(error: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = error.downcast_ref::<&str>() {
            (*message).into()
        } else if let Some(message) = error.downcast_ref::<String>() {
            message.clone()
        } else {
            "unknown panic payload".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowInfo;

    #[test]
    fn capture_region_uses_window_virtual_desktop_coordinates() {
        let window = WindowInfo {
            id: "hwnd:0x1".into(),
            hwnd: 1,
            hwnd_hex: "0x1".into(),
            pid: 1,
            tid: 1,
            process_name: Some("app.exe".into()),
            exe_path: Some("C:/app.exe".into()),
            title: "App".into(),
            class_name: "App".into(),
            x: -120,
            y: 80,
            width: 640,
            height: 480,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        };

        let region = capture_region_from_window(&window);

        assert_eq!(region.x, -120);
        assert_eq!(region.y, 80);
        assert_eq!(region.width, 640);
        assert_eq!(region.height, 480);
    }
}
