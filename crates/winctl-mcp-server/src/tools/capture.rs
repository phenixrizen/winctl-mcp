use crate::{AppState, VideoStartRequest, VideoStopRequest, WindowImageChangeWaitRequest};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::atomic::AtomicU64;
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
use std::process::{Command, Stdio};

pub(crate) const CAPTURE_HELPER_ENV: &str = "WINCTL_CAPTURE_HELPER";
pub(crate) const CAPTURE_DIR_ENV: &str = "WINCTL_CAPTURE_DIR";
const CAPTURE_HELPER_REQUEST_ENV: &str = "WINCTL_CAPTURE_REQUEST";
const CAPTURE_HELPER_RESPONSE_ENV: &str = "WINCTL_CAPTURE_RESPONSE";
#[cfg(windows)]
const CAPTURE_HELPER_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(windows)]
static CAPTURE_HELPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
