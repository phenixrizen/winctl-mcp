use crate::{
    AppState, WaitForStateRequest, WindowMoveRequest, WindowResizeRequest, WindowsForProcessRequest,
};
use std::thread;
use std::time::{Duration, Instant};
use winctl::{
    child_processes, close_window, find_windows, focus_window, foreground_diagnostics,
    list_windows, maximize_window, minimize_window, monitors, move_window_to, resize_window_to,
    restore_window, window_from_point, WindowInfo, WindowSelector,
};

pub fn windows_list() -> serde_json::Value {
    tracing::info!("windows.list requested");
    serde_json::json!(list_windows())
}

pub fn windows_find(selector: WindowSelector) -> serde_json::Value {
    tracing::info!("windows.find requested");
    serde_json::json!(find_windows(&selector, &list_windows()))
}

pub fn windows_bind(state: &AppState, selector: WindowSelector) -> serde_json::Value {
    match state.bind_window(selector) {
        Ok(v) => serde_json::json!({"ok": true, "bound": v}),
        Err(e) => {
            tracing::warn!(error_code = ?e.code, candidates = e.candidates.len(), "windows.bind failed");
            serde_json::json!({"ok": false, "error": e})
        }
    }
}

pub fn windows_describe(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.describe requested");
    let guard = state.bound.lock().expect("bound mutex poisoned");
    match guard.get(&bound_id) {
        Some(bound) => {
            tracing::info!(
                bound_id = %bound_id,
                hwnd = %bound.identity.hwnd_hex,
                pid = bound.identity.pid,
                "windows.describe succeeded"
            );
            serde_json::json!({"ok": true, "bound": bound})
        }
        None => serde_json::json!({
            "ok": false,
            "error": {
                "code": "binding_not_found",
                "bound_id": bound_id,
                "message": "bound_id is not registered"
            }
        }),
    }
}

pub fn windows_focus(_state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.focus requested");
    match _state.revalidate_bound_window(&bound_id) {
        Ok(window) => match focus_window(&window) {
            Ok(focus) => {
                tracing::info!(
                    bound_id = %bound_id,
                    hwnd = %window.hwnd_hex,
                    pid = window.pid,
                    attempts = focus.attempts,
                    foreground = focus.foreground,
                    "windows.focus succeeded"
                );
                serde_json::json!({"ok": true, "bound_id": bound_id, "window": window, "focus": focus})
            }
            Err(error) => {
                tracing::warn!(
                    bound_id = %bound_id,
                    error_code = ?error.code,
                    "windows.focus dispatch failed"
                );
                serde_json::json!({"ok": false, "error": error})
            }
        },
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                "windows.focus revalidation failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn windows_window_from_point(
    state: &AppState,
    x: i32,
    y: i32,
    bound_id: Option<String>,
) -> serde_json::Value {
    let bound_id_for_log = bound_id.clone();
    let bound = bound_id.and_then(|id| state.bound.lock().ok().and_then(|g| g.get(&id).cloned()));
    let out = window_from_point(x, y, bound.as_ref());
    tracing::info!(
        x = x,
        y = y,
        bound_id = ?bound_id_for_log,
        top_level_hwnd = ?out.top_level.as_ref().map(|window| window.hwnd_hex.as_str()),
        top_level_pid = ?out.top_level.as_ref().map(|window| window.pid),
        child_hwnd = ?out.child.as_ref().map(|window| window.hwnd_hex.as_str()),
        child_pid = ?out.child.as_ref().map(|window| window.pid),
        belongs_to_bound_window = ?out.belongs_to_bound_window,
        "windows.window_from_point resolved"
    );
    serde_json::json!(out)
}

pub fn windows_monitors() -> serde_json::Value {
    serde_json::json!(monitors())
}

pub fn windows_wait_for_state(state: &AppState, request: WaitForStateRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        timeout_ms = request.timeout_ms,
        poll_interval_ms = request.poll_interval_ms,
        visible = ?request.visible,
        foreground = ?request.foreground,
        minimized = ?request.minimized,
        cloaked = ?request.cloaked,
        title_contains = ?request.title_contains,
        class_name_contains = ?request.class_name_contains,
        "windows.wait_for_state requested"
    );
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5_000));
    let poll_interval = Duration::from_millis(request.poll_interval_ms.unwrap_or(100).max(10));
    let started = Instant::now();
    let mut last_window;

    loop {
        match state.revalidate_bound_window(&request.bound_id) {
            Ok(window) => {
                if window_matches_state(&window, &request) {
                    tracing::info!(
                        bound_id = %request.bound_id,
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "windows.wait_for_state matched"
                    );
                    return serde_json::json!({
                        "ok": true,
                        "bound_id": request.bound_id,
                        "matched": true,
                        "timeout": false,
                        "window": window,
                        "timing": {"elapsed_ms": started.elapsed().as_millis() as u64}
                    });
                }
                last_window = Some(window);
            }
            Err(error) => {
                tracing::warn!(
                    bound_id = %request.bound_id,
                    error_code = ?error.code,
                    "windows.wait_for_state revalidation failed"
                );
                return serde_json::json!({"ok": false, "error": error});
            }
        }

        if started.elapsed() >= timeout {
            tracing::warn!(
                bound_id = %request.bound_id,
                timeout_ms = timeout.as_millis() as u64,
                "windows.wait_for_state timed out"
            );
            return serde_json::json!({
                "ok": false,
                "bound_id": request.bound_id,
                "matched": false,
                "timeout": true,
                "last_window": last_window,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64},
                "error": {
                    "code": "window_state_timeout",
                    "message": "bound window did not satisfy requested state before timeout"
                }
            });
        }

        thread::sleep(poll_interval);
    }
}

pub fn windows_move(state: &AppState, request: WindowMoveRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        x = request.x,
        y = request.y,
        "windows.move requested"
    );
    with_revalidated_window(state, &request.bound_id, "windows.move", |window| {
        move_window_to(&window, request.x, request.y)
    })
}

pub fn windows_resize(state: &AppState, request: WindowResizeRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        width = request.width,
        height = request.height,
        "windows.resize requested"
    );
    with_revalidated_window(state, &request.bound_id, "windows.resize", |window| {
        resize_window_to(&window, request.width, request.height)
    })
}

pub fn windows_minimize(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.minimize requested");
    with_revalidated_window(state, &bound_id, "windows.minimize", |window| {
        minimize_window(&window)
    })
}

pub fn windows_maximize(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.maximize requested");
    with_revalidated_window(state, &bound_id, "windows.maximize", |window| {
        maximize_window(&window)
    })
}

pub fn windows_restore(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.restore requested");
    with_revalidated_window(state, &bound_id, "windows.restore", |window| {
        restore_window(&window)
    })
}

pub fn windows_close(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.close requested");
    with_revalidated_window(state, &bound_id, "windows.close", |window| {
        close_window(&window)
    })
}

pub fn windows_foreground_diagnostics(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "windows.foreground_diagnostics requested");
    with_revalidated_window(
        state,
        &bound_id,
        "windows.foreground_diagnostics",
        |window| foreground_diagnostics(&window),
    )
}

pub fn windows_for_process(
    state: &AppState,
    request: WindowsForProcessRequest,
) -> serde_json::Value {
    tracing::info!(
        pid = ?request.pid,
        launch_id = ?request.launch_id,
        include_child_process_windows = request.include_child_process_windows,
        "windows.for_process requested"
    );
    let Some(pid) = request.pid.or_else(|| {
        request
            .launch_id
            .as_deref()
            .and_then(|launch_id| state.tracked_by_launch_id(launch_id))
            .map(|tracked| tracked.pid)
    }) else {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "missing_process_target",
                "message": "pid or launch_id is required"
            }
        });
    };
    let child_pids: Vec<u32> = if request.include_child_process_windows {
        child_processes(pid)
            .unwrap_or_default()
            .into_iter()
            .map(|process| process.pid)
            .collect()
    } else {
        Vec::new()
    };
    let windows: Vec<_> = list_windows()
        .into_iter()
        .filter(|window| window.pid == pid || child_pids.contains(&window.pid))
        .map(|window| {
            serde_json::json!({
                "window": window,
                "belongs_to_original_pid": window.pid == pid,
                "child_process_window": window.pid != pid,
            })
        })
        .collect();
    serde_json::json!({
        "ok": true,
        "pid": pid,
        "launch_id": request.launch_id,
        "windows": windows,
        "child_pids": child_pids,
    })
}

fn with_revalidated_window<F>(
    state: &AppState,
    bound_id: &str,
    tool: &str,
    operation: F,
) -> serde_json::Value
where
    F: FnOnce(WindowInfo) -> Result<winctl::WindowManagementResult, winctl::WindowManagementError>,
{
    match state.revalidate_bound_window(bound_id) {
        Ok(window) => match operation(window) {
            Ok(result) => serde_json::json!({"ok": true, "bound_id": bound_id, "result": result}),
            Err(error) => {
                tracing::warn!(
                    bound_id = %bound_id,
                    error_code = %error.code,
                    tool = tool,
                    "window management operation failed"
                );
                serde_json::json!({"ok": false, "error": error})
            }
        },
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

fn window_matches_state(window: &WindowInfo, request: &WaitForStateRequest) -> bool {
    request
        .visible
        .map(|expected| window.visible == expected)
        .unwrap_or(true)
        && request
            .foreground
            .map(|expected| window.foreground == expected)
            .unwrap_or(true)
        && request
            .minimized
            .map(|expected| window.minimized == expected)
            .unwrap_or(true)
        && request
            .cloaked
            .map(|expected| window.cloaked == expected)
            .unwrap_or(true)
        && request
            .title_contains
            .as_ref()
            .map(|needle| contains_case_insensitive(&window.title, needle))
            .unwrap_or(true)
        && request
            .class_name_contains
            .as_ref()
            .map(|needle| contains_case_insensitive(&window.class_name, needle))
            .unwrap_or(true)
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}
