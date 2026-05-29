use crate::{AppState, UiFindRequest, UiResolveRequest, UiSnapshotRequest};
use winctl::{find_ui_elements, resolve_ui_element_ref, ui_automation_snapshot};

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
