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

    match screenshot_window_to_path(&window, screenshot_window_output_path(&window)) {
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
}
