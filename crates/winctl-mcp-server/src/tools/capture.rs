use crate::{AppState, VideoStartRequest, VideoStopRequest, WindowImageChangeWaitRequest};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use winctl::{
    list_windows, monitors, screenshot_display_to_path, screenshot_window_to_path, CaptureError,
    CaptureErrorCode, MonitorInfo, ScreenshotResult, WindowIdentity, WindowInfo,
};

#[cfg(windows)]
use std::process::{Child, Command, Stdio};

pub(crate) const CAPTURE_HELPER_ENV: &str = "WINCTL_CAPTURE_HELPER";
pub(crate) const CAPTURE_DIR_ENV: &str = "WINCTL_CAPTURE_DIR";
const CAPTURE_HELPER_REQUEST_ENV: &str = "WINCTL_CAPTURE_REQUEST";
const CAPTURE_HELPER_RESPONSE_ENV: &str = "WINCTL_CAPTURE_RESPONSE";
#[cfg(windows)]
const CAPTURE_HELPER_TIMEOUT: Duration = Duration::from_secs(15);

const MAX_COMPLETED_VIDEO_RECORDINGS: usize = 50;
const DEFAULT_VIDEO_FRAME_INTERVAL_MS: u64 = 250;
const DEFAULT_VIDEO_MAX_DURATION_MS: u64 = 5 * 60 * 1000;
const DEFAULT_VIDEO_MAX_FRAME_WIDTH: u32 = 1280;
const DEFAULT_VIDEO_MAX_FRAME_HEIGHT: u32 = 720;
const MIN_VIDEO_FRAME_DIMENSION: u32 = 64;
const MAX_VIDEO_FRAME_DIMENSION: u32 = 7680;

#[derive(Default)]
pub struct VideoRuntimeState {
    active: Option<ActiveVideoRecording>,
    completed: VecDeque<VideoRecordingSummary>,
    next_id: u64,
}

struct ActiveVideoRecording {
    recording_id: String,
    started_unix_ms: u64,
    target: VideoTarget,
    frame_interval_ms: u64,
    max_duration_ms: u64,
    max_frame_width: u32,
    max_frame_height: u32,
    output_path: PathBuf,
    frames_dir: PathBuf,
    stop: Arc<AtomicBool>,
    handle: JoinHandle<VideoRecordingSummary>,
}

#[derive(Clone)]
enum VideoTarget {
    Window {
        bound_id: String,
        identity: WindowIdentity,
        window: WindowInfo,
    },
    Display {
        display_index: usize,
        monitor: MonitorInfo,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRecordingSummary {
    pub recording_id: String,
    pub started_unix_ms: u64,
    pub finished_unix_ms: u64,
    pub status: String,
    pub target: serde_json::Value,
    pub output_path: Option<String>,
    pub frames_dir: String,
    pub frame_count: usize,
    pub frame_interval_ms: u64,
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub encoded_width: Option<u32>,
    pub encoded_height: Option<u32>,
    pub elapsed_ms: u64,
    pub format: String,
    #[serde(default)]
    pub capture_providers: Vec<String>,
    #[serde(default)]
    pub capture_fallbacks: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

struct EncodedGifInfo {
    width: u32,
    height: u32,
    resized: bool,
}

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
    helper_pid: Option<u32>,
    elapsed_ms: Option<u64>,
}

#[cfg(windows)]
struct CaptureHelperJob {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for CaptureHelperJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
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

    match capture_window_to_path(
        &window,
        screenshot_window_output_path(state.capture_dir.as_ref().as_path(), &window),
    ) {
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

pub(crate) fn screenshot_window_output_path(
    capture_dir: &Path,
    window: &winctl::WindowInfo,
) -> String {
    capture_output_path(
        capture_dir,
        format!("window-{}.png", sanitize_path_component(&window.hwnd_hex)),
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
        screenshot_display_output_path(state.capture_dir.as_ref().as_path(), display_index),
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

pub fn wait_for_window_image_change(
    state: &AppState,
    request: WindowImageChangeWaitRequest,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        timeout_ms = request.timeout_ms,
        poll_interval_ms = request.poll_interval_ms,
        "capture.wait_for_window_image_change requested"
    );
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5_000));
    let poll_interval = Duration::from_millis(request.poll_interval_ms.unwrap_or(250).max(50));
    let started = Instant::now();

    let initial = match capture_and_hash_window(state, &request.bound_id) {
        Ok(value) => value,
        Err(error) => return error,
    };

    loop {
        if started.elapsed() >= timeout {
            tracing::warn!(
                bound_id = %request.bound_id,
                timeout_ms = timeout.as_millis() as u64,
                "capture.wait_for_window_image_change timed out"
            );
            return serde_json::json!({
                "ok": false,
                "bound_id": request.bound_id,
                "changed": false,
                "timeout": true,
                "initial_screenshot": initial.screenshot,
                "initial_hash": initial.hash,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64},
                "error": {
                    "code": "image_change_timeout",
                    "message": "bound window screenshot did not change before timeout"
                }
            });
        }

        thread::sleep(poll_interval);
        let current = match capture_and_hash_window(state, &request.bound_id) {
            Ok(value) => value,
            Err(error) => return error,
        };
        if current.hash != initial.hash {
            tracing::info!(
                bound_id = %request.bound_id,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "capture.wait_for_window_image_change matched"
            );
            return serde_json::json!({
                "ok": true,
                "bound_id": request.bound_id,
                "changed": true,
                "timeout": false,
                "initial_screenshot": initial.screenshot,
                "changed_screenshot": current.screenshot,
                "initial_hash": initial.hash,
                "changed_hash": current.hash,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64}
            });
        }
    }
}

pub fn video_start(state: &AppState, request: VideoStartRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = ?request.bound_id,
        display_index = ?request.display_index,
        frame_interval_ms = ?request.frame_interval_ms,
        max_duration_ms = ?request.max_duration_ms,
        "capture.video_start requested"
    );
    let target = match video_target(state, &request) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let frame_interval_ms = request
        .frame_interval_ms
        .unwrap_or(DEFAULT_VIDEO_FRAME_INTERVAL_MS)
        .clamp(100, 5_000);
    let max_duration_ms = request
        .max_duration_ms
        .unwrap_or(DEFAULT_VIDEO_MAX_DURATION_MS)
        .clamp(500, 30 * 60 * 1000);
    let max_frame_width = request
        .max_frame_width
        .unwrap_or(DEFAULT_VIDEO_MAX_FRAME_WIDTH)
        .clamp(MIN_VIDEO_FRAME_DIMENSION, MAX_VIDEO_FRAME_DIMENSION);
    let max_frame_height = request
        .max_frame_height
        .unwrap_or(DEFAULT_VIDEO_MAX_FRAME_HEIGHT)
        .clamp(MIN_VIDEO_FRAME_DIMENSION, MAX_VIDEO_FRAME_DIMENSION);
    let mut runtime = state.video_runtime.lock().expect("video mutex poisoned");
    if let Some(active) = &runtime.active {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "video_recording_already_active",
                "message": "a video recording is already active; stop it before starting another"
            },
            "active": active_summary(active),
        });
    }
    runtime.next_id = runtime.next_id.saturating_add(1).max(1);
    let recording_id = format!("video-{}", runtime.next_id);
    let safe_name = request
        .output_name
        .as_deref()
        .map(sanitize_path_component)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| recording_id.clone());
    let videos_dir = state.capture_dir.join("videos");
    let frames_dir = videos_dir.join(format!("{safe_name}-frames"));
    if let Err(error) = fs::create_dir_all(&frames_dir) {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "video_frames_dir_failed",
                "message": format!("failed to create video frame directory {}: {error}", frames_dir.display())
            }
        });
    }
    let output_path = videos_dir.join(format!("{safe_name}.gif"));
    let stop = Arc::new(AtomicBool::new(false));
    let started_unix_ms = now_unix_ms();
    let target_for_thread = target.clone();
    let capture_lock = state.capture_lock.clone();
    let recording_id_for_thread = recording_id.clone();
    let frames_dir_for_thread = frames_dir.clone();
    let output_path_for_thread = output_path.clone();
    let stop_for_thread = stop.clone();
    let handle = thread::spawn(move || {
        run_video_recording(
            recording_id_for_thread,
            started_unix_ms,
            target_for_thread,
            frame_interval_ms,
            max_duration_ms,
            max_frame_width,
            max_frame_height,
            frames_dir_for_thread,
            output_path_for_thread,
            stop_for_thread,
            capture_lock,
        )
    });
    let active = ActiveVideoRecording {
        recording_id: recording_id.clone(),
        started_unix_ms,
        target,
        frame_interval_ms,
        max_duration_ms,
        max_frame_width,
        max_frame_height,
        output_path: output_path.clone(),
        frames_dir: frames_dir.clone(),
        stop,
        handle,
    };
    let summary = active_summary(&active);
    runtime.active = Some(active);
    serde_json::json!({
        "ok": true,
        "recording": summary,
    })
}

pub fn video_stop(state: &AppState, request: VideoStopRequest) -> serde_json::Value {
    tracing::info!(recording_id = ?request.recording_id, "capture.video_stop requested");
    let active = {
        let mut runtime = state.video_runtime.lock().expect("video mutex poisoned");
        let Some(active) = runtime.active.take() else {
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "video_recording_not_active",
                    "message": "no video recording is active"
                }
            });
        };
        if request
            .recording_id
            .as_ref()
            .map(|recording_id| recording_id != &active.recording_id)
            .unwrap_or(false)
        {
            let current = active_summary(&active);
            runtime.active = Some(active);
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "video_recording_id_mismatch",
                    "message": "the requested recording_id is not the active recording"
                },
                "active": current,
            });
        }
        active
    };
    active.stop.store(true, Ordering::SeqCst);
    let summary = match active.handle.join() {
        Ok(summary) => summary,
        Err(error) => VideoRecordingSummary {
            recording_id: active.recording_id,
            started_unix_ms: active.started_unix_ms,
            finished_unix_ms: now_unix_ms(),
            status: "failed".into(),
            target: target_json(&active.target),
            output_path: None,
            frames_dir: active.frames_dir.to_string_lossy().into_owned(),
            frame_count: 0,
            frame_interval_ms: active.frame_interval_ms,
            max_frame_width: active.max_frame_width,
            max_frame_height: active.max_frame_height,
            encoded_width: None,
            encoded_height: None,
            elapsed_ms: 0,
            format: "gif".into(),
            capture_providers: Vec::new(),
            capture_fallbacks: Vec::new(),
            warnings: vec![format!(
                "video recording thread panicked: {}",
                panic_message(error)
            )],
        },
    };
    let mut runtime = state.video_runtime.lock().expect("video mutex poisoned");
    runtime.completed.push_back(summary.clone());
    while runtime.completed.len() > MAX_COMPLETED_VIDEO_RECORDINGS {
        runtime.completed.pop_front();
    }
    serde_json::json!({
        "ok": summary.status == "completed",
        "recording": summary,
    })
}

fn video_target(
    state: &AppState,
    request: &VideoStartRequest,
) -> Result<VideoTarget, serde_json::Value> {
    match (&request.bound_id, request.display_index) {
        (Some(bound_id), None) => {
            let window = state.revalidate_bound_window(bound_id).map_err(|error| {
                serde_json::json!({
                    "ok": false,
                    "error": error,
                })
            })?;
            Ok(VideoTarget::Window {
                bound_id: bound_id.clone(),
                identity: WindowIdentity::from_window(&window),
                window,
            })
        }
        (None, display_index) => {
            let display_index = display_index.unwrap_or(0);
            let desktop = monitors();
            let Some(monitor) = desktop.monitors.get(display_index).cloned() else {
                return Err(serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "no_display",
                        "message": format!("display index {display_index} is unavailable")
                    }
                }));
            };
            Ok(VideoTarget::Display {
                display_index,
                monitor,
            })
        }
        (Some(_), Some(_)) => Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "video_target_ambiguous",
                "message": "specify either bound_id or display_index, not both"
            }
        })),
    }
}

fn active_summary(active: &ActiveVideoRecording) -> serde_json::Value {
    serde_json::json!({
        "recording_id": active.recording_id,
        "started_unix_ms": active.started_unix_ms,
        "target": target_json(&active.target),
        "frame_interval_ms": active.frame_interval_ms,
        "max_duration_ms": active.max_duration_ms,
        "max_frame_width": active.max_frame_width,
        "max_frame_height": active.max_frame_height,
        "output_path": active.output_path,
        "frames_dir": active.frames_dir,
        "format": "gif",
    })
}

fn run_video_recording(
    recording_id: String,
    started_unix_ms: u64,
    target: VideoTarget,
    frame_interval_ms: u64,
    max_duration_ms: u64,
    max_frame_width: u32,
    max_frame_height: u32,
    frames_dir: PathBuf,
    output_path: PathBuf,
    stop: Arc<AtomicBool>,
    capture_lock: Arc<Mutex<()>>,
) -> VideoRecordingSummary {
    let started = Instant::now();
    let mut frame_paths = Vec::new();
    let mut capture_providers = Vec::new();
    let mut capture_fallbacks = Vec::new();
    let mut warnings = Vec::new();
    let mut index = 0usize;
    while !stop.load(Ordering::SeqCst) && started.elapsed().as_millis() < max_duration_ms as u128 {
        let frame_path = frames_dir.join(format!("frame-{index:06}.png"));
        let capture_result = {
            let _capture_guard = match capture_lock.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            capture_video_frame(&target, frame_path.clone())
        };
        match capture_result {
            Ok(screenshot) => {
                if let Some(provider) = &screenshot.provider {
                    if !capture_providers.iter().any(|item| item == provider) {
                        capture_providers.push(provider.clone());
                    }
                    if let Some(fallback_from) = &screenshot.fallback_from {
                        capture_fallbacks.push(serde_json::json!({
                            "frame_index": index,
                            "from": fallback_from,
                            "to": provider,
                        }));
                    }
                }
                frame_paths.push(PathBuf::from(screenshot.output_path));
                index += 1;
            }
            Err(error) => {
                warnings.push(format!("frame capture failed: {}", error.message));
                break;
            }
        }
        thread::sleep(Duration::from_millis(frame_interval_ms));
    }
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let encoded = if frame_paths.is_empty() {
        warnings.push("recording stopped before any frames were captured".into());
        None
    } else {
        match encode_gif(
            &frame_paths,
            &output_path,
            frame_interval_ms,
            max_frame_width,
            max_frame_height,
        ) {
            Ok(info) => {
                if info.resized {
                    warnings.push(format!(
                        "encoded GIF frames were scaled to fit within {max_frame_width}x{max_frame_height}"
                    ));
                }
                Some(info)
            }
            Err(error) => {
                warnings.push(error);
                None
            }
        }
    };
    let output = encoded
        .as_ref()
        .map(|_| output_path.to_string_lossy().into_owned());
    let status = if output.is_some() {
        "completed"
    } else {
        "failed"
    };
    VideoRecordingSummary {
        recording_id,
        started_unix_ms,
        finished_unix_ms: now_unix_ms(),
        status: status.into(),
        target: target_json(&target),
        output_path: output,
        frames_dir: frames_dir.to_string_lossy().into_owned(),
        frame_count: frame_paths.len(),
        frame_interval_ms,
        max_frame_width,
        max_frame_height,
        encoded_width: encoded.as_ref().map(|info| info.width),
        encoded_height: encoded.as_ref().map(|info| info.height),
        elapsed_ms,
        format: "gif".into(),
        capture_providers,
        capture_fallbacks,
        warnings,
    }
}

fn capture_video_frame(
    target: &VideoTarget,
    output_path: PathBuf,
) -> Result<ScreenshotResult, CaptureError> {
    match target {
        VideoTarget::Window {
            identity, window, ..
        } => {
            let current = list_windows()
                .into_iter()
                .find(|candidate| identity.matches_window(candidate))
                .ok_or_else(|| CaptureError {
                    code: CaptureErrorCode::CaptureFailed,
                    message: format!(
                        "bound video target {} pid {} is no longer available",
                        window.hwnd_hex, window.pid
                    ),
                })?;
            capture_window_to_path(&current, output_path.to_string_lossy().into_owned())
        }
        VideoTarget::Display {
            display_index,
            monitor,
        } => capture_display_to_path(
            *display_index,
            monitor,
            output_path.to_string_lossy().into_owned(),
        ),
    }
}

fn encode_gif(
    frame_paths: &[PathBuf],
    output_path: &Path,
    frame_interval_ms: u64,
    max_frame_width: u32,
    max_frame_height: u32,
) -> Result<EncodedGifInfo, String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create video output directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let file = fs::File::create(output_path).map_err(|error| {
        format!(
            "failed to create video artifact {}: {error}",
            output_path.display()
        )
    })?;
    let mut writer = Some(std::io::BufWriter::new(file));
    let mut encoder = None;
    let delay_cs = ((frame_interval_ms + 9) / 10).clamp(1, u16::MAX as u64) as u16;
    let mut encoded = None;
    let mut resized_any = false;
    for frame_path in frame_paths {
        let frame = image::open(frame_path)
            .map_err(|error| {
                format!(
                    "failed to read video frame {}: {error}",
                    frame_path.display()
                )
            })?
            .into_rgba8();
        let (original_width, original_height) = frame.dimensions();
        let (frame, resized) = resize_frame_for_gif(frame, max_frame_width, max_frame_height);
        let (encoded_width, encoded_height) = frame.dimensions();
        let encoder = match &mut encoder {
            Some(encoder) => encoder,
            None => {
                let writer = writer
                    .take()
                    .ok_or_else(|| "video GIF writer was already consumed".to_string())?;
                let mut new_encoder =
                    gif::Encoder::new(writer, encoded_width as u16, encoded_height as u16, &[])
                        .map_err(|error| format!("failed to create gif encoder: {error}"))?;
                new_encoder
                    .set_repeat(gif::Repeat::Infinite)
                    .map_err(|error| format!("failed to configure gif encoder: {error}"))?;
                encoder.insert(new_encoder)
            }
        };
        resized_any |= resized;
        if encoded.is_none() {
            encoded = Some(EncodedGifInfo {
                width: encoded_width,
                height: encoded_height,
                resized: false,
            });
        }
        tracing::debug!(
            frame_path = %frame_path.display(),
            original_width,
            original_height,
            encoded_width,
            encoded_height,
            resized,
            "encoding video frame"
        );
        let mut pixels = frame.into_raw();
        let mut gif_frame = gif::Frame::from_rgba_speed(
            encoded_width as u16,
            encoded_height as u16,
            &mut pixels,
            30,
        );
        gif_frame.delay = delay_cs;
        encoder
            .write_frame(&gif_frame)
            .map_err(|error| format!("failed to encode video gif: {error}"))?;
    }
    if let Some(encoder) = encoder {
        let mut writer = encoder
            .into_inner()
            .map_err(|error| format!("failed to finish video gif: {error}"))?;
        writer.flush().map_err(|error| {
            format!(
                "failed to flush video artifact {}: {error}",
                output_path.display()
            )
        })?;
    }
    let mut info = encoded.ok_or_else(|| "no video frames were available to encode".to_string())?;
    info.resized = resized_any;
    Ok(info)
}

fn resize_frame_for_gif(
    frame: image::RgbaImage,
    max_frame_width: u32,
    max_frame_height: u32,
) -> (image::RgbaImage, bool) {
    let (width, height) = frame.dimensions();
    if width <= max_frame_width && height <= max_frame_height {
        return (frame, false);
    }
    let width_scale = max_frame_width as f64 / width.max(1) as f64;
    let height_scale = max_frame_height as f64 / height.max(1) as f64;
    let scale = width_scale.min(height_scale).min(1.0);
    let resized_width = ((width as f64 * scale).round() as u32).max(1);
    let resized_height = ((height as f64 * scale).round() as u32).max(1);
    (
        image::imageops::resize(
            &frame,
            resized_width,
            resized_height,
            image::imageops::FilterType::Triangle,
        ),
        true,
    )
}

fn target_json(target: &VideoTarget) -> serde_json::Value {
    match target {
        VideoTarget::Window {
            bound_id,
            identity,
            window,
        } => serde_json::json!({
            "kind": "window",
            "bound_id": bound_id,
            "identity": identity,
            "window": {
                "hwnd": window.hwnd,
                "hwnd_hex": window.hwnd_hex,
                "pid": window.pid,
                "process_name": window.process_name,
                "exe_path": window.exe_path,
                "title": window.title,
                "class_name": window.class_name,
            }
        }),
        VideoTarget::Display {
            display_index,
            monitor,
        } => serde_json::json!({
            "kind": "display",
            "display_index": display_index,
            "monitor": monitor,
        }),
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn panic_message(error: Box<dyn std::any::Any + Send>) -> String {
    if let Some(value) = error.downcast_ref::<&str>() {
        (*value).into()
    } else if let Some(value) = error.downcast_ref::<String>() {
        value.clone()
    } else {
        "unknown panic".into()
    }
}

pub(crate) fn default_capture_dir() -> PathBuf {
    if let Some(value) = std::env::var_os(CAPTURE_DIR_ENV) {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }

    #[cfg(windows)]
    {
        if let Some(value) = std::env::var_os("LOCALAPPDATA") {
            if !value.is_empty() {
                return PathBuf::from(value).join("winctl-mcp").join("captures");
            }
        }
    }

    std::env::temp_dir().join("winctl-mcp").join("captures")
}

pub(crate) fn ensure_capture_dir(capture_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(capture_dir).map_err(|error| {
        anyhow::anyhow!(
            "failed to create capture directory {}: {error}",
            capture_dir.display()
        )
    })
}

fn screenshot_display_output_path(capture_dir: &Path, display_index: usize) -> String {
    capture_output_path(capture_dir, format!("display-{display_index}.png"))
}

fn capture_output_path(capture_dir: &Path, file_name: String) -> String {
    capture_dir.join(file_name).to_string_lossy().into_owned()
}

struct HashedScreenshot {
    screenshot: ScreenshotResult,
    hash: u64,
}

fn capture_and_hash_window(
    state: &AppState,
    bound_id: &str,
) -> Result<HashedScreenshot, serde_json::Value> {
    let response = screenshot_window(state, bound_id.to_owned());
    if !response
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err(response);
    }
    let screenshot: ScreenshotResult = serde_json::from_value(
        response.get("screenshot").cloned().unwrap_or_default(),
    )
    .map_err(|error| {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "invalid_screenshot_metadata",
                "message": format!("screenshot metadata could not be decoded: {error}")
            },
            "capture_response": response
        })
    })?;
    let hash = hash_file(&screenshot.output_path).map_err(|error| {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "image_hash_failed",
                "message": format!("failed to hash screenshot {}: {error}", screenshot.output_path)
            },
            "screenshot": screenshot
        })
    })?;

    Ok(HashedScreenshot { screenshot, hash })
}

fn hash_file(path: &str) -> std::io::Result<u64> {
    let bytes = fs::read(path)?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(hasher.finish())
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
    let current_exe = std::env::current_exe().map_err(|error| CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!("failed to resolve capture helper executable: {error}"),
    })?;
    tracing::info!(
        capture_kind = capture_kind,
        helper_exe = %current_exe.display(),
        "capture helper spawning"
    );

    let mut command = Command::new(current_exe);
    command
        .env(CAPTURE_HELPER_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_capture_helper_command(&mut command);

    let mut child = command.spawn().map_err(|error| CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!("failed to start capture helper: {error}"),
    })?;
    let helper_pid = child.id();
    let job = match CaptureHelperJob::create_for_child(&child) {
        Ok(job) => Some(job),
        Err(error) => {
            tracing::warn!(
                capture_kind = capture_kind,
                helper_pid = helper_pid,
                error = %error,
                "capture helper job object setup failed; falling back to direct child termination"
            );
            None
        }
    };
    let stdout = child.stdout.take().map(spawn_capture_pipe_reader);
    let stderr = child.stderr.take().map(spawn_capture_pipe_reader);
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(&request).and_then(|_| stdin.flush()) {
            terminate_capture_helper(&mut child, job.as_ref(), 1);
            let _ = child.wait();
            return Err(CaptureError {
                code: CaptureErrorCode::CaptureFailed,
                message: format!(
                    "failed to send {capture_kind} capture helper request to pid {helper_pid}: {error}"
                ),
            });
        }
    } else {
        terminate_capture_helper(&mut child, job.as_ref(), 1);
        let _ = child.wait();
        return Err(CaptureError {
            code: CaptureErrorCode::CaptureFailed,
            message: format!("{capture_kind} capture helper pid {helper_pid} did not expose stdin"),
        });
    }
    tracing::info!(
        capture_kind = capture_kind,
        helper_pid = helper_pid,
        "capture helper spawned"
    );

    tracing::info!(
        capture_kind = capture_kind,
        helper_pid = helper_pid,
        timeout_secs = timeout.as_secs(),
        "capture helper waiting for process exit"
    );
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                terminate_capture_helper(&mut child, job.as_ref(), 1);
                let _ = child.wait();
                let stdout = join_capture_pipe_reader(stdout);
                let stderr = join_capture_pipe_reader(stderr);
                let error = capture_helper_timeout_error(
                    capture_kind,
                    timeout,
                    helper_pid,
                    elapsed_ms,
                    &stderr,
                );
                tracing::warn!(
                    capture_kind = capture_kind,
                    helper_pid = helper_pid,
                    timeout_secs = timeout.as_secs(),
                    elapsed_ms = elapsed_ms,
                    stdout_bytes = stdout.len(),
                    stderr = %summarize_output(&stderr),
                    "capture helper timed out"
                );
                return Err(error);
            }
            Err(error) => {
                terminate_capture_helper(&mut child, job.as_ref(), 1);
                let _ = child.wait();
                return Err(CaptureError {
                    code: CaptureErrorCode::CaptureFailed,
                    message: format!(
                        "failed to wait for {capture_kind} capture helper pid {helper_pid}: {error}"
                    ),
                });
            }
        }
    };

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let stdout = join_capture_pipe_reader(stdout);
    let stderr = join_capture_pipe_reader(stderr);
    tracing::info!(
        capture_kind = capture_kind,
        helper_pid = helper_pid,
        success = status.success(),
        status_code = status.code(),
        elapsed_ms = elapsed_ms,
        stdout_bytes = stdout.len(),
        stderr_bytes = stderr.len(),
        "capture helper exited"
    );
    decode_capture_helper_output(CaptureHelperProcessOutput {
        success: status.success(),
        status_code: status.code(),
        stdout,
        stderr,
        helper_pid: Some(helper_pid),
        elapsed_ms: Some(elapsed_ms),
    })
}

#[cfg(windows)]
fn configure_capture_helper_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    command.creation_flags(CREATE_NO_WINDOW.0);
}

#[cfg(windows)]
fn spawn_capture_pipe_reader<R>(mut reader: R) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

#[cfg(windows)]
fn join_capture_pipe_reader(handle: Option<thread::JoinHandle<String>>) -> String {
    handle
        .map(|handle| {
            handle
                .join()
                .unwrap_or_else(|_| "<capture helper pipe reader panicked>".into())
        })
        .unwrap_or_default()
}

#[cfg(windows)]
fn terminate_capture_helper(child: &mut Child, job: Option<&CaptureHelperJob>, exit_code: u32) {
    if let Some(job) = job {
        if let Err(error) = job.terminate(exit_code) {
            tracing::warn!(
                helper_pid = child.id(),
                error = %error,
                "failed to terminate capture helper job; falling back to child.kill"
            );
            let _ = child.kill();
        }
    } else {
        let _ = child.kill();
    }
}

#[cfg(windows)]
impl CaptureHelperJob {
    fn create_for_child(child: &Child) -> Result<Self, String> {
        use std::mem::size_of;
        use std::os::windows::io::AsRawHandle;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map_err(|error| format!("CreateJobObjectW failed: {error}"))?;
        let job = Self { handle };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(|error| format!("SetInformationJobObject failed: {error}"))?;
        unsafe { AssignProcessToJobObject(job.handle, HANDLE(child.as_raw_handle())) }
            .map_err(|error| format!("AssignProcessToJobObject failed: {error}"))?;
        Ok(job)
    }

    fn terminate(&self, exit_code: u32) -> Result<(), String> {
        use windows::Win32::System::JobObjects::TerminateJobObject;

        unsafe { TerminateJobObject(self.handle, exit_code) }
            .map_err(|error| format!("TerminateJobObject failed: {error}"))
    }
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn decode_capture_helper_output(
    output: CaptureHelperProcessOutput,
) -> Result<ScreenshotResult, CaptureError> {
    tracing::info!(
        success = output.success,
        status_code = output.status_code,
        helper_pid = output.helper_pid,
        elapsed_ms = output.elapsed_ms,
        stdout_bytes = output.stdout.len(),
        stderr_bytes = output.stderr.len(),
        "decoding capture helper output"
    );
    if !output.success {
        let status = output.status_code.map(format_exit_status_code);
        let stderr = summarize_output(&output.stderr);
        let pid = output
            .helper_pid
            .map(|pid| format!(" pid {pid}"))
            .unwrap_or_default();
        let elapsed = output
            .elapsed_ms
            .map(|elapsed_ms| format!(" after {elapsed_ms}ms"))
            .unwrap_or_default();
        let details = if stderr.is_empty() {
            format!(
                "capture helper{pid} exited with status {}{elapsed}",
                status.unwrap_or_else(|| "unknown".into())
            )
        } else {
            format!(
                "capture helper{pid} exited with status {}{elapsed}; stderr: {stderr}",
                status.unwrap_or_else(|| "unknown".into())
            )
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
fn capture_helper_timeout_error(
    capture_kind: &str,
    timeout: Duration,
    helper_pid: u32,
    elapsed_ms: u64,
    stderr: &str,
) -> CaptureError {
    let stderr = summarize_output(stderr);
    let suffix = if stderr.is_empty() {
        String::new()
    } else {
        format!("; stderr: {stderr}")
    };
    CaptureError {
        code: CaptureErrorCode::CaptureFailed,
        message: format!(
            "{capture_kind} capture helper pid {helper_pid} timed out after {}s ({elapsed_ms}ms elapsed){suffix}",
            timeout.as_secs(),
        ),
    }
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn format_exit_status_code(code: i32) -> String {
    let unsigned = code as u32;
    let label = match unsigned {
        0xC000_0005 => Some("access_violation"),
        0xC000_0409 => Some("stack_buffer_overrun"),
        0xC000_00FD => Some("stack_overflow"),
        0xC000_0374 => Some("heap_corruption"),
        0xC000_013A => Some("control_c_exit"),
        0xC000_0135 => Some("dll_not_found"),
        _ => None,
    };
    match label {
        Some(label) => format!("{code} (0x{unsigned:08X}, {label})"),
        None => format!("{code} (0x{unsigned:08X})"),
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
        let capture_dir = PathBuf::from("captures-root");
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

        let path = screenshot_window_output_path(&capture_dir, &window);

        assert_eq!(
            path,
            capture_dir
                .join("window-0x0000000000981332.png")
                .to_string_lossy()
        );
        assert!(!Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(':'));
    }

    #[test]
    fn default_capture_dir_does_not_depend_on_process_cwd() {
        let path = default_capture_dir();

        assert_ne!(path, PathBuf::from("./captures"));
        assert!(!path.as_os_str().is_empty());
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
            helper_pid: Some(1234),
            elapsed_ms: Some(50),
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
            helper_pid: Some(1234),
            elapsed_ms: Some(50),
        };

        let error = decode_capture_helper_output(output).unwrap_err();

        assert_eq!(error.code, winctl::CaptureErrorCode::CaptureFailed);
        assert_eq!(error.message, "windows-capture panicked");
    }

    #[test]
    fn helper_timeout_is_reported_as_capture_failed() {
        let error = capture_helper_timeout_error(
            "window",
            std::time::Duration::from_secs(12),
            1234,
            12_050,
            "native stack stopped responding",
        );

        assert_eq!(error.code, winctl::CaptureErrorCode::CaptureFailed);
        assert!(error
            .message
            .contains("window capture helper pid 1234 timed out"));
        assert!(error.message.contains("12s"));
        assert!(error.message.contains("12050ms"));
        assert!(error.message.contains("native stack stopped responding"));
    }

    #[test]
    fn helper_crash_status_includes_native_exit_label() {
        let output = CaptureHelperProcessOutput {
            success: false,
            status_code: Some(0xC000_0005_u32 as i32),
            stdout: String::new(),
            stderr: "fault in d3d11".into(),
            helper_pid: Some(4321),
            elapsed_ms: Some(99),
        };

        let error = decode_capture_helper_output(output).unwrap_err();

        assert_eq!(error.code, winctl::CaptureErrorCode::CaptureFailed);
        assert!(error.message.contains("pid 4321"));
        assert!(error.message.contains("0xC0000005"));
        assert!(error.message.contains("access_violation"));
        assert!(error.message.contains("fault in d3d11"));
    }
}
