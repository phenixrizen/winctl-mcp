use crate::AppState;
use winctl::{monitors, screenshot_display_to_path, screenshot_window_to_path};

pub fn screenshot_window(state: &AppState, bound_id: String) -> serde_json::Value {
    let window = match state.revalidate_bound_window(&bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };

    match screenshot_window_to_path(&window, format!("./captures/{bound_id}.png")) {
        Ok(screenshot) => serde_json::json!({"ok": true, "screenshot": screenshot}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn screenshot_display(display_index: usize) -> serde_json::Value {
    let desktop = monitors();
    let Some(monitor) = desktop.monitors.get(display_index) else {
        return serde_json::json!({"ok": false, "error": {"code": "no_display", "message": format!("display index {display_index} is unavailable")}});
    };

    match screenshot_display_to_path(
        display_index,
        monitor,
        format!("./captures/display-{display_index}.png"),
    ) {
        Ok(screenshot) => serde_json::json!({"ok": true, "screenshot": screenshot}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}
