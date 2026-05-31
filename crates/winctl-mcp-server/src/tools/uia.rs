use std::thread;
use std::time::{Duration, Instant};

use crate::{
    AppState, UiElementActionRequest, UiFindRequest, UiRangeValueRequest, UiResolveRequest,
    UiSetValueRequest, UiSnapshotRequest, UiWaitForElementRequest,
};
use winctl::{
    find_ui_elements, resolve_ui_element_ref, ui_automation_snapshot, ClickRequest,
    CoordinateSpace, ShortcutRequest, TypeTextRequest, UiAutomationSnapshot, UiElementInfo,
    UiRect,
};

pub fn uia_snapshot(state: &AppState, request: UiSnapshotRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        max_depth = request.max_depth,
        max_elements = request.max_elements,
        "uia.snapshot requested"
    );
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %request.bound_id,
                error_code = ?error.code,
                "uia.snapshot revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };

    match ui_automation_snapshot(
        &window,
        request.max_depth.unwrap_or(8),
        request.max_elements.unwrap_or(2_000),
    ) {
        Ok(snapshot) => {
            tracing::info!(
                bound_id = %request.bound_id,
                flattened_count = snapshot.flattened_count,
                truncated = snapshot.truncated,
                warnings = snapshot.warnings.len(),
                "uia.snapshot succeeded"
            );
            serde_json::json!({"ok": true, "snapshot": snapshot})
        }
        Err(error) => {
            tracing::warn!(
                bound_id = %request.bound_id,
                error_code = ?error.code,
                "uia.snapshot failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn uia_find(state: &AppState, request: UiFindRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        selector = ?request.selector,
        max_depth = request.max_depth,
        max_elements = request.max_elements,
        "uia.find requested"
    );
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %request.bound_id,
                error_code = ?error.code,
                "uia.find revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };
    let snapshot = match ui_automation_snapshot(
        &window,
        request.max_depth.unwrap_or(8),
        request.max_elements.unwrap_or(2_000),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let matches = find_ui_elements(&snapshot, &request.selector);
    let match_count = matches.len();
    let diagnostics = candidate_diagnostics(match_count, snapshot.truncated);
    serde_json::json!({
        "ok": true,
        "matches": matches,
        "match_count": match_count,
        "snapshot_summary": {
            "flattened_count": snapshot.flattened_count,
            "truncated": snapshot.truncated,
            "warnings": snapshot.warnings,
        },
        "diagnostics": diagnostics
    })
}

pub fn uia_resolve(state: &AppState, request: UiResolveRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = %request.element_ref,
        max_depth = request.max_depth,
        max_elements = request.max_elements,
        "uia.resolve requested"
    );
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(
                bound_id = %request.bound_id,
                error_code = ?error.code,
                "uia.resolve revalidation failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };
    let snapshot = match ui_automation_snapshot(
        &window,
        request.max_depth.unwrap_or(8),
        request.max_elements.unwrap_or(2_000),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };

    match resolve_ui_element_ref(&snapshot, &request.element_ref) {
        Some(element) => serde_json::json!({
            "ok": true,
            "element": element,
            "revalidated": true,
            "snapshot_summary": {
                "flattened_count": snapshot.flattened_count,
                "truncated": snapshot.truncated,
                "warnings": snapshot.warnings,
            }
        }),
        None => serde_json::json!({
            "ok": false,
            "revalidated": false,
            "error": {
                "code": "uia_element_not_found",
                "message": "element reference was not found in the current UI Automation snapshot"
            },
            "snapshot_summary": {
                "flattened_count": snapshot.flattened_count,
                "truncated": snapshot.truncated,
                "warnings": snapshot.warnings,
            }
        }),
    }
}

fn candidate_diagnostics(match_count: usize, snapshot_truncated: bool) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if match_count == 0 {
        diagnostics.push("no elements matched the selector".into());
    } else if match_count > 1 {
        diagnostics.push(
            "selector matched multiple elements; refine before element-targeted actions".into(),
        );
    }
    if snapshot_truncated {
        diagnostics
            .push("snapshot was truncated; increase max_depth or max_elements if needed".into());
    }
    diagnostics
}

pub fn uia_invoke(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    click_fallback_action(state, request, "uia.invoke", "invoke", true)
}

pub fn uia_set_focus(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    click_fallback_action(state, request, "uia.set_focus", "set_focus", true)
}

pub fn uia_select(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    click_fallback_action(state, request, "uia.select", "select", true)
}

pub fn uia_toggle(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    click_fallback_action(state, request, "uia.toggle", "toggle", true)
}

pub fn uia_set_value(state: &AppState, request: UiSetValueRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        value_chars = request.value.chars().count(),
        replace_existing = request.replace_existing,
        "uia.set_value requested"
    );
    let action_request = UiElementActionRequest {
        bound_id: request.bound_id.clone(),
        element_ref: request.element_ref.clone(),
        selector: request.selector.clone(),
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.allow_offscreen,
    };
    let resolved = match resolve_action_target(state, &action_request) {
        Ok(resolved) => resolved,
        Err(error) => return error,
    };
    let fallback = coordinate_fallback_hint(&resolved.element);
    if let Err(error) = ensure_actionable(&resolved.element, request.allow_offscreen, true) {
        return unsupported_action("uia.set_value", "value_pattern_unavailable", resolved, error);
    }
    let click = dispatch_center_click(state, &request.bound_id, &resolved.element);
    if !json_ok(&click) {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "uia_coordinate_fallback_failed",
                "message": "element focus fallback failed before typing"
            },
            "before": resolved.element,
            "coordinate_fallback_hint": fallback,
            "focus_result": click,
        });
    }
    let select_existing = if request.replace_existing {
        Some(crate::tools::input::input_shortcut(
            state,
            ShortcutRequest {
                bound_id: request.bound_id.clone(),
                keys: vec!["ctrl".into(), "a".into()],
                hold_ms: Some(30),
            },
        ))
    } else {
        None
    };
    let type_result = crate::tools::input::input_type_text(
        state,
        TypeTextRequest {
            bound_id: request.bound_id.clone(),
            text: request.value,
        },
    );
    let after = resolve_action_target(state, &action_request)
        .ok()
        .map(|resolved| resolved.element);
    serde_json::json!({
        "ok": json_ok(&type_result),
        "pattern_used": "coordinate_focus_type_fallback",
        "direct_uia_pattern_used": false,
        "before": resolved.element,
        "after": after,
        "coordinate_fallback_hint": fallback,
        "focus_result": click,
        "select_existing_result": select_existing,
        "type_result": type_result,
        "warnings": [
            "direct UI Automation ValuePattern support is not enabled in this build; action used focus/type fallback after strict element revalidation"
        ]
    })
}

pub fn uia_get_value(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        "uia.get_value requested"
    );
    match resolve_action_target(state, &request) {
        Ok(resolved) => {
            let element = resolved.element;
            let value = serde_json::json!({
                "name": element.name.clone(),
                "automation_id": element.automation_id.clone(),
                "class_name": element.class_name.clone(),
                "role": element.role.clone(),
                "focused": element.focused,
                "enabled": element.enabled,
            });
            serde_json::json!({
                "ok": true,
                "pattern_used": "snapshot_properties",
                "direct_uia_pattern_used": false,
                "element": element,
                "value": value,
                "snapshot_summary": snapshot_summary(&resolved.snapshot),
                "warnings": [
                    "direct UI Automation ValuePattern read support is not enabled in this build; value-like snapshot properties were returned"
                ]
            })
        }
        Err(error) => error,
    }
}

pub fn uia_expand_collapse(
    state: &AppState,
    request: UiElementActionRequest,
) -> serde_json::Value {
    unsupported_pattern_action(state, request, "uia.expand_collapse", "expand_collapse")
}

pub fn uia_range_value(state: &AppState, request: UiRangeValueRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        value = request.value,
        "uia.range_value requested"
    );
    let action_request = UiElementActionRequest {
        bound_id: request.bound_id,
        element_ref: request.element_ref,
        selector: request.selector,
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.allow_offscreen,
    };
    unsupported_pattern_action(state, action_request, "uia.range_value", "range_value")
}

pub fn uia_scroll_into_view(
    state: &AppState,
    request: UiElementActionRequest,
) -> serde_json::Value {
    unsupported_pattern_action(state, request, "uia.scroll_into_view", "scroll_item")
}

pub fn uia_wait_for_element(
    state: &AppState,
    request: UiWaitForElementRequest,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        timeout_ms = ?request.timeout_ms,
        "uia.wait_for_element requested"
    );
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5_000));
    let poll_interval = Duration::from_millis(request.poll_interval_ms.unwrap_or(150).max(50));
    let started = Instant::now();
    let action_request = UiElementActionRequest {
        bound_id: request.bound_id.clone(),
        element_ref: request.element_ref.clone(),
        selector: request.selector.clone(),
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.require_visible == Some(false),
    };
    let mut last_error = None;
    loop {
        if started.elapsed() >= timeout {
            tracing::warn!(
                bound_id = %request.bound_id,
                timeout_ms = timeout.as_millis() as u64,
                "uia.wait_for_element timed out"
            );
            return serde_json::json!({
                "ok": false,
                "matched": false,
                "timeout": true,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64},
                "last_error": last_error,
                "error": {
                    "code": "uia_wait_for_element_timeout",
                    "message": "UI Automation element did not match before timeout"
                }
            });
        }
        match resolve_action_target(state, &action_request) {
            Ok(resolved) => {
                if wait_conditions_match(&resolved.element, &request) {
                    tracing::info!(
                        bound_id = %request.bound_id,
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "uia.wait_for_element matched"
                    );
                    return serde_json::json!({
                        "ok": true,
                        "matched": true,
                        "timeout": false,
                        "element": resolved.element,
                        "snapshot_summary": snapshot_summary(&resolved.snapshot),
                        "timing": {"elapsed_ms": started.elapsed().as_millis() as u64}
                    });
                }
                last_error = Some(serde_json::json!({
                    "code": "uia_wait_conditions_not_met",
                    "message": "element was found but did not satisfy requested state",
                    "element": resolved.element,
                }));
            }
            Err(error) => {
                last_error = Some(error.get("error").cloned().unwrap_or(error));
            }
        }
        thread::sleep(poll_interval);
    }
}

struct ResolvedActionTarget {
    snapshot: UiAutomationSnapshot,
    element: UiElementInfo,
    match_count: usize,
}

fn click_fallback_action(
    state: &AppState,
    request: UiElementActionRequest,
    tool_name: &'static str,
    pattern: &'static str,
    require_bounds: bool,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        tool_name = tool_name,
        "uia coordinate-fallback action requested"
    );
    let resolved = match resolve_action_target(state, &request) {
        Ok(resolved) => resolved,
        Err(error) => return error,
    };
    let fallback = coordinate_fallback_hint(&resolved.element);
    if let Err(error) = ensure_actionable(&resolved.element, request.allow_offscreen, require_bounds)
    {
        return unsupported_action(tool_name, "element_not_actionable", resolved, error);
    }
    let click = dispatch_center_click(state, &request.bound_id, &resolved.element);
    let after = resolve_action_target(state, &request)
        .ok()
        .map(|resolved| resolved.element);
    serde_json::json!({
        "ok": json_ok(&click),
        "pattern_used": format!("{pattern}_coordinate_fallback"),
        "direct_uia_pattern_used": false,
        "before": resolved.element,
        "after": after,
        "coordinate_fallback_hint": fallback,
        "dispatch": click,
        "warnings": [
            format!("direct UI Automation {pattern} pattern support is not enabled in this build; action used strict element revalidation plus center-click fallback")
        ]
    })
}

fn unsupported_pattern_action(
    state: &AppState,
    request: UiElementActionRequest,
    tool_name: &'static str,
    pattern: &'static str,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        tool_name = tool_name,
        pattern = pattern,
        "uia unsupported pattern action requested"
    );
    match resolve_action_target(state, &request) {
        Ok(resolved) => unsupported_action(
            tool_name,
            "uia_pattern_not_available",
            resolved,
            format!(
                "direct UI Automation {pattern} pattern support is not enabled and no safe fallback is available"
            ),
        ),
        Err(error) => error,
    }
}

fn unsupported_action(
    tool_name: &'static str,
    code: &'static str,
    resolved: ResolvedActionTarget,
    message: String,
) -> serde_json::Value {
    let fallback = coordinate_fallback_hint(&resolved.element);
    tracing::warn!(
        tool_name = tool_name,
        element_ref = %resolved.element.element_ref,
        match_count = resolved.match_count,
        code = code,
        "uia action failed closed"
    );
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        },
        "before": resolved.element,
        "snapshot_summary": snapshot_summary(&resolved.snapshot),
        "coordinate_fallback_hint": fallback,
        "pattern_used": serde_json::Value::Null,
        "direct_uia_pattern_used": false,
    })
}

fn resolve_action_target(
    state: &AppState,
    request: &UiElementActionRequest,
) -> Result<ResolvedActionTarget, serde_json::Value> {
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
    };
    let snapshot = match ui_automation_snapshot(
        &window,
        request.max_depth.unwrap_or(8),
        request.max_elements.unwrap_or(2_000),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
    };
    let element = if let Some(element_ref) = &request.element_ref {
        resolve_ui_element_ref(&snapshot, element_ref).cloned()
    } else if let Some(selector) = &request.selector {
        let matches = find_ui_elements(&snapshot, selector);
        match matches.len() {
            1 => Some(matches[0].clone()),
            0 => {
                return Err(serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "uia_element_not_found",
                        "message": "selector did not match any element in the current UI Automation snapshot"
                    },
                    "snapshot_summary": snapshot_summary(&snapshot),
                }));
            }
            count => {
                return Err(serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "uia_ambiguous_selector",
                        "message": "selector matched multiple elements; refine before acting",
                        "match_count": count,
                    },
                    "matches": matches,
                    "snapshot_summary": snapshot_summary(&snapshot),
                }));
            }
        }
    } else {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "uia_selector_required",
                "message": "element_ref or selector is required"
            }
        }));
    };
    let Some(element) = element else {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "uia_element_not_found",
                "message": "element reference was not found in the current UI Automation snapshot"
            },
            "snapshot_summary": snapshot_summary(&snapshot),
        }));
    };
    Ok(ResolvedActionTarget {
        snapshot,
        element,
        match_count: 1,
    })
}

fn ensure_actionable(
    element: &UiElementInfo,
    allow_offscreen: bool,
    require_bounds: bool,
) -> Result<(), String> {
    if element.enabled == Some(false) {
        return Err("element is disabled".into());
    }
    if element.offscreen == Some(true) && !allow_offscreen {
        return Err("element is offscreen".into());
    }
    if require_bounds {
        let Some(bounds) = &element.bounds else {
            return Err("element bounds are unavailable".into());
        };
        if bounds.width <= 0 || bounds.height <= 0 {
            return Err("element bounds are empty".into());
        }
    }
    Ok(())
}

fn dispatch_center_click(
    state: &AppState,
    bound_id: &str,
    element: &UiElementInfo,
) -> serde_json::Value {
    let Some((x, y)) = element_center(element.bounds.as_ref()) else {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "uia_element_bounds_unavailable",
                "message": "element has no usable bounds for coordinate fallback"
            }
        });
    };
    crate::tools::input::input_click(
        state,
        ClickRequest {
            bound_id: bound_id.into(),
            x,
            y,
            coordinate_space: CoordinateSpace::ScreenPixels,
            button: Some("left".into()),
            fail_if_outside_bound: Some(true),
        },
    )
}

fn coordinate_fallback_hint(element: &UiElementInfo) -> serde_json::Value {
    let center = element_center(element.bounds.as_ref()).map(|(x, y)| {
        serde_json::json!({
            "x": x,
            "y": y,
            "coordinate_space": "screen_pixels",
        })
    });
    serde_json::json!({
        "bounds": element.bounds,
        "center": center,
        "element_ref": element.element_ref,
    })
}

fn element_center(bounds: Option<&UiRect>) -> Option<(f64, f64)> {
    let bounds = bounds?;
    if bounds.width <= 0 || bounds.height <= 0 {
        return None;
    }
    Some((
        bounds.x as f64 + bounds.width as f64 / 2.0,
        bounds.y as f64 + bounds.height as f64 / 2.0,
    ))
}

fn snapshot_summary(snapshot: &UiAutomationSnapshot) -> serde_json::Value {
    serde_json::json!({
        "owner_window": snapshot.owner_window,
        "flattened_count": snapshot.flattened_count,
        "truncated": snapshot.truncated,
        "warnings": snapshot.warnings,
    })
}

fn wait_conditions_match(element: &UiElementInfo, request: &UiWaitForElementRequest) -> bool {
    if let Some(require_enabled) = request.require_enabled {
        if element.enabled != Some(require_enabled) {
            return false;
        }
    }
    if request.require_visible == Some(true) && element.offscreen == Some(true) {
        return false;
    }
    if let Some(name_contains) = &request.name_contains {
        let Some(name) = element.name.as_deref() else {
            return false;
        };
        if !name
            .to_ascii_lowercase()
            .contains(&name_contains.to_ascii_lowercase())
        {
            return false;
        }
    }
    true
}

fn json_ok(value: &serde_json::Value) -> bool {
    value
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}
