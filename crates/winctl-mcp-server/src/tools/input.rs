use crate::tools::windows::windows_window_from_point;
use crate::AppState;
use std::thread;
use std::time::{Duration, Instant};
use winctl::{
    click_screen, double_click_screen, drag_screen, focus_window, key_down_virtual, key_up_virtual,
    monitors, move_mouse_screen, parse_mouse_button, parse_shortcut_keys, parse_virtual_key,
    resolve_screen_point, scroll_screen, shortcut_virtual, type_text_unicode, ClickRequest,
    CoordinateSpace, DelayRequest, DoubleClickRequest, DragRequest, KeyRequest, MouseMoveRequest,
    ResolvedPoint, ScrollRequest, ShortcutRequest, TypeTextRequest, WindowInfo,
};

struct ResolvedPointerTarget {
    point: ResolvedPoint,
    preflight: serde_json::Value,
    replay: serde_json::Value,
}

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
    let started = Instant::now();
    let target = match resolve_pointer_target(
        state,
        &req.bound_id,
        req.x,
        req.y,
        &req.coordinate_space,
        req.fail_if_outside_bound,
        "input.click",
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };
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
    let click = match click_screen(&target.point, button) {
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
                screen_x = target.point.screen_x,
                screen_y = target.point.screen_y,
                "input.click dispatch failed"
            );
            return serde_json::json!({"ok": false, "error": error, "point": target.point, "preflight": target.preflight, "replay": target.replay});
        }
    };

    serde_json::json!({
        "ok": true,
        "point": target.point,
        "preflight": target.preflight,
        "click": click,
        "replay": target.replay,
        "timing": elapsed_timing(started)
    })
}

pub fn input_mouse_move(state: &AppState, req: MouseMoveRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        x = req.x,
        y = req.y,
        coordinate_space = ?req.coordinate_space,
        "input.mouse_move requested"
    );
    let started = Instant::now();
    let target = match resolve_pointer_target(
        state,
        &req.bound_id,
        req.x,
        req.y,
        &req.coordinate_space,
        req.fail_if_outside_bound,
        "input.mouse_move",
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };

    match move_mouse_screen(&target.point) {
        Ok(move_result) => serde_json::json!({
            "ok": true,
            "point": target.point,
            "preflight": target.preflight,
            "move": move_result,
            "replay": target.replay,
            "timing": elapsed_timing(started)
        }),
        Err(error) => {
            tracing::warn!(
                bound_id = %req.bound_id,
                error_code = ?error.code,
                "input.mouse_move dispatch failed"
            );
            serde_json::json!({"ok": false, "error": error, "point": target.point, "preflight": target.preflight, "replay": target.replay})
        }
    }
}

pub fn input_double_click(state: &AppState, req: DoubleClickRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        x = req.x,
        y = req.y,
        coordinate_space = ?req.coordinate_space,
        button = ?req.button,
        "input.double_click requested"
    );
    let started = Instant::now();
    let target = match resolve_pointer_target(
        state,
        &req.bound_id,
        req.x,
        req.y,
        &req.coordinate_space,
        req.fail_if_outside_bound,
        "input.double_click",
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let button = match parse_mouse_button(req.button.as_deref()) {
        Ok(button) => button,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let interval_ms = req.interval_ms.unwrap_or(80);

    match double_click_screen(&target.point, button, interval_ms) {
        Ok(double_click) => serde_json::json!({
            "ok": true,
            "point": target.point,
            "preflight": target.preflight,
            "double_click": double_click,
            "replay": target.replay,
            "timing": elapsed_timing(started)
        }),
        Err(error) => {
            serde_json::json!({"ok": false, "error": error, "point": target.point, "preflight": target.preflight, "replay": target.replay})
        }
    }
}

pub fn input_drag(state: &AppState, req: DragRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        start_x = req.start_x,
        start_y = req.start_y,
        end_x = req.end_x,
        end_y = req.end_y,
        coordinate_space = ?req.coordinate_space,
        button = ?req.button,
        duration_ms = ?req.duration_ms,
        "input.drag requested"
    );
    let started = Instant::now();
    let start = match resolve_pointer_target(
        state,
        &req.bound_id,
        req.start_x,
        req.start_y,
        &req.coordinate_space,
        req.fail_if_outside_bound,
        "input.drag.start",
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let end = match resolve_pointer_target(
        state,
        &req.bound_id,
        req.end_x,
        req.end_y,
        &req.coordinate_space,
        req.fail_if_outside_bound,
        "input.drag.end",
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let button = match parse_mouse_button(req.button.as_deref()) {
        Ok(button) => button,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let duration_ms = req.duration_ms.unwrap_or(250);

    match drag_screen(&start.point, &end.point, button, duration_ms) {
        Ok(drag) => serde_json::json!({
            "ok": true,
            "start_point": start.point,
            "end_point": end.point,
            "start_preflight": start.preflight,
            "end_preflight": end.preflight,
            "drag": drag,
            "replay": {"start": start.replay, "end": end.replay},
            "timing": elapsed_timing(started)
        }),
        Err(error) => {
            serde_json::json!({"ok": false, "error": error, "start_point": start.point, "end_point": end.point, "replay": {"start": start.replay, "end": end.replay}})
        }
    }
}

pub fn input_scroll(state: &AppState, req: ScrollRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        x = req.x,
        y = req.y,
        coordinate_space = ?req.coordinate_space,
        delta_x = ?req.delta_x,
        delta_y = ?req.delta_y,
        "input.scroll requested"
    );
    let started = Instant::now();
    let target = match resolve_pointer_target(
        state,
        &req.bound_id,
        req.x,
        req.y,
        &req.coordinate_space,
        req.fail_if_outside_bound,
        "input.scroll",
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let delta_x = req.delta_x.unwrap_or(0);
    let delta_y = req.delta_y.unwrap_or(-120);

    match scroll_screen(&target.point, delta_x, delta_y) {
        Ok(scroll) => serde_json::json!({
            "ok": true,
            "point": target.point,
            "preflight": target.preflight,
            "scroll": scroll,
            "replay": target.replay,
            "timing": elapsed_timing(started)
        }),
        Err(error) => {
            serde_json::json!({"ok": false, "error": error, "point": target.point, "preflight": target.preflight, "replay": target.replay})
        }
    }
}

pub fn input_key_down(state: &AppState, req: KeyRequest) -> serde_json::Value {
    dispatch_key_action(state, req, "input.key_down", true)
}

pub fn input_key_up(state: &AppState, req: KeyRequest) -> serde_json::Value {
    dispatch_key_action(state, req, "input.key_up", false)
}

pub fn input_shortcut(state: &AppState, req: ShortcutRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        keys_count = req.keys.len(),
        hold_ms = ?req.hold_ms,
        "input.shortcut requested"
    );
    let started = Instant::now();
    let window = match revalidate_and_focus(state, &req.bound_id, "input.shortcut") {
        Ok(window) => window,
        Err(error) => return error,
    };
    let keys = match parse_shortcut_keys(&req.keys) {
        Ok(keys) => keys,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let hold_ms = req.hold_ms.unwrap_or(50);

    match shortcut_virtual(&keys, hold_ms) {
        Ok(shortcut) => serde_json::json!({
            "ok": true,
            "bound_id": req.bound_id,
            "shortcut": shortcut,
            "replay": keyboard_replay_metadata(&window),
            "timing": elapsed_timing(started)
        }),
        Err(error) => {
            serde_json::json!({"ok": false, "error": error, "replay": keyboard_replay_metadata(&window)})
        }
    }
}

pub fn input_delay(req: DelayRequest) -> serde_json::Value {
    tracing::info!(duration_ms = req.duration_ms, "input.delay requested");
    let started = Instant::now();
    thread::sleep(Duration::from_millis(req.duration_ms.min(60_000)));
    serde_json::json!({
        "ok": true,
        "delay": {
            "requested_duration_ms": req.duration_ms,
            "elapsed_ms": started.elapsed().as_millis() as u64
        },
        "timing": elapsed_timing(started)
    })
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

pub fn input_type_secret_value(
    state: &AppState,
    bound_id: String,
    secret: &str,
) -> serde_json::Value {
    tracing::info!(bound_id = %bound_id, "macro.type_secret input dispatch requested");
    let window = match state.revalidate_bound_window(&bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                "macro.type_secret revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };
    if let Err(error) = focus_window(&window) {
        tracing::warn!(
            bound_id = %bound_id,
            hwnd = %window.hwnd_hex,
            pid = window.pid,
            error_code = ?error.code,
            "macro.type_secret focus failed"
        );
        return serde_json::json!({"ok": false, "error": error});
    }
    match type_text_unicode(secret) {
        Ok(_) => {
            tracing::info!(bound_id = %bound_id, "macro.type_secret dispatched");
            serde_json::json!({
                "ok": true,
                "bound_id": bound_id,
                "typed_secret": true,
                "replay": keyboard_replay_metadata(&window)
            })
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                "macro.type_secret dispatch failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

fn dispatch_key_action(
    state: &AppState,
    req: KeyRequest,
    tool_name: &'static str,
    down: bool,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %req.bound_id,
        key = %req.key,
        tool_name = tool_name,
        "keyboard action requested"
    );
    let started = Instant::now();
    let window = match revalidate_and_focus(state, &req.bound_id, tool_name) {
        Ok(window) => window,
        Err(error) => return error,
    };
    let key = match parse_virtual_key(&req.key) {
        Ok(key) => key,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let result = if down {
        key_down_virtual(&key)
    } else {
        key_up_virtual(&key)
    };

    match result {
        Ok(key_result) => serde_json::json!({
            "ok": true,
            "bound_id": req.bound_id,
            "key": key_result,
            "replay": keyboard_replay_metadata(&window),
            "timing": elapsed_timing(started)
        }),
        Err(error) => {
            serde_json::json!({"ok": false, "error": error, "replay": keyboard_replay_metadata(&window)})
        }
    }
}

fn revalidate_and_focus(
    state: &AppState,
    bound_id: &str,
    tool_name: &'static str,
) -> Result<WindowInfo, serde_json::Value> {
    let window = match state.revalidate_bound_window(bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                tool_name = tool_name,
                "keyboard action revalidation failed"
            );
            return Err(serde_json::json!({"ok": false, "error": error}));
        }
    };
    if let Err(error) = focus_window(&window) {
        tracing::warn!(
            bound_id = %bound_id,
            hwnd = %window.hwnd_hex,
            pid = window.pid,
            error_code = ?error.code,
            tool_name = tool_name,
            "keyboard action focus failed"
        );
        return Err(serde_json::json!({"ok": false, "error": error}));
    }
    Ok(window)
}

fn resolve_pointer_target(
    state: &AppState,
    bound_id: &str,
    x: f64,
    y: f64,
    coordinate_space: &CoordinateSpace,
    fail_if_outside_bound: Option<bool>,
    tool_name: &'static str,
) -> Result<ResolvedPointerTarget, serde_json::Value> {
    let window = match state.revalidate_bound_window(bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                tool_name = tool_name,
                "pointer action revalidation failed"
            );
            return Err(serde_json::json!({"ok": false, "error": error}));
        }
    };
    let point = match resolve_screen_point(Some(&window), x, y, coordinate_space) {
        Ok(point) => {
            tracing::info!(
                bound_id = %bound_id,
                screen_x = point.screen_x,
                screen_y = point.screen_y,
                coordinate_space = ?point.coordinate_space,
                tool_name = tool_name,
                "pointer action coordinate resolved"
            );
            point
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %bound_id,
                error = %error,
                tool_name = tool_name,
                "pointer action coordinate resolution failed"
            );
            return Err(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "coordinate_resolution_failed",
                    "message": error.to_string()
                }
            }));
        }
    };

    let preflight = windows_window_from_point(
        state,
        point.screen_x,
        point.screen_y,
        Some(bound_id.to_owned()),
    );
    let belongs = preflight
        .get("belongs_to_bound_window")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    tracing::info!(
        bound_id = %bound_id,
        screen_x = point.screen_x,
        screen_y = point.screen_y,
        belongs_to_bound_window = belongs,
        tool_name = tool_name,
        "pointer action preflight completed"
    );
    if fail_if_outside_bound.unwrap_or(true) && !belongs {
        tracing::warn!(
            bound_id = %bound_id,
            screen_x = point.screen_x,
            screen_y = point.screen_y,
            tool_name = tool_name,
            "pointer action preflight rejected point"
        );
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "preflight_outside_bound",
                "message": "point did not resolve to the bound window"
            },
            "preflight": preflight
        }));
    }

    let replay = pointer_replay_metadata(&window, &point, &preflight);
    Ok(ResolvedPointerTarget {
        point,
        preflight,
        replay,
    })
}

fn pointer_replay_metadata(
    window: &WindowInfo,
    point: &ResolvedPoint,
    preflight: &serde_json::Value,
) -> serde_json::Value {
    let monitor = monitor_for_point(point.screen_x, point.screen_y);
    serde_json::json!({
        "coordinate_space": point.coordinate_space,
        "resolved_screen_position": {
            "x": point.screen_x,
            "y": point.screen_y
        },
        "window": window_replay_metadata(window),
        "monitor": monitor,
        "preflight": preflight,
    })
}

fn keyboard_replay_metadata(window: &WindowInfo) -> serde_json::Value {
    serde_json::json!({
        "window": window_replay_metadata(window),
    })
}

fn window_replay_metadata(window: &WindowInfo) -> serde_json::Value {
    serde_json::json!({
        "bound_window_identity": {
            "hwnd": window.hwnd,
            "hwnd_hex": window.hwnd_hex,
            "pid": window.pid,
            "process_name": window.process_name,
            "executable_path": window.exe_path,
        },
        "window_bounds": {
            "x": window.x,
            "y": window.y,
            "width": window.width,
            "height": window.height
        },
        "state": {
            "visible": window.visible,
            "foreground": window.foreground,
            "minimized": window.minimized,
            "cloaked": window.cloaked,
            "top_level": window.top_level
        }
    })
}

fn monitor_for_point(screen_x: i32, screen_y: i32) -> Option<serde_json::Value> {
    monitors()
        .monitors
        .into_iter()
        .enumerate()
        .find(|(_, monitor)| {
            screen_x >= monitor.x
                && screen_x < monitor.x + monitor.width
                && screen_y >= monitor.y
                && screen_y < monitor.y + monitor.height
        })
        .map(|(index, monitor)| {
            serde_json::json!({
                "index": index,
                "name": monitor.name,
                "x": monitor.x,
                "y": monitor.y,
                "width": monitor.width,
                "height": monitor.height,
                "dpi_scale": monitor.dpi_scale,
                "primary": monitor.primary,
            })
        })
}

fn elapsed_timing(started: Instant) -> serde_json::Value {
    serde_json::json!({
        "elapsed_ms": started.elapsed().as_millis() as u64,
    })
}
