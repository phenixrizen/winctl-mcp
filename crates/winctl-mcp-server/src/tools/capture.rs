use crate::AppState;
use winctl::{CaptureRegion, ScreenshotResult};

pub fn screenshot_window(_state: &AppState, bound_id: String) -> serde_json::Value {
    let out = ScreenshotResult {
        output_path: format!("./captures/{bound_id}.png"),
        region_virtual_desktop: CaptureRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        width: 0,
        height: 0,
        image_base64: None,
    };
    serde_json::json!({"ok": true, "screenshot": out, "note": "capture implementation pending windows-capture integration"})
}

pub fn screenshot_display(display_index: usize) -> serde_json::Value {
    let out = ScreenshotResult {
        output_path: format!("./captures/display-{display_index}.png"),
        region_virtual_desktop: CaptureRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        width: 0,
        height: 0,
        image_base64: None,
    };
    serde_json::json!({"ok": true, "screenshot": out, "note": "capture implementation pending windows-capture integration"})
}
