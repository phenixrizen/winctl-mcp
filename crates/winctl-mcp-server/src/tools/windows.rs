use crate::AppState;
use winctl::{
    find_windows, focus_window, list_windows, monitors, window_from_point, WindowSelector,
};

pub fn windows_list() -> serde_json::Value {
    serde_json::json!(list_windows())
}

pub fn windows_find(selector: WindowSelector) -> serde_json::Value {
    serde_json::json!(find_windows(&selector, &list_windows()))
}

pub fn windows_bind(state: &AppState, selector: WindowSelector) -> serde_json::Value {
    match state.bind_window(selector) {
        Ok(v) => serde_json::json!({"ok": true, "bound": v}),
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    }
}

pub fn windows_describe(state: &AppState, bound_id: String) -> serde_json::Value {
    let guard = state.bound.lock().expect("bound mutex poisoned");
    let bound = guard.get(&bound_id);
    serde_json::json!(bound)
}

pub fn windows_focus(_state: &AppState, bound_id: String) -> serde_json::Value {
    match _state.revalidate_bound_window(&bound_id) {
        Ok(window) => match focus_window(&window) {
            Ok(focus) => {
                serde_json::json!({"ok": true, "bound_id": bound_id, "window": window, "focus": focus})
            }
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        },
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn windows_window_from_point(
    state: &AppState,
    x: i32,
    y: i32,
    bound_id: Option<String>,
) -> serde_json::Value {
    let bound = bound_id.and_then(|id| state.bound.lock().ok().and_then(|g| g.get(&id).cloned()));
    let out = window_from_point(x, y, bound.as_ref());
    serde_json::json!(out)
}

pub fn windows_monitors() -> serde_json::Value {
    serde_json::json!(monitors())
}
