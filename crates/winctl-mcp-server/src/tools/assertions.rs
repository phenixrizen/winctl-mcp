use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use image::{GenericImageView, ImageBuffer, Rgba};
use winctl::{find_ui_elements, flatten_ui_elements, ui_automation_snapshot};

use crate::{
    AppState, AssertClipboardRequest, AssertElementRequest, AssertPixelColorRequest,
    AssertTextVisibleRequest, AssertWindowCountRequest, CaptureCompareBaselineRequest,
    CaptureOcrRegionRequest, CaptureReadTextRequest,
};

pub fn assert_element(state: &AppState, request: AssertElementRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        "assert.element requested"
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
    let matches = if let Some(element_ref) = &request.element_ref {
        flatten_ui_elements(&snapshot.root)
            .into_iter()
            .filter(|element| element.element_ref == *element_ref)
            .collect::<Vec<_>>()
    } else if let Some(selector) = &request.selector {
        find_ui_elements(&snapshot, selector)
    } else {
        return fail(
            "uia_selector_required",
            "element_ref or selector is required",
            None,
        );
    };
    let exists = !matches.is_empty();
    let mut failures = Vec::new();
    if request.exists.unwrap_or(true) != exists {
        failures.push(format!("exists was {exists}"));
    }
    let first = matches.first().copied();
    if let (Some(expected), Some(element)) = (request.enabled, first) {
        if element.enabled != Some(expected) {
            failures.push(format!("enabled was {:?}", element.enabled));
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
    let passed = failures.is_empty();
    serde_json::json!({
        "ok": true,
        "passed": passed,
        "match_count": matches.len(),
        "matches": matches,
        "failures": failures,
        "snapshot_summary": snapshot_summary(&snapshot),
    })
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
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let mut matches = Vec::new();
    if contains_ci(Some(&window.title), &request.text) {
        matches.push(serde_json::json!({"source": "window_title", "text": window.title}));
    }
    if contains_ci(Some(&window.class_name), &request.text) {
        matches.push(serde_json::json!({"source": "window_class", "text": window.class_name}));
    }
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
            serde_json::json!({
                "ok": true,
                "passed": !matches.is_empty(),
                "matches": matches,
                "snapshot_summary": snapshot_summary(&snapshot),
            })
        }
        Err(error) => serde_json::json!({
            "ok": true,
            "passed": !matches.is_empty(),
            "matches": matches,
            "warnings": [format!("UI Automation snapshot unavailable: {}", error.message)],
        }),
    }
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
    let image_path = match resolve_image_path(state, request.image_path, request.bound_id) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let image = match image::open(&image_path) {
        Ok(image) => image,
        Err(error) => return fail("image_open_failed", &format!("{error}"), Some(image_path)),
    };
    if request.x >= image.width() || request.y >= image.height() {
        return fail(
            "pixel_out_of_bounds",
            "sample coordinate is outside the image",
            Some(image_path),
        );
    }
    let pixel = image.get_pixel(request.x, request.y).0;
    let actual = [pixel[0], pixel[1], pixel[2]];
    let tolerance = request.tolerance.unwrap_or(0);
    let passed = request
        .expected_rgb
        .map(|expected| rgb_within_tolerance(actual, expected, tolerance))
        .unwrap_or(true);
    serde_json::json!({
        "ok": true,
        "passed": passed,
        "image_path": image_path,
        "x": request.x,
        "y": request.y,
        "actual_rgb": actual,
        "expected_rgb": request.expected_rgb,
        "tolerance": tolerance,
    })
}

pub fn assert_window_count(request: AssertWindowCountRequest) -> serde_json::Value {
    tracing::info!(selector = ?request.selector, "assert.window_count requested");
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
    serde_json::json!({
        "ok": true,
        "passed": failures.is_empty(),
        "count": count,
        "matches": matches,
        "failures": failures,
    })
}

pub fn assert_clipboard(request: AssertClipboardRequest) -> serde_json::Value {
    tracing::info!("assert.clipboard requested");
    let mut clipboard = match winctl::clipboard_read_text() {
        Ok(clipboard) => clipboard,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
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
    serde_json::json!({
        "ok": true,
        "passed": failures.is_empty(),
        "clipboard": clipboard,
        "failures": failures,
    })
}

pub fn capture_ocr_region(state: &AppState, request: CaptureOcrRegionRequest) -> serde_json::Value {
    tracing::info!(
        image_path = ?request.image_path,
        bound_id = ?request.bound_id,
        "capture.ocr_region requested"
    );
    let image_path = match resolve_image_path(state, request.image_path, request.bound_id) {
        Ok(path) => path,
        Err(error) => return error,
    };
    serde_json::json!({
        "ok": false,
        "provider_enabled": false,
        "image_path": image_path,
        "region": {
            "x": request.x,
            "y": request.y,
            "width": request.width,
            "height": request.height,
        },
        "error": {
            "code": "ocr_provider_unavailable",
            "message": "OCR provider integration is not enabled in this build"
        }
    })
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
