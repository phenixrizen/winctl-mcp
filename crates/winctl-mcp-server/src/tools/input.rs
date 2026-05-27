use crate::tools::windows::windows_window_from_point;
use crate::AppState;
use winctl::{
    click_screen, focus_window, parse_mouse_button, resolve_screen_point, type_text_unicode,
    ClickRequest, TypeTextRequest,
};

pub fn input_click(state: &AppState, req: ClickRequest) -> serde_json::Value {
    let window = match state.revalidate_bound_window(&req.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let point = match resolve_screen_point(Some(&window), req.x, req.y, &req.coordinate_space) {
        Ok(point) => point,
        Err(error) => {
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "coordinate_resolution_failed",
                    "message": error.to_string()
                }
            })
        }
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
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "preflight_outside_bound",
                "message": "point did not resolve to the bound window"
            },
            "preflight": preflight
        });
    }
    let button = match parse_mouse_button(req.button.as_deref()) {
        Ok(button) => button,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let click = match click_screen(&point, button) {
        Ok(click) => click,
        Err(error) => {
            return serde_json::json!({"ok": false, "error": error, "point": point, "preflight": preflight})
        }
    };

    serde_json::json!({"ok": true, "point": point, "preflight": preflight, "click": click})
}

pub fn input_type_text(state: &AppState, req: TypeTextRequest) -> serde_json::Value {
    let window = match state.revalidate_bound_window(&req.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    if let Err(error) = focus_window(&window) {
        return serde_json::json!({"ok": false, "error": error});
    }
    let typed = match type_text_unicode(&req.text) {
        Ok(typed) => typed,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };

    serde_json::json!({"ok": true, "bound_id": req.bound_id, "typed": typed})
}
