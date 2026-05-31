use crate::{AppState, DialogInvokeButtonRequest, DialogListRequest};
use winctl::{
    find_ui_elements, list_windows, resolve_ui_element_ref, ui_automation_snapshot,
    ui_invoke_pattern, UiActionTarget, UiAutomationError, UiElementInfo, UiElementSelector, UiRect,
    WindowInfo,
};

pub fn dialogs_list(_state: &AppState, request: DialogListRequest) -> serde_json::Value {
    tracing::info!(
        max_depth = request.max_depth,
        max_elements = request.max_elements,
        include_non_foreground = request.include_non_foreground,
        include_non_dialog_foreground = request.include_non_dialog_foreground,
        "dialogs.list requested"
    );
    let secure_desktop = secure_desktop_status();
    if secure_desktop
        .get("active")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return serde_json::json!({
            "ok": true,
            "provider_enabled": true,
            "secure_desktop": secure_desktop,
            "foreground": serde_json::Value::Null,
            "dialogs": [],
            "warnings": [
                "the input desktop appears to be a secure desktop; UAC consent prompts cannot be automated by winctl-mcp"
            ],
        });
    }

    let windows = list_windows();
    let foreground = windows.iter().find(|window| window.foreground).cloned();
    let mut warnings = Vec::new();
    let mut dialogs = Vec::new();
    for window in windows {
        let dialog_like = is_dialog_like(&window);
        let include = if window.foreground {
            dialog_like || request.include_non_dialog_foreground
        } else {
            request.include_non_foreground && dialog_like
        };
        if include {
            dialogs.push(dialog_json(
                &window,
                request.max_depth.unwrap_or(8),
                request.max_elements.unwrap_or(1_000),
                &mut warnings,
            ));
        }
    }

    serde_json::json!({
        "ok": true,
        "provider_enabled": true,
        "secure_desktop": secure_desktop,
        "foreground": foreground.map(|window| window_summary(&window)),
        "dialogs": dialogs,
        "warnings": warnings,
    })
}

pub fn dialogs_invoke_button(
    _state: &AppState,
    request: DialogInvokeButtonRequest,
) -> serde_json::Value {
    tracing::info!(
        hwnd = %request.hwnd,
        pid = request.pid,
        button_name = ?request.button_name,
        element_ref = ?request.element_ref,
        selector = ?request.selector,
        "dialogs.invoke_button requested"
    );
    let secure_desktop = secure_desktop_status();
    if secure_desktop
        .get("active")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return serde_json::json!({
            "ok": false,
            "secure_desktop": secure_desktop,
            "error": {
                "code": "uac_secure_desktop_not_automatable",
                "message": "a secure desktop appears to be active; UAC consent prompts cannot be automated by winctl-mcp"
            }
        });
    }

    let window = match resolve_dialog_window(&request) {
        Ok(window) => window,
        Err(error) => return error,
    };
    if is_uac_prompt(&window) {
        return serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(&window),
            "error": {
                "code": "uac_prompt_not_automatable",
                "message": "UAC consent prompts are not automated; approve or deny them manually on the secure desktop"
            }
        });
    }

    let target = action_target(&request);
    let resolved = match resolve_button_target(&window, &request) {
        Ok(element) => element,
        Err(error) => return error,
    };
    match ui_invoke_pattern(&window, &target) {
        Ok(outcome) => serde_json::json!({
            "ok": true,
            "dialog": dialog_identity(&window),
            "button": button_json(&resolved),
            "outcome": outcome,
        }),
        Err(error) => dialog_action_error(&window, Some(&resolved), error),
    }
}

fn resolve_dialog_window(
    request: &DialogInvokeButtonRequest,
) -> Result<WindowInfo, serde_json::Value> {
    let Some(window) = list_windows().into_iter().find(|window| {
        window.pid == request.pid && window.hwnd_hex.eq_ignore_ascii_case(&request.hwnd)
    }) else {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "dialog_not_found",
                "message": "no current top-level window matched the requested HWND and PID"
            },
            "requested": {
                "hwnd": request.hwnd,
                "pid": request.pid
            }
        }));
    };
    if !window.foreground {
        return Err(serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(&window),
            "error": {
                "code": "dialog_not_foreground",
                "message": "dialogs.invoke_button only acts on the current foreground dialog"
            }
        }));
    }
    if !request.allow_non_dialog && !is_dialog_like(&window) {
        return Err(serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(&window),
            "error": {
                "code": "foreground_window_not_dialog",
                "message": "the requested foreground window is not recognized as a native dialog; set allow_non_dialog only for explicit diagnostics"
            }
        }));
    }
    Ok(window)
}

fn resolve_button_target(
    window: &WindowInfo,
    request: &DialogInvokeButtonRequest,
) -> Result<UiElementInfo, serde_json::Value> {
    let snapshot = match ui_automation_snapshot(
        window,
        request.max_depth.unwrap_or(8),
        request.max_elements.unwrap_or(1_000),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return Err(serde_json::json!({"ok": false, "error": error})),
    };
    let element = if let Some(element_ref) = &request.element_ref {
        resolve_ui_element_ref(&snapshot, element_ref).cloned()
    } else if let Some(selector) = effective_selector(request) {
        let matches = find_ui_elements(&snapshot, &selector);
        match matches.len() {
            1 => Some(matches[0].clone()),
            0 => {
                return Err(serde_json::json!({
                    "ok": false,
                    "dialog": dialog_identity(window),
                    "error": {
                        "code": "dialog_button_not_found",
                        "message": "no dialog button matched the requested selector"
                    },
                    "snapshot_summary": snapshot_summary(snapshot.flattened_count, snapshot.truncated, snapshot.warnings),
                }));
            }
            count => {
                return Err(serde_json::json!({
                    "ok": false,
                    "dialog": dialog_identity(window),
                    "error": {
                        "code": "dialog_button_ambiguous",
                        "message": "selector matched multiple dialog elements; refine before acting",
                        "match_count": count,
                    },
                    "matches": matches.into_iter().map(|element| button_json(&element)).collect::<Vec<_>>(),
                    "snapshot_summary": snapshot_summary(snapshot.flattened_count, snapshot.truncated, snapshot.warnings),
                }));
            }
        }
    } else {
        return Err(serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(window),
            "error": {
                "code": "dialog_button_selector_required",
                "message": "element_ref, selector, or button_name is required"
            }
        }));
    };

    let Some(element) = element else {
        return Err(serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(window),
            "error": {
                "code": "dialog_button_not_found",
                "message": "element_ref was not found in the current dialog UI Automation snapshot"
            },
            "snapshot_summary": snapshot_summary(snapshot.flattened_count, snapshot.truncated, snapshot.warnings),
        }));
    };
    if !is_button_element(&element) {
        return Err(serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(window),
            "element": element,
            "error": {
                "code": "dialog_target_not_button",
                "message": "dialogs.invoke_button only invokes Button or SplitButton UI Automation elements"
            }
        }));
    }
    if !request.allow_offscreen && element.offscreen == Some(true) {
        return Err(serde_json::json!({
            "ok": false,
            "dialog": dialog_identity(window),
            "button": button_json(&element),
            "error": {
                "code": "dialog_button_offscreen",
                "message": "dialog button is offscreen; set allow_offscreen only when that is intentional"
            }
        }));
    }
    Ok(element)
}

fn effective_selector(request: &DialogInvokeButtonRequest) -> Option<UiElementSelector> {
    if let Some(selector) = &request.selector {
        return Some(selector.clone());
    }
    request.button_name.as_ref().map(|name| UiElementSelector {
        name: Some(name.clone()),
        role: Some("Button".into()),
        ..Default::default()
    })
}

fn action_target(request: &DialogInvokeButtonRequest) -> UiActionTarget {
    UiActionTarget {
        element_ref: request.element_ref.clone(),
        selector: effective_selector(request),
        max_depth: request.max_depth,
        max_elements: request.max_elements,
        allow_offscreen: request.allow_offscreen,
    }
}

fn dialog_action_error(
    window: &WindowInfo,
    element: Option<&UiElementInfo>,
    error: UiAutomationError,
) -> serde_json::Value {
    tracing::warn!(
        hwnd = %window.hwnd_hex,
        pid = window.pid,
        error_code = ?error.code,
        "dialogs.invoke_button failed closed"
    );
    serde_json::json!({
        "ok": false,
        "dialog": dialog_identity(window),
        "button": element.map(button_json),
        "error": error,
        "direct_uia_pattern_used": false,
        "warnings": [
            "coordinate fallback was not dispatched; dialog actions use UI Automation patterns only"
        ]
    })
}

fn dialog_json(
    window: &WindowInfo,
    max_depth: usize,
    max_elements: usize,
    warnings: &mut Vec<String>,
) -> serde_json::Value {
    let (buttons, snapshot) = match ui_automation_snapshot(window, max_depth, max_elements) {
        Ok(snapshot) => {
            let mut buttons = Vec::new();
            collect_buttons(&snapshot.root, &mut buttons);
            (
                buttons,
                Some(snapshot_summary(
                    snapshot.flattened_count,
                    snapshot.truncated,
                    snapshot.warnings,
                )),
            )
        }
        Err(error) => {
            warnings.push(format!(
                "failed to inspect dialog {} with UI Automation: {error}",
                window.hwnd_hex
            ));
            (Vec::new(), None)
        }
    };
    serde_json::json!({
        "dialog_id": window.hwnd_hex,
        "kind": classify_dialog(window),
        "dialog_like": is_dialog_like(window),
        "automatable": !is_uac_prompt(window),
        "window": window_summary(window),
        "buttons": buttons,
        "snapshot_summary": snapshot,
    })
}

fn collect_buttons(element: &UiElementInfo, buttons: &mut Vec<serde_json::Value>) {
    if is_button_element(element) {
        buttons.push(button_json(element));
    }
    for child in &element.children {
        collect_buttons(child, buttons);
    }
}

fn button_json(element: &UiElementInfo) -> serde_json::Value {
    serde_json::json!({
        "element_ref": element.element_ref,
        "name": element.name,
        "role": element.role,
        "automation_id": element.automation_id,
        "class_name": element.class_name,
        "enabled": element.enabled,
        "focused": element.focused,
        "offscreen": element.offscreen,
        "bounds": element.bounds,
        "center": element_center(element.bounds.as_ref()),
    })
}

fn element_center(bounds: Option<&UiRect>) -> Option<serde_json::Value> {
    let bounds = bounds?;
    if bounds.width <= 0 || bounds.height <= 0 {
        return None;
    }
    Some(serde_json::json!({
        "x": bounds.x as f64 + bounds.width as f64 / 2.0,
        "y": bounds.y as f64 + bounds.height as f64 / 2.0,
        "coordinate_space": "screen_pixels",
    }))
}

fn snapshot_summary(
    flattened_count: usize,
    truncated: bool,
    warnings: Vec<String>,
) -> serde_json::Value {
    serde_json::json!({
        "flattened_count": flattened_count,
        "truncated": truncated,
        "warnings": warnings,
    })
}

fn is_button_element(element: &UiElementInfo) -> bool {
    matches!(element.role.as_deref(), Some("Button" | "SplitButton"))
}

fn is_dialog_like(window: &WindowInfo) -> bool {
    window.class_name.eq_ignore_ascii_case("#32770") || is_uac_prompt(window)
}

fn is_uac_prompt(window: &WindowInfo) -> bool {
    window
        .process_name
        .as_deref()
        .map(|name| name.eq_ignore_ascii_case("consent.exe"))
        .unwrap_or(false)
}

fn classify_dialog(window: &WindowInfo) -> &'static str {
    if is_uac_prompt(window) {
        "uac_prompt"
    } else if window.class_name.eq_ignore_ascii_case("#32770") {
        let title = window.title.to_ascii_lowercase();
        if title.contains("open") || title.contains("save") {
            "file_dialog"
        } else {
            "native_dialog"
        }
    } else if window.foreground {
        "foreground_window"
    } else {
        "dialog"
    }
}

fn window_summary(window: &WindowInfo) -> serde_json::Value {
    serde_json::json!({
        "hwnd": window.hwnd,
        "hwnd_hex": window.hwnd_hex,
        "pid": window.pid,
        "tid": window.tid,
        "process_name": window.process_name,
        "exe_path": window.exe_path,
        "title": window.title,
        "class_name": window.class_name,
        "rect": {
            "x": window.x,
            "y": window.y,
            "width": window.width,
            "height": window.height,
        },
        "visible": window.visible,
        "enabled": window.enabled,
        "foreground": window.foreground,
        "minimized": window.minimized,
        "cloaked": window.cloaked,
        "top_level": window.top_level,
    })
}

fn dialog_identity(window: &WindowInfo) -> serde_json::Value {
    serde_json::json!({
        "dialog_id": window.hwnd_hex,
        "hwnd": window.hwnd,
        "hwnd_hex": window.hwnd_hex,
        "pid": window.pid,
        "process_name": window.process_name,
        "exe_path": window.exe_path,
        "title": window.title,
        "class_name": window.class_name,
        "foreground": window.foreground,
    })
}

#[cfg(not(windows))]
fn secure_desktop_status() -> serde_json::Value {
    serde_json::json!({
        "active": false,
        "provider_enabled": false,
        "status": "unsupported_platform",
        "message": "secure desktop detection is only available on Windows"
    })
}

#[cfg(windows)]
fn secure_desktop_status() -> serde_json::Value {
    match input_desktop_name() {
        Ok(input_desktop) => {
            let active = !input_desktop.eq_ignore_ascii_case("default");
            serde_json::json!({
                "active": active,
                "provider_enabled": true,
                "status": if active { "secure_or_non_default_desktop" } else { "default_desktop" },
                "input_desktop": input_desktop,
                "message": if active {
                    "the active input desktop is not Default; UAC and secure-desktop prompts cannot be automated"
                } else {
                    "the active input desktop is Default"
                }
            })
        }
        Err(error) => serde_json::json!({
            "active": true,
            "provider_enabled": true,
            "status": "input_desktop_inaccessible",
            "message": "the input desktop could not be opened; this commonly happens while a secure desktop prompt is active",
            "error": error,
        }),
    }
}

#[cfg(windows)]
fn input_desktop_name() -> Result<String, String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_CONTROL_FLAGS,
        DESKTOP_READOBJECTS, UOI_NAME,
    };

    let desktop = unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) }
        .map_err(|error| error.to_string())?;
    let name = desktop_name(HANDLE(desktop.0));
    let _ = unsafe { CloseDesktop(desktop) };
    name
}

#[cfg(windows)]
fn desktop_name(handle: windows::Win32::Foundation::HANDLE) -> Result<String, String> {
    use windows::Win32::System::StationsAndDesktops::{GetUserObjectInformationW, UOI_NAME};

    let mut needed = 0u32;
    let _ = unsafe { GetUserObjectInformationW(handle, UOI_NAME, None, 0, Some(&mut needed)) };
    if needed == 0 {
        return Err("desktop name length was unavailable".into());
    }
    let mut buffer = vec![0u16; (needed as usize).saturating_add(1) / 2];
    unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            Some(buffer.as_mut_ptr() as *mut _),
            needed,
            Some(&mut needed),
        )
    }
    .map_err(|error| error.to_string())?;
    while buffer.last() == Some(&0) {
        buffer.pop();
    }
    Ok(String::from_utf16_lossy(&buffer))
}
