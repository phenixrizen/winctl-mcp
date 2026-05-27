use crate::tools::windows::windows_window_from_point;
use crate::AppState;
use winctl::{resolve_screen_point, ClickRequest, TypeTextRequest};

pub fn input_click(state: &AppState, req: ClickRequest) -> serde_json::Value {
    let window = match state.revalidate_bound_window(&req.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let point = match resolve_screen_point(Some(&window), req.x, req.y, &req.coordinate_space) {
        Ok(point) => point,
        Err(error) => return serde_json::json!({"ok": false, "error": error.to_string()}),
    };

    let preflight = windows_window_from_point(
        state,
        point.screen_x,
        point.screen_y,
        Some(req.bound_id.clone()),
    );
    let belongs = preflight
        .get("belongs_to_bound_window")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if req.fail_if_outside_bound.unwrap_or(true) && !belongs {
        return serde_json::json!({"ok": false, "error": "preflight failed: point not in bound window", "preflight": preflight});
    }
    serde_json::json!({"ok": true, "point": point, "preflight": preflight, "note": "click dispatch pending Win32 SendInput implementation"})
}

pub fn input_type_text(state: &AppState, req: TypeTextRequest) -> serde_json::Value {
    if let Err(error) = state.revalidate_bound_window(&req.bound_id) {
        return serde_json::json!({"ok": false, "error": error});
    }

    serde_json::json!({"ok": true, "bound_id": req.bound_id, "typed_len": req.text.len(), "note": "type dispatch pending Win32 SendInput implementation"})
}
