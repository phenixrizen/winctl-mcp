use crate::AppState;
use winctl::{
    find_windows, focus_window, list_windows, monitors, window_from_point, WindowSelector,
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
