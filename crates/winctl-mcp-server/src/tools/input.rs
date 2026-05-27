use crate::tools::windows::windows_window_from_point;
use crate::AppState;
use winctl::{ClickRequest, TypeTextRequest};

pub fn input_click(state: &AppState, req: ClickRequest) -> serde_json::Value {
    if let Err(error) = state.revalidate_bound_window(&req.bound_id) {
        return serde_json::json!({"ok": false, "error": error});
    }

    let preflight = windows_window_from_point(
        state,
        req.x as i32,
        req.y as i32,
        Some(req.bound_id.clone()),
    );
    let belongs = preflight
        .get("belongs_to_bound_window")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if req.fail_if_outside_bound.unwrap_or(true) && !belongs {
        return serde_json::json!({"ok": false, "error": "preflight failed: point not in bound window", "preflight": preflight});
    }
    serde_json::json!({"ok": true, "preflight": preflight, "note": "click dispatch pending Win32 SendInput implementation"})
}

pub fn input_type_text(state: &AppState, req: TypeTextRequest) -> serde_json::Value {
    if let Err(error) = state.revalidate_bound_window(&req.bound_id) {
        return serde_json::json!({"ok": false, "error": error});
    }

    serde_json::json!({"ok": true, "bound_id": req.bound_id, "typed_len": req.text.len(), "note": "type dispatch pending Win32 SendInput implementation"})
}
