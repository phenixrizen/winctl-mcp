use crate::AppState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use winctl::{
    monitors, screenshot_display_to_path, screenshot_window_to_path, CaptureError,
    CaptureErrorCode, ScreenshotResult,
};

#[cfg(windows)]
use std::{
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Instant,
};

pub(crate) const CAPTURE_HELPER_ENV: &str = "WINCTL_CAPTURE_HELPER";
const CAPTURE_HELPER_REQUEST_ENV: &str = "WINCTL_CAPTURE_REQUEST";
const CAPTURE_HELPER_RESPONSE_ENV: &str = "WINCTL_CAPTURE_RESPONSE";
#[cfg(windows)]
const CAPTURE_HELPER_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(windows)]
static CAPTURE_HELPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CaptureHelperRequest {
    Window {
        window: winctl::WindowInfo,
        output_path: String,
    },
    Display {
        display_index: usize,
        monitor: winctl::MonitorInfo,
        output_path: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct CaptureHelperResponse {
    ok: bool,
    #[serde(default)]
    screenshot: Option<ScreenshotResult>,
    #[serde(default)]
    error: Option<CaptureError>,
}

#[derive(Debug)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
struct CaptureHelperProcessOutput {
    success: bool,
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
}

pub fn screenshot_window(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "capture.screenshot_window requested");
    let _capture_guard = match state.capture_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("capture lock was poisoned; continuing with recovered lock");
            poisoned.into_inner()
        }
    };
    tracing::info!(bound_id = %bound_id, "capture.screenshot_window lock acquired");

    let window = match state.revalidate_bound_window(&bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                "capture.screenshot_window revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };

    match capture_window_to_path(&window, screenshot_window_output_path(&window)) {
        Ok(screenshot) => {
            tracing::info!(
                bound_id = %bound_id,
                hwnd = %window.hwnd_hex,
                pid = window.pid,
                output_path = %screenshot.output_path,
                region_x = screenshot.region_virtual_desktop.x,
                region_y = screenshot.region_virtual_desktop.y,
                region_width = screenshot.region_virtual_desktop.width,
                region_height = screenshot.region_virtual_desktop.height,
                image_width = screenshot.width,
                image_height = screenshot.height,
                "capture.screenshot_window succeeded"
            );
            serde_json::json!({"ok": true, "screenshot": screenshot})
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                hwnd = %window.hwnd_hex,
                pid = window.pid,
                error_code = ?error.code,
                "capture.screenshot_window failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub(crate) fn screenshot_window_output_path(window: &winctl::WindowInfo) -> String {
    format!(
        "./captures/window-{}.png",
        sanitize_path_component(&window.hwnd_hex)
    )
}

pub fn screenshot_display(state: &AppState, display_index: usize) -> serde_json::Value {
    tracing::info!(
        display_index = display_index,
        "capture.screenshot_display requested"
    );
    let _capture_guard = match state.capture_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("capture lock was poisoned; continuing with recovered lock");
            poisoned.into_inner()
        }
    };
    tracing::info!(
        display_index = display_index,
        "capture.screenshot_display lock acquired"
    );

    let desktop = monitors();
    let Some(monitor) = desktop.monitors.get(display_index) else {
        tracing::warn!(
            display_index = display_index,
            monitors_count = desktop.monitors.len(),
            "capture.screenshot_display display not found"
        );
        return serde_json::json!({"ok": false, "error": {"code": "no_display", "message": format!("display index {display_index} is unavailable")}});
    };

    match capture_display_to_path(
        display_index,
        monitor,
        format!("./captures/display-{display_index}.png"),
    ) {
        Ok(screenshot) => {
            tracing::info!(
                display_index = display_index,
                output_path = %screenshot.output_path,
                region_x = screenshot.region_virtual_desktop.x,
                region_y = screenshot.region_virtual_desktop.y,
                region_width = screenshot.region_virtual_desktop.width,
                region_height = screenshot.region_virtual_desktop.height,
                image_width = screenshot.width,
                image_height = screenshot.height,
                "capture.screenshot_display succeeded"
            );
            serde_json::json!({"ok": true, "screenshot": screenshot})
        }
        Err(error) => {
            tracing::warn!(
                display_index = display_index,
                error_code = ?error.code,
                "capture.screenshot_display failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub(crate) fn is_capture_helper_mode() -> bool {
    std::env::var_os(CAPTURE_HELPER_ENV).is_some()
}

pub(crate) fn run_capture_helper() -> anyhow::Result<()> {
    let input = if let Some(request_path) = std::env::var_os(CAPTURE_HELPER_REQUEST_ENV) {
        fs::read_to_string(PathBuf::from(request_path))?
    } else {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        input
    };

    let response = match serde_json::from_str::<CaptureHelperRequest>(&input) {
        Ok(request) => CaptureHelperResponse::from_result(run_capture_helper_request(request)),
        Err(error) => CaptureHelperResponse::from_result(Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!("invalid capture helper request: {error}"),
        })),
    };
    let payload = serde_json::to_vec(&response)?;

    if let Some(response_path) = std::env::var_os(CAPTURE_HELPER_RESPONSE_ENV) {
        write_capture_helper_response_file(&PathBuf::from(response_path), &payload)?;
    } else {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(&payload)?;
        writeln!(&mut stdout)?;
    }
    Ok(())
}

fn run_capture_helper_request(
    request: CaptureHelperRequest,
) -> Result<ScreenshotResult, CaptureError> {
    match request {
        CaptureHelperRequest::Window {
            window,
            output_path,
        } => screenshot_window_to_path(&window, output_path),
        CaptureHelperRequest::Display {
            display_index,
            monitor,
            output_path,
        } => screenshot_display_to_path(display_index, &monitor, output_path),
    }
}

fn capture_window_to_path(
    window: &winctl::WindowInfo,
    output_path: String,
) -> Result<ScreenshotResult, CaptureError> {
    #[cfg(windows)]
    {
        run_capture_helper_process(
            "window",
            CaptureHelperRequest::Window {
                window: window.clone(),
                output_path,
            },
            CAPTURE_HELPER_TIMEOUT,
        )
    }

    #[cfg(not(windows))]
    {
        screenshot_window_to_path(window, output_path)
    }
}

fn capture_display_to_path(
    display_index: usize,
    monitor: &winctl::MonitorInfo,
    output_path: String,
) -> Result<ScreenshotResult, CaptureError> {
    #[cfg(windows)]
    {
        run_capture_helper_process(
            "display",
            CaptureHelperRequest::Display {
                display_index,
                monitor: monitor.clone(),
                output_path,
            },
            CAPTURE_HELPER_TIMEOUT,
        )
    }

    #[cfg(not(windows))]
    {
        screenshot_display_to_path(display_index, monitor, output_path)
    }
}

#[cfg(windows)]
fn run_capture_helper_process(
    capture_kind: &str,
    request: CaptureHelperRequest,
    timeout: Duration,
) -> Result<ScreenshotResult, CaptureError> {
    let request = serde_json::to_vec(&request).map_err(|error| CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!("failed to encode capture helper request: {error}"),
    })?;
    let (request_path, response_path) = capture_helper_ipc_paths(capture_kind);
    write_atomic_file(&request_path, &request).map_err(|error| CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!(
            "failed to write capture helper request {}: {error}",
            request_path.display()
        ),
    })?;
    let current_exe = std::env::current_exe().map_err(|error| CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!("failed to resolve capture helper executable: {error}"),
    })?;
    tracing::info!(
        capture_kind = capture_kind,
        helper_exe = %current_exe.display(),
        "capture helper spawning"
    );
    let child = Command::new(current_exe)
        .env(CAPTURE_HELPER_ENV, "1")
        .env(CAPTURE_HELPER_REQUEST_ENV, &request_path)
        .env(CAPTURE_HELPER_RESPONSE_ENV, &response_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!("failed to start capture helper: {error}"),
        })?;
    tracing::info!(
        capture_kind = capture_kind,
        helper_pid = child.id(),
        "capture helper spawned"
    );
    let helper_pid = child.id();
    drop(child);

    tracing::info!(
        capture_kind = capture_kind,
        helper_pid = helper_pid,
        timeout_secs = timeout.as_secs(),
        request_path = %request_path.display(),
        response_path = %response_path.display(),
        "capture helper waiting for response file"
    );
    wait_for_capture_helper_response(
        helper_pid,
        capture_kind,
        &request_path,
        &response_path,
        timeout,
    )
}

#[cfg(windows)]
fn wait_for_capture_helper_response(
    helper_pid: u32,
    capture_kind: &str,
    request_path: &Path,
    response_path: &Path,
    timeout: Duration,
) -> Result<ScreenshotResult, CaptureError> {
    let started = Instant::now();

    loop {
        match fs::read_to_string(response_path) {
            Ok(stdout) => {
                tracing::info!(
                    capture_kind = capture_kind,
                    helper_pid = helper_pid,
                    response_path = %response_path.display(),
                    "capture helper response file received"
                );
                let _ = fs::remove_file(request_path);
                let _ = fs::remove_file(response_path);
                tracing::info!(
                    capture_kind = capture_kind,
                    helper_pid = helper_pid,
                    response_bytes = stdout.len(),
                    "capture helper response cleanup complete"
                );
                return decode_capture_helper_output(CaptureHelperProcessOutput {
                    success: true,
                    status_code: Some(0),
                    stdout,
                    stderr: String::new(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                terminate_capture_helper_pid(helper_pid);
                let _ = fs::remove_file(request_path);
                return Err(CaptureError {
                    code: CaptureErrorCode::CaptureFailed,
                    message: format!(
                        "failed to read capture helper response {}: {error}",
                        response_path.display()
                    ),
                });
            }
        }

        if started.elapsed() >= timeout {
            let error = capture_helper_timeout_error(capture_kind, timeout);
            tracing::warn!(
                capture_kind = capture_kind,
                helper_pid = helper_pid,
                timeout_secs = timeout.as_secs(),
                "capture helper timed out"
            );
            terminate_capture_helper_pid(helper_pid);
            let _ = fs::remove_file(request_path);
            let _ = fs::remove_file(response_path);
            return Err(error);
        }

        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn capture_helper_ipc_paths(capture_kind: &str) -> (PathBuf, PathBuf) {
    let sequence = CAPTURE_HELPER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    let name = format!(
        "winctl-capture-{capture_kind}-{}-{sequence}",
        std::process::id()
    );
    let mut request_path = path.clone();
    request_path.push(format!("{name}-request.json"));
    path.push(format!("{name}-response.json"));
    (request_path, path)
}

#[cfg(windows)]
fn terminate_capture_helper_pid(helper_pid: u32) {
    thread::spawn(move || {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &helper_pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    });
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn decode_capture_helper_output(
    output: CaptureHelperProcessOutput,
) -> Result<ScreenshotResult, CaptureError> {
    tracing::info!(
        success = output.success,
        status_code = output.status_code,
        stdout_bytes = output.stdout.len(),
        stderr_bytes = output.stderr.len(),
        "decoding capture helper output"
    );
    if !output.success {
        let status = output
            .status_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unknown".into());
        let stderr = summarize_output(&output.stderr);
        let details = if stderr.is_empty() {
            format!("capture helper exited with status {status}")
        } else {
            format!("capture helper exited with status {status}; stderr: {stderr}")
        };
        return Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: details,
        });
    }

    let stdout = output.stdout.trim();
    if stdout.is_empty() {
        return Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: "capture helper returned empty stdout".into(),
        });
    }

    let response: CaptureHelperResponse =
        serde_json::from_str(stdout).map_err(|error| CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!(
                "invalid capture helper response: {error}; stdout: {}",
                summarize_output(stdout)
            ),
        })?;
    tracing::info!(
        ok = response.ok,
        has_screenshot = response.screenshot.is_some(),
        has_error = response.error.is_some(),
        "decoded capture helper response"
    );

    match (response.ok, response.screenshot, response.error) {
        (true, Some(screenshot), _) => Ok(screenshot),
        (true, None, _) => Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: "capture helper reported success without screenshot metadata".into(),
        }),
        (false, _, Some(error)) => Err(error),
        (false, _, None) => Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: "capture helper reported failure without error metadata".into(),
        }),
    }
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn capture_helper_timeout_error(capture_kind: &str, timeout: Duration) -> CaptureError {
    CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!(
            "{capture_kind} capture helper timed out after {}s",
            timeout.as_secs()
        ),
    }
}

fn write_capture_helper_response_file(path: &Path, payload: &[u8]) -> anyhow::Result<()> {
    write_atomic_file(path, payload)
}

fn write_atomic_file(path: &Path, payload: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, payload)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn summarize_output(value: &str) -> String {
    const LIMIT: usize = 500;
    let value = value.trim();
    if value.chars().count() <= LIMIT {
        return value.into();
    }

    let mut summary: String = value.chars().take(LIMIT).collect();
    summary.push_str("...");
    summary
}

impl CaptureHelperResponse {
    fn from_result(result: Result<ScreenshotResult, CaptureError>) -> Self {
        match result {
            Ok(screenshot) => Self {
                ok: true,
                screenshot: Some(screenshot),
                error: None,
            },
            Err(error) => Self {
                ok: false,
                screenshot: None,
                error: Some(error),
            },
        }
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screenshot_json() -> serde_json::Value {
        serde_json::json!({
            "output_path": "./captures/window-0x1.png",
            "region_virtual_desktop": {
                "x": 10,
                "y": 20,
                "width": 300,
                "height": 200
            },
            "width": 300,
            "height": 200,
            "image_base64": null
        })
    }

    #[test]
    fn screenshot_window_output_path_uses_safe_hwnd_component() {
        let window = winctl::WindowInfo {
            id: "hwnd:0x0000000000981332".into(),
            hwnd: 0x981332,
            hwnd_hex: "0x0000000000981332".into(),
            pid: 1,
            tid: 1,
            process_name: Some("Betty.exe".into()),
            exe_path: Some("C:/Betty.exe".into()),
            title: "Betty".into(),
            class_name: "#32770".into(),
            x: 920,
            y: 230,
            width: 1616,
            height: 1039,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        };

        let path = screenshot_window_output_path(&window);

        assert_eq!(path, "./captures/window-0x0000000000981332.png");
        assert!(!path["./captures/".len()..].contains(':'));
    }

    #[test]
    fn helper_success_stdout_decodes_screenshot() {
        let output = CaptureHelperProcessOutput {
            success: true,
            status_code: Some(0),
            stdout: serde_json::json!({
                "ok": true,
                "screenshot": screenshot_json()
            })
            .to_string(),
            stderr: String::new(),
        };

        let screenshot = decode_capture_helper_output(output).unwrap();

        assert_eq!(screenshot.output_path, "./captures/window-0x1.png");
        assert_eq!(screenshot.region_virtual_desktop.x, 10);
        assert_eq!(screenshot.width, 300);
        assert_eq!(screenshot.height, 200);
    }

    #[test]
    fn helper_error_stdout_decodes_capture_error() {
        let output = CaptureHelperProcessOutput {
            success: true,
            status_code: Some(0),
            stdout: serde_json::json!({
                "ok": false,
                "error": {
                    "code": "capture_failed",
                    "message": "windows-capture panicked"
                }
            })
            .to_string(),
            stderr: String::new(),
        };

        let error = decode_capture_helper_output(output).unwrap_err();

        assert_eq!(error.code, winctl::CaptureErrorCode::CaptureFailed);
        assert_eq!(error.message, "windows-capture panicked");
    }

    #[test]
    fn helper_timeout_is_reported_as_capture_failed() {
        let error = capture_helper_timeout_error("window", std::time::Duration::from_secs(12));

        assert_eq!(error.code, winctl::CaptureErrorCode::CaptureFailed);
        assert!(error.message.contains("window capture helper timed out"));
        assert!(error.message.contains("12s"));
    }
}
