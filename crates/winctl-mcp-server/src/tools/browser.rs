use std::thread;
use std::time::{Duration, Instant};

use crate::{
    AppState, BrowserAssertRequest, BrowserDescribeRequest, BrowserExtractContentRequest,
    BrowserListRequest, BrowserWaitForNavigationRequest,
};
use winctl::{
    browser_session_identity, browser_state, process_to_browser_process, window_to_browser_window,
    BrowserKind, BrowserStateFilter,
};

pub fn browser_list(state: &AppState, request: BrowserListRequest) -> serde_json::Value {
    tracing::info!(
        browser = ?request.browser,
        pid = ?request.pid,
        include_windows = request.include_windows,
        only_mcp_launched = request.only_mcp_launched,
        "browser.list requested"
    );
    let mut browser_state = browser_state(BrowserStateFilter {
        browser: request.browser,
        pid: request.pid,
        include_windows: request.include_windows,
    });
    if request.only_mcp_launched {
        browser_state
            .processes
            .retain(|process| state.tracked_by_pid(process.pid).is_some());
        browser_state
            .windows
            .retain(|window| state.tracked_by_pid(window.pid).is_some());
    }
    for process in &mut browser_state.processes {
        if let Some(tracked) = state.tracked_by_pid(process.pid) {
            process.tracked_by_server = true;
            process.launch_id = Some(tracked.launch_id);
        }
    }
    for window in &mut browser_state.windows {
        if let Some(tracked) = state.tracked_by_pid(window.pid) {
            window.tracked_by_server = true;
            window.launch_id = Some(tracked.launch_id);
        }
    }
    serde_json::json!({"ok": true, "browser_state": browser_state})
}

pub fn browser_describe(state: &AppState, request: BrowserDescribeRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = ?request.bound_id,
        pid = ?request.pid,
        hwnd = ?request.hwnd,
        "browser.describe requested"
    );
    if let Some(bound_id) = request.bound_id {
        return match state.revalidate_bound_window(&bound_id) {
            Ok(window) => browser_window_response(state, window),
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        };
    }
    let windows = winctl::list_windows();
    if let Some(hwnd) = request.hwnd {
        let Some(window) = windows
            .into_iter()
            .find(|window| window.hwnd_hex.eq_ignore_ascii_case(&hwnd))
        else {
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "browser_window_not_found",
                    "message": format!("browser HWND {hwnd} was not found")
                }
            });
        };
        return browser_window_response(state, window);
    }
    if let Some(pid) = request.pid {
        let process = winctl::describe_process(pid).ok().flatten();
        let browser_process = process.and_then(|process| {
            process_to_browser_process(
                process,
                state.tracked_by_pid(pid).is_some(),
                state.tracked_by_pid(pid).map(|tracked| tracked.launch_id),
            )
        });
        let browser_windows: Vec<_> = windows
            .into_iter()
            .filter(|window| window.pid == pid)
            .filter_map(|window| {
                window_to_browser_window(
                    window,
                    state.tracked_by_pid(pid).is_some(),
                    state.tracked_by_pid(pid).map(|tracked| tracked.launch_id),
                )
            })
            .collect();
        return serde_json::json!({
            "ok": browser_process.is_some() || !browser_windows.is_empty(),
            "process": browser_process,
            "windows": browser_windows,
        });
    }
    serde_json::json!({
        "ok": false,
        "error": {
            "code": "missing_browser_target",
            "message": "bound_id, pid, or hwnd is required"
        }
    })
}

pub fn browser_wait_for_navigation(
    state: &AppState,
    request: BrowserWaitForNavigationRequest,
) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        title_contains = ?request.title_contains,
        title_not_contains = ?request.title_not_contains,
        timeout_ms = ?request.timeout_ms,
        "browser.wait_for_navigation requested"
    );
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(10_000));
    let poll_interval = Duration::from_millis(request.poll_interval_ms.unwrap_or(100).max(25));
    let started = Instant::now();
    loop {
        let window = match state.revalidate_bound_window(&request.bound_id) {
            Ok(window) => window,
            Err(error) => return serde_json::json!({"ok": false, "error": error}),
        };
        let Some(identity) = browser_session_identity(&window) else {
            return not_browser_window();
        };
        let contains_ok = request
            .title_contains
            .as_ref()
            .map(|needle| window.title.to_lowercase().contains(&needle.to_lowercase()))
            .unwrap_or(true);
        let not_contains_ok = request
            .title_not_contains
            .as_ref()
            .map(|needle| !window.title.to_lowercase().contains(&needle.to_lowercase()))
            .unwrap_or(true);
        if contains_ok && not_contains_ok {
            return serde_json::json!({
                "ok": true,
                "matched": true,
                "timeout": false,
                "identity": identity,
                "window": window,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64}
            });
        }
        if started.elapsed() >= timeout {
            return serde_json::json!({
                "ok": false,
                "matched": false,
                "timeout": true,
                "identity": identity,
                "window": window,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64},
                "error": {
                    "code": "browser_navigation_timeout",
                    "message": "browser window did not satisfy title transition before timeout"
                }
            });
        }
        thread::sleep(poll_interval);
    }
}

pub fn browser_assert(state: &AppState, request: BrowserAssertRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        browser = ?request.browser,
        title_contains = ?request.title_contains,
        class_name_contains = ?request.class_name_contains,
        "browser.assert requested"
    );
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let Some(identity) = browser_session_identity(&window) else {
        return not_browser_window();
    };
    let mut failures = Vec::new();
    if let Some(browser) = request.browser {
        if identity.browser != browser {
            failures.push(format!(
                "expected browser {:?}, got {:?}",
                browser, identity.browser
            ));
        }
    }
    if let Some(needle) = &request.title_contains {
        if !window.title.to_lowercase().contains(&needle.to_lowercase()) {
            failures.push(format!("title did not contain {needle}"));
        }
    }
    if let Some(needle) = &request.class_name_contains {
        if !window
            .class_name
            .to_lowercase()
            .contains(&needle.to_lowercase())
        {
            failures.push(format!("class name did not contain {needle}"));
        }
    }
    serde_json::json!({
        "ok": failures.is_empty(),
        "asserted": failures.is_empty(),
        "identity": identity,
        "window": window,
        "failures": failures,
    })
}

pub fn browser_extract_content(
    state: &AppState,
    request: BrowserExtractContentRequest,
) -> serde_json::Value {
    tracing::info!(bound_id = %request.bound_id, "browser.extract_content requested");
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let Some(identity) = browser_session_identity(&window) else {
        return not_browser_window();
    };
    serde_json::json!({
        "ok": true,
        "identity": identity,
        "content": {
            "title": window.title,
            "class_name": window.class_name,
            "dom_extraction": {
                "available": false,
                "reason": "browser DOM extraction requires explicit browser debugging/session integration"
            }
        },
        "warnings": ["returned window-level content hints only; DOM extraction is not enabled"]
    })
}

pub fn browser_screenshot_checkpoint(
    state: &AppState,
    request: BrowserExtractContentRequest,
) -> serde_json::Value {
    tracing::info!(bound_id = %request.bound_id, "browser.screenshot_checkpoint requested");
    let window = match state.revalidate_bound_window(&request.bound_id) {
        Ok(window) => window,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let Some(identity) = browser_session_identity(&window) else {
        return not_browser_window();
    };
    let capture = crate::tools::capture::screenshot_window(state, request.bound_id);
    serde_json::json!({
        "ok": capture.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "identity": identity,
        "checkpoint": capture,
    })
}

fn browser_window_response(state: &AppState, window: winctl::WindowInfo) -> serde_json::Value {
    let tracked = state.tracked_by_pid(window.pid);
    match window_to_browser_window(
        window,
        tracked.is_some(),
        tracked.as_ref().map(|tracked| tracked.launch_id.clone()),
    ) {
        Some(browser_window) => serde_json::json!({"ok": true, "window": browser_window}),
        None => not_browser_window(),
    }
}

fn not_browser_window() -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": "not_browser_window",
            "message": "target is not a recognized Chrome, Edge, or Firefox browser window"
        }
    })
}

#[allow(dead_code)]
fn browser_kind_name(kind: &BrowserKind) -> &'static str {
    match kind {
        BrowserKind::Chrome => "chrome",
        BrowserKind::Edge => "edge",
        BrowserKind::Firefox => "firefox",
        BrowserKind::Unknown => "unknown",
    }
}
