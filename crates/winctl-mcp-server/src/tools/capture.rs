use crate::AppState;
use winctl::{monitors, screenshot_display_to_path, screenshot_window_to_path};

pub fn screenshot_window(state: &AppState, bound_id: String) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "capture.screenshot_window requested");
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

    match screenshot_window_to_path(&window, format!("./captures/{bound_id}.png")) {
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

pub fn screenshot_display(display_index: usize) -> serde_json::Value {
    tracing::info!(
        display_index = display_index,
        "capture.screenshot_display requested"
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

    match screenshot_display_to_path(
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
