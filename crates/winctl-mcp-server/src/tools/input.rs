use crate::tools::windows::windows_window_from_point;
use crate::AppState;
use winctl::{
    click_screen, focus_window, parse_mouse_button, resolve_screen_point, type_text_unicode,
    ClickRequest, TypeTextRequest,
};

pub fn input_click(state: &AppState, req: ClickRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        x = req.x,
        y = req.y,
        coordinate_space = ?req.coordinate_space,
        button = ?req.button,
        fail_if_outside_bound = ?req.fail_if_outside_bound,
        "input.click requested"
    );
    let window = match state.revalidate_bound_window(&req.bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error_code = ?error.code,
                "input.click revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };
    let point = match resolve_screen_point(Some(&window), req.x, req.y, &req.coordinate_space) {
        Ok(point) => {
            tracing::info!(
                bound_id = %req.bound_id,
                screen_x = point.screen_x,
                screen_y = point.screen_y,
                coordinate_space = ?point.coordinate_space,
                "input.click coordinate resolved"
            );
            point
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error = %error,
                "input.click coordinate resolution failed"
            );
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "coordinate_resolution_failed",
                    "message": error.to_string()
                }
            });
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
    tracing::info!(
        bound_id = %req.bound_id,
        screen_x = point.screen_x,
        screen_y = point.screen_y,
        belongs_to_bound_window = belongs,
        "input.click preflight completed"
    );
    if req.fail_if_outside_bound.unwrap_or(true) && !belongs {
        tracing::warn!(
            bound_id = %req.bound_id,
            screen_x = point.screen_x,
            screen_y = point.screen_y,
            "input.click preflight rejected point"
        );
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
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error_code = ?error.code,
                "input.click invalid button"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };
    let click = match click_screen(&point, button) {
        Ok(click) => {
            tracing::info!(
                bound_id = %req.bound_id,
                screen_x = click.screen_x,
                screen_y = click.screen_y,
                button = ?click.button,
                "input.click dispatched"
            );
            click
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error_code = ?error.code,
                screen_x = point.screen_x,
                screen_y = point.screen_y,
                "input.click dispatch failed"
            );
            return serde_json::json!({"ok": false, "error": error, "point": point, "preflight": preflight});
        }
    };

    serde_json::json!({"ok": true, "point": point, "preflight": preflight, "click": click})
}

pub fn input_type_text(state: &AppState, req: TypeTextRequest) -> serde_json::Value {
    let text_chars = req.text.chars().count();
    let text_utf16_units = req.text.encode_utf16().count();
    tracing::info!(
        bound_id = %req.bound_id,
        text_chars = text_chars,
        text_utf16_units = text_utf16_units,
        "input.type_text requested"
    );
    let window = match state.revalidate_bound_window(&req.bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error_code = ?error.code,
                "input.type_text revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };
    if let Err(error) = focus_window(&window) {
        tracing::warn!(
            bound_id = %req.bound_id,
            hwnd = %window.hwnd_hex,
            pid = window.pid,
            error_code = ?error.code,
            "input.type_text focus failed"
        );
        return serde_json::json!({"ok": false, "error": error});
    }
    let typed = match type_text_unicode(&req.text) {
        Ok(typed) => {
            tracing::info!(
                bound_id = %req.bound_id,
                utf16_units = typed.utf16_units,
                "input.type_text dispatched"
            );
            typed
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error_code = ?error.code,
                "input.type_text dispatch failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };

    serde_json::json!({"ok": true, "bound_id": req.bound_id, "typed": typed})
}
