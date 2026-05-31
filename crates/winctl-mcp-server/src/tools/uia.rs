use std::thread;
use std::time::{Duration, Instant};

use crate::{
    AppState, UiElementActionRequest, UiFindRequest, UiRangeValueRequest, UiResolveRequest,
    UiSetValueRequest, UiSnapshotRequest, UiWaitForElementRequest,
};
use winctl::{
    find_ui_elements, resolve_ui_element_ref, ui_automation_snapshot, ui_expand_collapse_pattern,
    ui_get_value_pattern, ui_invoke_pattern, ui_range_value_pattern, ui_scroll_into_view_pattern,
    ui_select_pattern, ui_set_focus_pattern, ui_set_value_pattern, ui_toggle_pattern,
    UiActionTarget, UiAutomationError, UiAutomationErrorCode, UiAutomationSnapshot, UiElementInfo,
    UiExpandCollapseAction, UiRect, UiSelectionMode,
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
    direct_pattern_action(state, request, "uia.invoke", |window, target| {
        ui_invoke_pattern(window, target)
    })
}

pub fn uia_set_focus(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    direct_pattern_action(state, request, "uia.set_focus", |window, target| {
        ui_set_focus_pattern(window, target)
    })
}

pub fn uia_select(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    let mode = request.mode.unwrap_or(UiSelectionMode::Replace);
    direct_pattern_action(state, request, "uia.select", |window, target| {
        ui_select_pattern(window, target, mode)
    })
}

pub fn uia_toggle(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    let desired_state = request.desired_state;
    direct_pattern_action(state, request, "uia.toggle", |window, target| {
        ui_toggle_pattern(window, target, desired_state)
    })
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
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let action_request = set_value_action_request(&request);
    let target = action_target(&action_request);
    match ui_set_value_pattern(&window, &target, &request.value) {
        Ok(outcome) => serde_json::json!({"ok": true, "outcome": outcome}),
        Err(error) => uia_action_error(state, &action_request, "uia.set_value", error),
    }
}

pub fn uia_get_value(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        "uia.get_value requested"
    );
    direct_pattern_action(state, request, "uia.get_value", |window, target| {
        ui_get_value_pattern(window, target)
    })
}

pub fn uia_expand_collapse(state: &AppState, request: UiElementActionRequest) -> serde_json::Value {
    let action = request
        .expand_collapse_action
        .unwrap_or(UiExpandCollapseAction::Toggle);
    direct_pattern_action(
        state,
        request,
        "uia.expand_collapse",
        move |window, target| ui_expand_collapse_pattern(window, target, action),
    )
}

pub fn uia_range_value(state: &AppState, request: UiRangeValueRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        value = request.value,
        "uia.range_value requested"
    );
    let value = request.value;
    let action_request = range_value_action_request(request);
    direct_pattern_action(
        state,
        action_request,
        "uia.range_value",
        move |window, target| ui_range_value_pattern(window, target, value),
    )
}

pub fn uia_scroll_into_view(
    state: &AppState,
    request: UiElementActionRequest,
) -> serde_json::Value {
    direct_pattern_action(state, request, "uia.scroll_into_view", |window, target| {
        ui_scroll_into_view_pattern(window, target)
    })
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
        expand_collapse_action: None,
        desired_state: None,
        mode: None,
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
}

fn direct_pattern_action<F>(
    state: &AppState,
    request: UiElementActionRequest,
    tool_name: &'static str,
    action: F,
) -> serde_json::Value
where
    F: FnOnce(
        &winctl::WindowInfo,
        &UiActionTarget,
    ) -> Result<winctl::UiActionOutcome, UiAutomationError>,
{
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        desired_state = ?request.desired_state,
        mode = ?request.mode,
        tool_name = tool_name,
        "uia direct pattern action requested"
    );
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let target = action_target(&request);
    match action(&window, &target) {
        Ok(outcome) => serde_json::json!({"ok": true, "outcome": outcome}),
        Err(error) => uia_action_error(state, &request, tool_name, error),
    }
}

fn action_target(request: &UiElementActionRequest) -> UiActionTarget {
    UiActionTarget {
        element_ref: request.element_ref.clone(),
        selector: request.selector.clone(),
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.allow_offscreen,
    }
}

fn set_value_action_request(request: &UiSetValueRequest) -> UiElementActionRequest {
    UiElementActionRequest {
        bound_id: request.bound_id.clone(),
        element_ref: request.element_ref.clone(),
        selector: request.selector.clone(),
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.allow_offscreen,
        expand_collapse_action: None,
        desired_state: None,
        mode: None,
    }
}

fn range_value_action_request(request: UiRangeValueRequest) -> UiElementActionRequest {
    UiElementActionRequest {
        bound_id: request.bound_id,
        element_ref: request.element_ref,
        selector: request.selector,
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.allow_offscreen,
        expand_collapse_action: None,
        desired_state: None,
        mode: None,
    }
}

fn uia_action_error(
    state: &AppState,
    request: &UiElementActionRequest,
    tool_name: &'static str,
    error: UiAutomationError,
) -> serde_json::Value {
    let resolved = resolve_action_target(state, request).ok();
    tracing::warn!(
        tool_name = tool_name,
        error_code = ?error.code,
        "uia direct pattern action failed closed"
    );
    let fallback_hint = resolved
        .as_ref()
        .map(|resolved| coordinate_fallback_hint(&resolved.element));
    let warnings = if error.code == UiAutomationErrorCode::PatternUnavailable {
        vec!["coordinate fallback was not dispatched; use explicit input tools if a coordinate fallback is intended"]
    } else {
        Vec::new()
    };
    serde_json::json!({
        "ok": false,
        "error": error,
        "before": resolved.as_ref().map(|resolved| resolved.element.clone()),
        "snapshot_summary": resolved.as_ref().map(|resolved| snapshot_summary(&resolved.snapshot)),
        "coordinate_fallback_hint": fallback_hint,
        "pattern_used": serde_json::Value::Null,
        "direct_uia_pattern_used": false,
        "warnings": warnings,
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
    Ok(ResolvedActionTarget { snapshot, element })
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
