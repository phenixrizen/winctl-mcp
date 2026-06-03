use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::{GenericImageView, ImageBuffer, Rgba};
use regex::Regex;
use serde_json::{Map, Value};
use winctl::{
    find_ui_elements, flatten_ui_elements, ui_automation_snapshot, UiActionTarget,
    UiRect, UiResolvedElementState,
};

use crate::{
    AppState, AssertClipboardRequest, AssertElementRequest, AssertPixelColorRequest,
    AssertTextVisibleRequest, AssertWindowCountRequest, AssertionExpect,
    CaptureCompareBaselineRequest, CaptureOcrRegionRequest, CaptureReadTextRequest,
};

struct OcrOutput {
    provider: &'static str,
    text: String,
    words: Vec<serde_json::Value>,
}

struct AssertionAttempt {
    passed: bool,
    expected: Value,
    actual: Value,
    predicate: String,
    target: Value,
    diagnostics: Vec<Value>,
    extra: Map<String, Value>,
}

impl AssertionAttempt {
    fn new(
        passed: bool,
        predicate: impl Into<String>,
        expected: Value,
        actual: Value,
        target: Value,
    ) -> Self {
        Self {
            passed,
            expected,
            actual,
            predicate: predicate.into(),
            target,
            diagnostics: Vec::new(),
            extra: Map::new(),
        }
    }

    fn with_diagnostics(mut self, diagnostics: Vec<Value>) -> Self {
        self.diagnostics.extend(diagnostics);
        self
    }

    fn with_extra(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }
}

fn run_assertion<F>(
    negate: bool,
    timeout_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
    mut evaluate: F,
) -> serde_json::Value
where
    F: FnMut() -> Result<AssertionAttempt, serde_json::Value>,
{
    let started = Instant::now();
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(0));
    let poll_interval = Duration::from_millis(poll_interval_ms.unwrap_or(100).max(10));
    let mut attempts = 0u64;

    loop {
        attempts += 1;
        let mut attempt = match evaluate() {
            Ok(attempt) => attempt,
            Err(error) => return error,
        };
        let raw_passed = attempt.passed;
        let passed = if negate { !raw_passed } else { raw_passed };
        if passed || started.elapsed() >= timeout {
            attempt.passed = passed;
            return assertion_response(attempt, negate, attempts, started.elapsed());
        }
        thread::sleep(poll_interval);
    }
}

fn assertion_response(
    attempt: AssertionAttempt,
    negated: bool,
    attempts: u64,
    elapsed: Duration,
) -> serde_json::Value {
    let AssertionAttempt {
        passed,
        expected,
        actual,
        predicate,
        target,
        mut diagnostics,
        extra,
    } = attempt;
    if attempts > 1 {
        diagnostics.push(serde_json::json!({
            "kind": "polling",
            "attempts": attempts,
        }));
    }
    let mut response = Map::new();
    response.insert("ok".into(), Value::Bool(passed));
    response.insert("passed".into(), Value::Bool(passed));
    response.insert("negated".into(), Value::Bool(negated));
    response.insert("expected".into(), expected);
    response.insert("actual".into(), actual);
    response.insert("predicate".into(), Value::String(predicate));
    response.insert("target".into(), target);
    response.insert(
        "elapsed_ms".into(),
        Value::from(elapsed.as_millis() as u64),
    );
    response.insert("diagnostics".into(), Value::Array(diagnostics));
    if !passed {
        response.insert(
            "error".into(),
            serde_json::json!({
                "code": "assertion_failed",
                "message": "assertion predicate did not pass",
            }),
        );
    }
    response.extend(extra);
    Value::Object(response)
}

fn expect_absent(expect: Option<AssertionExpect>) -> bool {
    matches!(expect, Some(AssertionExpect::Absent))
}

fn regex_matches(actual: Option<&str>, regex: Option<&Regex>) -> bool {
    regex
        .map(|regex| actual.map(|actual| regex.is_match(actual)).unwrap_or(false))
        .unwrap_or(true)
}

fn compile_optional_regex(pattern: &Option<String>) -> Result<Option<Regex>, serde_json::Value> {
    pattern
        .as_deref()
        .map(|pattern| {
            Regex::new(pattern).map_err(|error| {
                serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "invalid_assertion_regex",
                        "message": format!("invalid regex {pattern:?}: {error}"),
                    }
                })
            })
        })
        .transpose()
}

fn role_matches(actual: Option<&str>, expected: &str) -> bool {
    actual
        .map(|actual| actual.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn bounds_within_tolerance(actual: Option<&UiRect>, expected: &UiRect, tolerance: i32) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let tolerance = tolerance.max(0);
    (actual.x - expected.x).abs() <= tolerance
        && (actual.y - expected.y).abs() <= tolerance
        && (actual.width - expected.width).abs() <= tolerance
        && (actual.height - expected.height).abs() <= tolerance
}

fn toggle_checked(state: Option<&str>) -> Option<bool> {
    match state {
        Some("on") => Some(true),
        Some("off") => Some(false),
        Some("indeterminate") => None,
        _ => None,
    }
}

fn expanded_state(state: Option<&str>) -> Option<bool> {
    match state {
        Some("expanded") | Some("partially_expanded") => Some(true),
        Some("collapsed") => Some(false),
        _ => None,
    }
}

fn needs_element_pattern_state(request: &AssertElementRequest) -> bool {
    request.checked.is_some()
        || request.selected.is_some()
        || request.expanded.is_some()
        || request.value.is_some()
        || request.value_contains.is_some()
        || request.value_regex.is_some()
        || request.editable.is_some()
        || request.readonly.is_some()
}

enum ResolvedElementStateError {
    ElementNotFound,
    Hard(serde_json::Value),
}

fn resolved_element_state(
    state: &AppState,
    bound_id: &str,
    element_ref: &Option<String>,
    selector: &Option<winctl::UiElementSelector>,
    max_depth: Option<usize>,
    max_elements: Option<usize>,
) -> Result<UiResolvedElementState, ResolvedElementStateError> {
    let window = state
        .revalidate_bound_window(bound_id)
        .map_err(|error| ResolvedElementStateError::Hard(serde_json::json!({"ok": false, "error": error})))?;
    winctl::ui_resolved_element_state(
        &window,
        &UiActionTarget {
            element_ref: element_ref.clone(),
            selector: selector.clone(),
            max_depth,
            max_elements,
            allow_offscreen: true,
        },
    )
    .map_err(|error| {
        if error.code == winctl::UiAutomationErrorCode::ElementNotFound {
            ResolvedElementStateError::ElementNotFound
        } else {
            ResolvedElementStateError::Hard(serde_json::json!({"ok": false, "error": error}))
        }
    })
}

pub fn assert_element(state: &AppState, request: AssertElementRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        "assert.element requested"
    );
    let name_regex = match compile_optional_regex(&request.name_regex) {
        Ok(regex) => regex,
        Err(error) => return error,
    };
    let value_regex = match compile_optional_regex(&request.value_regex) {
        Ok(regex) => regex,
        Err(error) => return error,
    };
    let absent = expect_absent(request.expect) || request.exists == Some(false);
    let use_resolved_state = needs_element_pattern_state(&request) && !absent;
    if use_resolved_state && request.count.is_some() {
        return fail(
            "uia_pattern_assertion_requires_single_element",
            "pattern-state predicates require a single resolved UIA element; use count in a separate assertion",
            None,
        );
    }
    run_assertion(
        request.negate,
        request.timeout_ms,
        request.poll_interval_ms,
        || {
            let mut diagnostics = Vec::new();
            let (matches, first_pattern_state, snapshot_summary_value) = if use_resolved_state {
                if request.element_ref.is_none() && request.selector.is_none() {
                    return Err(fail(
                        "uia_selector_required",
                        "element_ref or selector is required",
                        None,
                    ));
                }
                let resolved = match resolved_element_state(
                    state,
                    &request.bound_id,
                    &request.element_ref,
                    &request.selector,
                    request.max_depth,
                    request.max_elements,
                ) {
                    Ok(resolved) => resolved,
                    Err(ResolvedElementStateError::ElementNotFound) => {
                        return Ok(AssertionAttempt::new(
                            false,
                            "all",
                            serde_json::json!({
                                "expect": "present",
                                "exists": true,
                                "enabled": request.enabled,
                                "focused": request.focused,
                                "checked": request.checked,
                                "selected": request.selected,
                                "expanded": request.expanded,
                                "name": request.name,
                                "name_contains": request.name_contains,
                                "name_regex": request.name_regex,
                                "value": request.value,
                                "value_contains": request.value_contains,
                                "value_regex": request.value_regex,
                                "role": request.role,
                                "control_type_id": request.control_type_id,
                                "editable": request.editable,
                                "readonly": request.readonly,
                                "offscreen": request.offscreen,
                                "bounds": request.bounds,
                                "bounds_tolerance": request.bounds_tolerance,
                                "count": request.count,
                            }),
                            serde_json::json!({
                                "exists": false,
                                "match_count": 0,
                                "first": null,
                                "first_pattern_state": null,
                                "failures": ["expected present, found no matches"],
                            }),
                            serde_json::json!({
                                "kind": "uia_element",
                                "bound_id": request.bound_id,
                                "element_ref": request.element_ref,
                                "selector": request.selector,
                            }),
                        )
                        .with_extra("match_count", Value::from(0usize))
                        .with_extra("matches", serde_json::json!(Vec::<winctl::UiElementInfo>::new()))
                        .with_extra(
                            "failures",
                            serde_json::json!(["expected present, found no matches"]),
                        )
                        .with_extra("snapshot_summary", Value::Null));
                    }
                    Err(ResolvedElementStateError::Hard(error)) => return Err(error),
                };
                for warning in &resolved.pattern_state.warnings {
                    diagnostics.push(serde_json::json!({
                        "kind": "warning",
                        "message": warning,
                    }));
                }
                (vec![resolved.element], Some(resolved.pattern_state), Value::Null)
            } else {
                let snapshot = fresh_snapshot(
                    state,
                    &request.bound_id,
                    request.max_depth,
                    request.max_elements,
                )?;
                let matches = if let Some(element_ref) = &request.element_ref {
                    flatten_ui_elements(&snapshot.root)
                        .into_iter()
                        .filter(|element| element.element_ref == *element_ref)
                        .cloned()
                        .collect::<Vec<_>>()
                } else if let Some(selector) = &request.selector {
                    find_ui_elements(&snapshot, selector)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    return Err(fail(
                        "uia_selector_required",
                        "element_ref or selector is required",
                        None,
                    ));
                };
                (matches, None, snapshot_summary(&snapshot))
            };
            let exists = !matches.is_empty();
            let mut failures = Vec::new();
            if absent {
                if exists {
                    failures.push(format!("expected absent, found {} match(es)", matches.len()));
                }
            } else {
                if !exists {
                    failures.push("expected present, found no matches".to_owned());
                }
                let first = matches.first();
                if let (Some(expected), Some(element)) = (request.enabled, first) {
                    if element.enabled != Some(expected) {
                        failures.push(format!("enabled was {:?}", element.enabled));
                    }
                }
                if let (Some(expected), Some(element)) = (request.focused, first) {
                    if element.focused != Some(expected) {
                        failures.push(format!("focused was {:?}", element.focused));
                    }
                }
                if let (Some(expected), Some(element)) = (request.offscreen, first) {
                    if element.offscreen != Some(expected) {
                        failures.push(format!("offscreen was {:?}", element.offscreen));
                    }
                }
                if let (Some(expected), Some(element)) = (&request.name, first) {
                    if element.name.as_deref() != Some(expected.as_str()) {
                        failures.push(format!("name was {:?}", element.name));
                    }
                }
                if let (Some(needle), Some(element)) = (&request.name_contains, first) {
                    if !contains_ci(element.name.as_deref(), needle) {
                        failures.push(format!("name did not contain {needle:?}"));
                    }
                }
                if let (Some(pattern), Some(element)) = (&request.name_regex, first) {
                    if !regex_matches(element.name.as_deref(), name_regex.as_ref()) {
                        failures.push(format!("name did not match regex {pattern:?}"));
                    }
                }
                if let (Some(expected), Some(element)) = (&request.role, first) {
                    if !role_matches(element.role.as_deref(), expected) {
                        failures.push(format!("role was {:?}", element.role));
                    }
                }
                if let (Some(expected), Some(element)) = (request.control_type_id, first) {
                    if element.control_type_id != Some(expected) {
                        failures.push(format!(
                            "control_type_id was {:?}",
                            element.control_type_id
                        ));
                    }
                }
                if let (Some(expected), Some(element)) = (&request.bounds, first) {
                    let tolerance = request.bounds_tolerance.unwrap_or(0);
                    if !bounds_within_tolerance(element.bounds.as_ref(), expected, tolerance) {
                        failures.push(format!(
                            "bounds were {:?}, expected {:?} within tolerance {}",
                            element.bounds, expected, tolerance
                        ));
                    }
                }
                if let Some(expected) = request.checked {
                    let actual = first_pattern_state
                        .as_ref()
                        .and_then(|state| toggle_checked(state.toggle_state.as_deref()));
                    if actual != Some(expected) {
                        failures.push(format!("checked was {actual:?}"));
                    }
                }
                if let Some(expected) = request.selected {
                    let actual = first_pattern_state.as_ref().and_then(|state| state.selected);
                    if actual != Some(expected) {
                        failures.push(format!("selected was {actual:?}"));
                    }
                }
                if let Some(expected) = request.expanded {
                    let actual = first_pattern_state.as_ref().and_then(|state| {
                        expanded_state(state.expand_collapse_state.as_deref())
                    });
                    if actual != Some(expected) {
                        failures.push(format!("expanded was {actual:?}"));
                    }
                }
                if let Some(expected) = &request.value {
                    let actual = first_pattern_state
                        .as_ref()
                        .and_then(|state| state.value.as_deref());
                    if actual != Some(expected.as_str()) {
                        failures.push(format!("value was {actual:?}"));
                    }
                }
                if let Some(needle) = &request.value_contains {
                    let actual = first_pattern_state
                        .as_ref()
                        .and_then(|state| state.value.as_deref());
                    if !contains_ci(actual, needle) {
                        failures.push(format!("value did not contain {needle:?}"));
                    }
                }
                if let Some(pattern) = &request.value_regex {
                    let actual = first_pattern_state
                        .as_ref()
                        .and_then(|state| state.value.as_deref());
                    if !regex_matches(actual, value_regex.as_ref()) {
                        failures.push(format!("value did not match regex {pattern:?}"));
                    }
                }
                if let Some(expected) = request.readonly {
                    let actual = first_pattern_state
                        .as_ref()
                        .and_then(|state| state.value_readonly);
                    if actual != Some(expected) {
                        failures.push(format!("readonly was {actual:?}"));
                    }
                }
                if let Some(expected) = request.editable {
                    let actual = first_pattern_state
                        .as_ref()
                        .and_then(|state| state.value_readonly.map(|readonly| !readonly));
                    if actual != Some(expected) {
                        failures.push(format!("editable was {actual:?}"));
                    }
                }
            }
            if let Some(expected) = request.count {
                if matches.len() != expected {
                    failures.push(format!(
                        "count was {}, expected exactly {}",
                        matches.len(),
                        expected
                    ));
                }
            }
            let failures_json = failures
                .iter()
                .map(|failure| Value::String(failure.clone()))
                .collect::<Vec<_>>();
            let expected = serde_json::json!({
                "expect": if absent { "absent" } else { "present" },
                "exists": !absent,
                "enabled": request.enabled,
                "focused": request.focused,
                "checked": request.checked,
                "selected": request.selected,
                "expanded": request.expanded,
                "name": request.name,
                "name_contains": request.name_contains,
                "name_regex": request.name_regex,
                "value": request.value,
                "value_contains": request.value_contains,
                "value_regex": request.value_regex,
                "role": request.role,
                "control_type_id": request.control_type_id,
                "editable": request.editable,
                "readonly": request.readonly,
                "offscreen": request.offscreen,
                "bounds": request.bounds,
                "bounds_tolerance": request.bounds_tolerance,
                "count": request.count,
            });
            let actual = serde_json::json!({
                "exists": exists,
                "match_count": matches.len(),
                "first": matches.first(),
                "first_pattern_state": first_pattern_state,
                "failures": failures,
            });
            Ok(AssertionAttempt::new(
                failures_json.is_empty(),
                "all",
                expected,
                actual,
                serde_json::json!({
                    "kind": "uia_element",
                    "bound_id": request.bound_id,
                    "element_ref": request.element_ref,
                    "selector": request.selector,
                }),
            )
            .with_extra("match_count", Value::from(matches.len()))
            .with_extra("matches", serde_json::json!(matches))
            .with_extra("failures", Value::Array(failures_json))
            .with_extra("snapshot_summary", snapshot_summary_value)
            .with_diagnostics(diagnostics))
        },
    )
}

pub fn assert_text_visible(
    state: &AppState,
    request: AssertTextVisibleRequest,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        text = %request.text,
        "assert.text_visible requested"
    );
    run_assertion(
        request.negate,
        request.timeout_ms,
        request.poll_interval_ms,
        || {
            let window = match state.revalidate_bound_window(&request.bound_id) {
                Ok(window) => window,
                Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
            };
            let mut matches = Vec::new();
            if contains_ci(Some(&window.title), &request.text) {
                matches.push(serde_json::json!({"source": "window_title", "text": window.title}));
            }
            if contains_ci(Some(&window.class_name), &request.text) {
                matches
                    .push(serde_json::json!({"source": "window_class", "text": window.class_name}));
            }
            let mut snapshot_summary_value = Value::Null;
            let mut diagnostics = Vec::new();
            let mut warnings = Vec::<String>::new();
            match ui_automation_snapshot(
                &window,
                request.max_depth.unwrap_or(8),
                request.max_elements.unwrap_or(2_000),
            ) {
                Ok(snapshot) => {
                    for element in flatten_ui_elements(&snapshot.root) {
                        if contains_ci(element.name.as_deref(), &request.text)
                            || contains_ci(element.automation_id.as_deref(), &request.text)
                            || contains_ci(element.class_name.as_deref(), &request.text)
                        {
                            matches.push(serde_json::json!({
                                "source": "uia",
                                "element_ref": element.element_ref,
                                "name": element.name,
                                "automation_id": element.automation_id,
                                "class_name": element.class_name,
                            }));
                        }
                    }
                    snapshot_summary_value = snapshot_summary(&snapshot);
                }
                Err(error) => {
                    let warning = format!("UI Automation snapshot unavailable: {}", error.message);
                    warnings.push(warning.clone());
                    diagnostics.push(serde_json::json!({
                        "kind": "warning",
                        "message": warning,
                    }));
                }
            }
            let absent = expect_absent(request.expect);
            let raw_passed = if absent {
                matches.is_empty()
            } else {
                !matches.is_empty()
            };
            Ok(AssertionAttempt::new(
                raw_passed,
                "text_visible",
                serde_json::json!({
                    "expect": if absent { "absent" } else { "present" },
                    "text": request.text,
                }),
                serde_json::json!({
                    "visible": !matches.is_empty(),
                    "match_count": matches.len(),
                    "matches": matches,
                }),
                serde_json::json!({
                    "kind": "bound_window_text",
                    "bound_id": request.bound_id,
                }),
            )
            .with_extra("matches", serde_json::json!(matches))
            .with_extra("snapshot_summary", snapshot_summary_value)
            .with_extra("warnings", serde_json::json!(warnings))
            .with_diagnostics(diagnostics))
        },
    )
}

pub fn assert_pixel_color(state: &AppState, request: AssertPixelColorRequest) -> serde_json::Value {
    tracing::info!(
        image_path = ?request.image_path,
        bound_id = ?request.bound_id,
        x = request.x,
        y = request.y,
        expected_rgb = ?request.expected_rgb,
        "assert.pixel_color requested"
    );
    run_assertion(
        request.negate,
        request.timeout_ms,
        request.poll_interval_ms,
        || {
            let image_path =
                resolve_image_path(state, request.image_path.clone(), request.bound_id.clone())?;
            let image = match image::open(&image_path) {
                Ok(image) => image,
                Err(error) => {
                    return Err(fail(
                        "image_open_failed",
                        &format!("{error}"),
                        Some(image_path),
                    ))
                }
            };
            if request.x >= image.width() || request.y >= image.height() {
                return Err(fail(
                    "pixel_out_of_bounds",
                    "sample coordinate is outside the image",
                    Some(image_path),
                ));
            }
            let pixel = image.get_pixel(request.x, request.y).0;
            let actual = [pixel[0], pixel[1], pixel[2]];
            let tolerance = request.tolerance.unwrap_or(0);
            let passed = request
                .expected_rgb
                .map(|expected| rgb_within_tolerance(actual, expected, tolerance))
                .unwrap_or(true);
            Ok(AssertionAttempt::new(
                passed,
                "pixel_color",
                serde_json::json!({
                    "rgb": request.expected_rgb,
                    "tolerance": tolerance,
                }),
                serde_json::json!({
                    "rgb": actual,
                    "x": request.x,
                    "y": request.y,
                }),
                serde_json::json!({
                    "kind": "pixel",
                    "image_path": image_path,
                    "bound_id": request.bound_id,
                    "x": request.x,
                    "y": request.y,
                }),
            )
            .with_extra("image_path", serde_json::json!(image_path))
            .with_extra("x", Value::from(request.x))
            .with_extra("y", Value::from(request.y))
            .with_extra("actual_rgb", serde_json::json!(actual))
            .with_extra("expected_rgb", serde_json::json!(request.expected_rgb))
            .with_extra("tolerance", Value::from(tolerance)))
        },
    )
}

pub fn assert_window_count(request: AssertWindowCountRequest) -> serde_json::Value {
    tracing::info!(selector = ?request.selector, "assert.window_count requested");
    run_assertion(
        request.negate,
        request.timeout_ms,
        request.poll_interval_ms,
        || {
            let windows = winctl::list_windows();
            let matches = winctl::find_windows(&request.selector, &windows);
            let count = matches.len();
            let mut failures = Vec::new();
            if let Some(expected) = request.expected {
                if count != expected {
                    failures.push(format!("count {count} did not equal {expected}"));
                }
            }
            if let Some(min) = request.min {
                if count < min {
                    failures.push(format!("count {count} was less than {min}"));
                }
            }
            if let Some(max) = request.max {
                if count > max {
                    failures.push(format!("count {count} was greater than {max}"));
                }
            }
            if request.expected.is_none() && request.min.is_none() && request.max.is_none() {
                let absent = expect_absent(request.expect);
                if absent && count != 0 {
                    failures.push(format!("expected absent, found {count} match(es)"));
                } else if !absent && count == 0 {
                    failures.push("expected present, found no matches".to_owned());
                }
            }
            let failures_json = failures
                .iter()
                .map(|failure| Value::String(failure.clone()))
                .collect::<Vec<_>>();
            Ok(AssertionAttempt::new(
                failures_json.is_empty(),
                "window_count",
                serde_json::json!({
                    "expected": request.expected,
                    "min": request.min,
                    "max": request.max,
                    "expect": request.expect,
                }),
                serde_json::json!({"count": count}),
                serde_json::json!({
                    "kind": "window_selector",
                    "selector": request.selector,
                }),
            )
            .with_extra("count", Value::from(count))
            .with_extra("matches", serde_json::json!(matches))
            .with_extra("failures", Value::Array(failures_json)))
        },
    )
}

pub fn assert_clipboard(request: AssertClipboardRequest) -> serde_json::Value {
    tracing::info!("assert.clipboard requested");
    run_assertion(
        request.negate,
        request.timeout_ms,
        request.poll_interval_ms,
        || {
            let mut clipboard = match winctl::clipboard_read_text() {
                Ok(clipboard) => clipboard,
                Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
            };
            if let (Some(text), Some(max_chars)) = (clipboard.text.as_mut(), request.max_chars) {
                if text.chars().count() > max_chars {
                    *text = text.chars().take(max_chars).collect();
                }
            }
            let text = clipboard.text.clone().unwrap_or_default();
            let mut failures = Vec::new();
            if let Some(expected) = &request.expected {
                if &text != expected {
                    failures.push("clipboard text did not equal expected text".to_owned());
                }
            }
            if let Some(contains) = &request.contains {
                if !text.contains(contains) {
                    failures.push("clipboard text did not contain requested text".to_owned());
                }
            }
            let failures_json = failures
                .iter()
                .map(|failure| Value::String(failure.clone()))
                .collect::<Vec<_>>();
            Ok(AssertionAttempt::new(
                failures_json.is_empty(),
                "clipboard_text",
                serde_json::json!({
                    "expected": request.expected,
                    "contains": request.contains,
                    "max_chars": request.max_chars,
                }),
                serde_json::json!({
                    "text_present": clipboard.text.is_some(),
                    "char_count": text.chars().count(),
                }),
                serde_json::json!({"kind": "clipboard"}),
            )
            .with_extra("clipboard", serde_json::json!(clipboard))
            .with_extra("failures", Value::Array(failures_json)))
        },
    )
}

pub fn capture_ocr_region(state: &AppState, request: CaptureOcrRegionRequest) -> serde_json::Value {
    tracing::info!(
        image_path = ?request.image_path,
        bound_id = ?request.bound_id,
        "capture.ocr_region requested"
    );
    let (image_path, mapping) =
        match resolve_image_and_mapping(state, request.image_path, request.bound_id) {
            Ok(resolved) => resolved,
            Err(error) => return error,
        };
    let image = match image::open(&image_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => return fail("image_open_failed", &format!("{error}"), Some(image_path)),
    };
    let x = request.x.unwrap_or(0);
    let y = request.y.unwrap_or(0);
    if x >= image.width() || y >= image.height() {
        return fail(
            "ocr_region_out_of_bounds",
            "OCR region origin is outside the image",
            Some(image_path),
        );
    }
    let width = request
        .width
        .unwrap_or_else(|| image.width().saturating_sub(x));
    let height = request
        .height
        .unwrap_or_else(|| image.height().saturating_sub(y));
    if width == 0
        || height == 0
        || x.saturating_add(width) > image.width()
        || y.saturating_add(height) > image.height()
    {
        return fail(
            "ocr_region_invalid",
            "OCR region must fit inside the image and have non-zero size",
            Some(image_path),
        );
    }
    let crop = image::imageops::crop_imm(&image, x, y, width, height).to_image();
    let crop_path = state
        .capture_dir
        .join(format!("ocr-region-{}.png", now_unix_ms()));
    if let Err(error) = crop.save(&crop_path) {
        return fail(
            "ocr_region_save_failed",
            &format!("{error}"),
            Some(crop_path),
        );
    }
    let region = serde_json::json!({
        "x": x,
        "y": y,
        "width": width,
        "height": height,
    });
    // Word boxes from the providers are crop-local pixels. Translate them so a
    // caller can click them directly: when the image is a bound-window screenshot
    // (`mapping` present) words come back in `screen_pixels` with a ready `center`
    // point usable as `input.click` with `coordinate_space: "screen_pixels"`;
    // otherwise they stay in the source `image_pixels`.
    let coordinate_space = if mapping.is_some() {
        "screen_pixels"
    } else {
        "image_pixels"
    };

    #[cfg(windows)]
    let (provider_output, mut provider_warnings): (Option<OcrOutput>, Vec<serde_json::Value>) =
        match run_windows_media_ocr(&crop_path) {
            Ok(output) => (Some(output), Vec::new()),
            Err(error) => {
                tracing::warn!(error = %error, "Windows.Media.Ocr provider failed; falling back to tesseract");
                (
                    None,
                    vec![serde_json::json!({
                        "provider": "windows_media_ocr",
                        "error": error,
                    })],
                )
            }
        };
    #[cfg(not(windows))]
    let (provider_output, mut provider_warnings): (Option<OcrOutput>, Vec<serde_json::Value>) =
        (None, Vec::new());

    if let Some(output) = provider_output {
        let OcrOutput {
            provider,
            text,
            words,
        } = output;
        let words = translate_ocr_words(words, x, y, mapping.as_ref());
        return ocr_success(
            image_path,
            crop_path,
            region,
            coordinate_space,
            OcrOutput {
                provider,
                text,
                words,
            },
            provider_warnings,
        );
    }

    match run_tesseract_tsv(&crop_path) {
        Ok(tsv) => {
            let words = translate_ocr_words(parse_tesseract_tsv(&tsv), x, y, mapping.as_ref());
            let text = words
                .iter()
                .filter_map(|word| word.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            ocr_success(
                image_path,
                crop_path,
                region,
                coordinate_space,
                OcrOutput {
                    provider: "tesseract",
                    text,
                    words,
                },
                provider_warnings,
            )
        }
        Err(error) => {
            provider_warnings.push(serde_json::json!({
                "provider": "tesseract",
                "error": error,
            }));
            serde_json::json!({
                "ok": false,
                "provider_enabled": false,
                "provider": "none",
                "image_path": image_path,
                "crop_path": crop_path,
                "region": region,
                "error": {
                    "code": "ocr_provider_unavailable",
                    "message": "no OCR provider succeeded",
                    "providers": provider_warnings,
                },
            })
        }
    }
}

pub fn capture_read_text(state: &AppState, request: CaptureReadTextRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        "capture.read_text requested"
    );
    let snapshot = match fresh_snapshot(
        state,
        &request.bound_id,
        request.max_depth,
        request.max_elements,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return error,
    };
    let mut fragments = Vec::new();
    for element in flatten_ui_elements(&snapshot.root) {
        for (field, value) in [
            ("name", element.name.as_deref()),
            ("automation_id", element.automation_id.as_deref()),
            ("class_name", element.class_name.as_deref()),
        ] {
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                fragments.push(serde_json::json!({
                    "source": field,
                    "text": value,
                    "element_ref": element.element_ref,
                }));
            }
        }
    }
    let text = fragments
        .iter()
        .filter_map(|fragment| fragment.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::json!({
        "ok": true,
        "text": text,
        "fragments": fragments,
        "snapshot_summary": snapshot_summary(&snapshot),
    })
}

pub fn capture_compare_baseline(
    state: &AppState,
    request: CaptureCompareBaselineRequest,
) -> serde_json::Value {
    tracing::info!(
        actual_path = %request.actual_path,
        baseline_path = %request.baseline_path,
        tolerance = ?request.tolerance,
        "capture.compare_baseline requested"
    );
    let actual_path = PathBuf::from(&request.actual_path);
    let baseline_path = PathBuf::from(&request.baseline_path);
    let actual = match image::open(&actual_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            return fail(
                "actual_image_open_failed",
                &format!("{error}"),
                Some(actual_path),
            )
        }
    };
    let baseline = match image::open(&baseline_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            return fail(
                "baseline_image_open_failed",
                &format!("{error}"),
                Some(baseline_path),
            )
        }
    };
    if actual.dimensions() != baseline.dimensions() {
        return serde_json::json!({
            "ok": true,
            "passed": false,
            "error": {
                "code": "image_dimensions_differ",
                "message": "actual and baseline dimensions differ"
            },
            "actual_dimensions": {"width": actual.width(), "height": actual.height()},
            "baseline_dimensions": {"width": baseline.width(), "height": baseline.height()},
        });
    }
    let tolerance = request.tolerance.unwrap_or(0);
    let mut diff = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(actual.width(), actual.height());
    let mut different_pixels = 0u64;
    for (x, y, actual_pixel) in actual.enumerate_pixels() {
        let baseline_pixel = baseline.get_pixel(x, y);
        let actual_rgb = [actual_pixel[0], actual_pixel[1], actual_pixel[2]];
        let baseline_rgb = [baseline_pixel[0], baseline_pixel[1], baseline_pixel[2]];
        if rgb_within_tolerance(actual_rgb, baseline_rgb, tolerance) {
            diff.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        } else {
            different_pixels += 1;
            diff.put_pixel(x, y, Rgba([255, 0, 80, 255]));
        }
    }
    let max_different_pixels = request.max_different_pixels.unwrap_or(0);
    let passed = different_pixels <= max_different_pixels;
    let diff_path = request.diff_path.map(PathBuf::from).unwrap_or_else(|| {
        state
            .capture_dir
            .join(format!("baseline-diff-{}.png", now_unix_ms()))
    });
    let diff_save = diff.save(&diff_path).map(|_| diff_path.clone());
    serde_json::json!({
        "ok": true,
        "passed": passed,
        "actual_path": actual_path,
        "baseline_path": baseline_path,
        "diff_path": diff_save.ok(),
        "different_pixels": different_pixels,
        "max_different_pixels": max_different_pixels,
        "tolerance": tolerance,
        "dimensions": {"width": actual.width(), "height": actual.height()},
    })
}

fn fresh_snapshot(
    state: &AppState,
    bound_id: &str,
    max_depth: Option<usize>,
    max_elements: Option<usize>,
) -> Result<winctl::UiAutomationSnapshot, serde_json::Value> {
    let window = match state.revalidate_bound_window(bound_id) {
        Ok(window) => window,
        Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
    };
    ui_automation_snapshot(
        &window,
        max_depth.unwrap_or(8),
        max_elements.unwrap_or(2_000),
    )
    .map_err(|error| serde_json::json!({"ok": false, "error": error}))
}

/// Maps image (bitmap) pixels to virtual-desktop screen pixels for a bound-window
/// screenshot, so OCR boxes can be returned in a directly clickable coordinate space.
struct ScreenMapping {
    origin_x: f64,
    origin_y: f64,
    scale_x: f64,
    scale_y: f64,
}

/// Resolve the image to OCR and, when it comes from a bound-window screenshot, the
/// mapping from image pixels to screen pixels. A caller-supplied `image_path` has no
/// known screen origin, so it returns `None` and OCR boxes stay in image pixels.
fn resolve_image_and_mapping(
    state: &AppState,
    image_path: Option<String>,
    bound_id: Option<String>,
) -> Result<(PathBuf, Option<ScreenMapping>), serde_json::Value> {
    if let Some(path) = image_path {
        return Ok((PathBuf::from(path), None));
    }
    let Some(bound_id) = bound_id else {
        return Err(fail(
            "image_or_bound_id_required",
            "image_path or bound_id is required",
            None,
        ));
    };
    let capture = crate::tools::capture::screenshot_window(state, bound_id);
    if !capture
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err(capture);
    }
    let screenshot = capture.get("screenshot").cloned().unwrap_or_default();
    let path = screenshot
        .get("output_path")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| {
            fail(
                "screenshot_path_missing",
                "screenshot did not include output_path",
                None,
            )
        })?;
    let mapping = (|| {
        let region = screenshot.get("region_virtual_desktop")?;
        let region_x = region.get("x")?.as_i64()? as f64;
        let region_y = region.get("y")?.as_i64()? as f64;
        let region_w = region.get("width")?.as_i64()? as f64;
        let region_h = region.get("height")?.as_i64()? as f64;
        let image_w = screenshot.get("width")?.as_u64()? as f64;
        let image_h = screenshot.get("height")?.as_u64()? as f64;
        if image_w <= 0.0 || image_h <= 0.0 {
            return None;
        }
        Some(ScreenMapping {
            origin_x: region_x,
            origin_y: region_y,
            scale_x: region_w / image_w,
            scale_y: region_h / image_h,
        })
    })();
    Ok((path, mapping))
}

/// Translate provider word boxes (crop-local pixels) into image pixels (adding the
/// crop origin) and, when a `ScreenMapping` is present, into screen pixels. Adds a
/// `center` point per word for one-call `input.click` targeting.
fn translate_ocr_words(
    words: Vec<serde_json::Value>,
    crop_x: u32,
    crop_y: u32,
    mapping: Option<&ScreenMapping>,
) -> Vec<serde_json::Value> {
    words
        .into_iter()
        .map(|mut word| {
            let bounds = word.get("bounds").cloned().unwrap_or_default();
            let local = |key: &str| bounds.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as f64;
            let img_x = local("x") + crop_x as f64;
            let img_y = local("y") + crop_y as f64;
            let (bx, by, bw, bh) = match mapping {
                Some(m) => (
                    m.origin_x + img_x * m.scale_x,
                    m.origin_y + img_y * m.scale_y,
                    local("width") * m.scale_x,
                    local("height") * m.scale_y,
                ),
                None => (img_x, img_y, local("width"), local("height")),
            };
            word["bounds"] = serde_json::json!({
                "x": bx.round() as i64,
                "y": by.round() as i64,
                "width": bw.round() as i64,
                "height": bh.round() as i64,
            });
            word["center"] = serde_json::json!({
                "x": (bx + bw / 2.0).round() as i64,
                "y": (by + bh / 2.0).round() as i64,
            });
            word
        })
        .collect()
}

fn resolve_image_path(
    state: &AppState,
    image_path: Option<String>,
    bound_id: Option<String>,
) -> Result<PathBuf, serde_json::Value> {
    if let Some(path) = image_path {
        return Ok(PathBuf::from(path));
    }
    let Some(bound_id) = bound_id else {
        return Err(fail(
            "image_or_bound_id_required",
            "image_path or bound_id is required",
            None,
        ));
    };
    let capture = crate::tools::capture::screenshot_window(state, bound_id);
    if !capture
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err(capture);
    }
    capture
        .get("screenshot")
        .and_then(|screenshot| screenshot.get("output_path"))
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| {
            fail(
                "screenshot_path_missing",
                "screenshot did not include output_path",
                None,
            )
        })
}

fn snapshot_summary(snapshot: &winctl::UiAutomationSnapshot) -> serde_json::Value {
    serde_json::json!({
        "owner_window": snapshot.owner_window,
        "flattened_count": snapshot.flattened_count,
        "truncated": snapshot.truncated,
        "warnings": snapshot.warnings,
    })
}

fn rgb_within_tolerance(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected.iter())
        .all(|(actual, expected)| actual.abs_diff(*expected) <= tolerance)
}

fn contains_ci(actual: Option<&str>, expected: &str) -> bool {
    actual
        .map(|actual| {
            actual
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

fn ocr_success(
    image_path: PathBuf,
    crop_path: PathBuf,
    region: serde_json::Value,
    coordinate_space: &str,
    output: OcrOutput,
    provider_warnings: Vec<serde_json::Value>,
) -> serde_json::Value {
    let mut response = serde_json::json!({
        "ok": true,
        "provider_enabled": true,
        "provider": output.provider,
        "image_path": image_path,
        "crop_path": crop_path,
        "region": region,
        "coordinate_space": coordinate_space,
        "text": output.text,
        "words": output.words,
    });
    if !provider_warnings.is_empty() {
        response["provider_warnings"] = serde_json::Value::Array(provider_warnings);
    }
    response
}

#[cfg(windows)]
fn run_windows_media_ocr(image_path: &PathBuf) -> Result<OcrOutput, serde_json::Value> {
    run_windows_media_ocr_inner(image_path).map_err(|error| {
        serde_json::json!({
            "code": "windows_media_ocr_failed",
            "message": error,
        })
    })
}

#[cfg(windows)]
fn run_windows_media_ocr_inner(image_path: &PathBuf) -> Result<OcrOutput, String> {
    use windows::core::HSTRING;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::{FileAccessMode, Streams::FileRandomAccessStream};
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    let ro_initialized = match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(error = %error, "RoInitialize failed before Windows OCR; trying WinRT activation anyway");
            false
        }
    };
    let result = (|| {
        let max_dimension = OcrEngine::MaxImageDimension().map_err(|error| error.to_string())?;
        let image = image::open(image_path)
            .map_err(|error| format!("failed to inspect OCR image dimensions: {error}"))?;
        if image.width() > max_dimension || image.height() > max_dimension {
            return Err(format!(
                "OCR image {}x{} exceeds Windows.Media.Ocr max dimension {max_dimension}",
                image.width(),
                image.height()
            ));
        }

        let path = HSTRING::from(image_path.as_os_str().to_string_lossy().as_ref());
        let stream = FileRandomAccessStream::OpenAsync(&path, FileAccessMode::Read)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let bitmap = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let engine =
            OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| error.to_string())?;
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|error| error.to_string())?;
        let text = result
            .Text()
            .map_err(|error| error.to_string())?
            .to_string();
        let lines = result.Lines().map_err(|error| error.to_string())?;
        let mut words = Vec::new();
        for line_index in 0..lines.Size().map_err(|error| error.to_string())? {
            let line = lines.GetAt(line_index).map_err(|error| error.to_string())?;
            let line_words = line.Words().map_err(|error| error.to_string())?;
            for word_index in 0..line_words.Size().map_err(|error| error.to_string())? {
                let word = line_words
                    .GetAt(word_index)
                    .map_err(|error| error.to_string())?;
                let bounds = word.BoundingRect().map_err(|error| error.to_string())?;
                words.push(serde_json::json!({
                    "text": word.Text().map_err(|error| error.to_string())?.to_string(),
                    "confidence": serde_json::Value::Null,
                    "bounds": {
                        "x": bounds.X.max(0.0).round() as u32,
                        "y": bounds.Y.max(0.0).round() as u32,
                        "width": bounds.Width.max(0.0).round() as u32,
                        "height": bounds.Height.max(0.0).round() as u32,
                    },
                    "line": line_index + 1,
                    "word": word_index + 1,
                }));
            }
        }
        Ok(OcrOutput {
            provider: "windows_media_ocr",
            text,
            words,
        })
    })();
    if ro_initialized {
        unsafe { RoUninitialize() };
    }
    result
}

fn run_tesseract_tsv(image_path: &PathBuf) -> Result<String, serde_json::Value> {
    let mut command = Command::new("tesseract");
    command
        .arg(image_path)
        .arg("stdout")
        .arg("--psm")
        .arg("6")
        .arg("tsv")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        serde_json::json!({
            "code": "ocr_provider_unavailable",
            "message": format!("failed to start tesseract OCR provider: {error}"),
            "hint": "install Tesseract OCR or configure a future Windows.Media.Ocr provider",
        })
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let started = Instant::now();
    let timeout = Duration::from_secs(10);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                match child.wait() {
                    Ok(status) => break status,
                    Err(error) => {
                        return Err(serde_json::json!({
                            "code": "ocr_provider_wait_failed",
                            "message": format!("failed waiting for tesseract after timeout: {error}"),
                        }));
                    }
                }
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                return Err(serde_json::json!({
                    "code": "ocr_provider_wait_failed",
                    "message": format!("failed polling tesseract: {error}"),
                }));
            }
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    let stdout_text = String::from_utf8_lossy(&stdout).to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).to_string();
    if timed_out {
        return Err(serde_json::json!({
            "code": "ocr_provider_timeout",
            "message": "tesseract OCR timed out after 10 seconds",
            "stderr": stderr_text,
        }));
    }
    if !status.success() {
        return Err(serde_json::json!({
            "code": "ocr_provider_failed",
            "message": format!("tesseract exited with status {:?}", status.code()),
            "stderr": stderr_text,
        }));
    }
    Ok(stdout_text)
}

fn read_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

fn parse_tesseract_tsv(tsv: &str) -> Vec<serde_json::Value> {
    tsv.lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() < 12 {
                return None;
            }
            let text = fields[11..].join("\t").trim().to_owned();
            if text.is_empty() {
                return None;
            }
            let confidence = fields[10].parse::<f64>().ok();
            if confidence.map(|value| value < 0.0).unwrap_or(false) {
                return None;
            }
            Some(serde_json::json!({
                "text": text,
                "confidence": confidence,
                "bounds": {
                    "x": fields[6].parse::<u32>().unwrap_or_default(),
                    "y": fields[7].parse::<u32>().unwrap_or_default(),
                    "width": fields[8].parse::<u32>().unwrap_or_default(),
                    "height": fields[9].parse::<u32>().unwrap_or_default(),
                },
                "line": fields[4].parse::<u32>().ok(),
                "word": fields[5].parse::<u32>().ok(),
            }))
        })
        .collect()
}

fn fail(code: &str, message: &str, path: Option<PathBuf>) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        },
        "path": path,
    })
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tesseract_tsv_words() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t30\t40\t96.5\tHello\n5\t1\t1\t1\t1\t2\t44\t20\t20\t40\t90\tworld\n";

        let words = parse_tesseract_tsv(tsv);

        assert_eq!(words.len(), 2);
        assert_eq!(words[0]["text"], "Hello");
        assert_eq!(words[0]["bounds"]["x"], 10);
        assert_eq!(words[1]["text"], "world");
    }
}
