use crate::AppState;
use winctl::{
    find_windows, list_windows, monitors, BoundWindow, WindowFromPointResult, WindowSelector,
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
        Ok(window) => serde_json::json!({"ok": true, "bound_id": bound_id, "window": window}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn windows_window_from_point(
    state: &AppState,
    x: i32,
    y: i32,
    bound_id: Option<String>,
) -> serde_json::Value {
    let top = list_windows()
        .into_iter()
        .find(|w| x >= w.x && y >= w.y && x < w.x + w.width && y < w.y + w.height);

    let belongs = bound_id.and_then(|id| {
        state
            .bound
            .lock()
            .ok()
            .and_then(|g| g.get(&id).cloned())
            .and_then(|b: BoundWindow| {
                top.as_ref()
                    .map(|t| t.hwnd == b.window.hwnd && t.pid == b.window.pid)
            })
    });

    let out = WindowFromPointResult {
        screen_x: x,
        screen_y: y,
        child: top.clone(),
        top_level: top,
        belongs_to_bound_window: belongs,
    };
    serde_json::json!(out)
}

pub fn windows_monitors() -> serde_json::Value {
    serde_json::json!(monitors())
}
