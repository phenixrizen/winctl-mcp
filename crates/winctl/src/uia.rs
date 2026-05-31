use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::WindowInfo;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiAutomationSnapshot {
    pub owner_window: UiOwnerWindow,
    pub root: UiElementInfo,
    pub flattened_count: usize,
    pub max_depth: usize,
    pub max_elements: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiOwnerWindow {
    pub hwnd: isize,
    pub hwnd_hex: String,
    pub pid: u32,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub title: String,
}

impl From<&WindowInfo> for UiOwnerWindow {
    fn from(window: &WindowInfo) -> Self {
        Self {
            hwnd: window.hwnd,
            hwnd_hex: window.hwnd_hex.clone(),
            pid: window.pid,
            process_name: window.process_name.clone(),
            executable_path: window.exe_path.clone(),
            title: window.title.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct UiRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiElementInfo {
    pub element_ref: String,
    pub path: Vec<usize>,
    pub depth: usize,
    pub child_index: usize,
    pub name: Option<String>,
    pub role: Option<String>,
    pub control_type_id: Option<i32>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub bounds: Option<UiRect>,
    pub enabled: Option<bool>,
    pub focused: Option<bool>,
    pub offscreen: Option<bool>,
    pub diagnostics: Vec<String>,
    pub children: Vec<UiElementInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct UiElementSelector {
    pub element_ref: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub text_contains: Option<String>,
    pub include_offscreen: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct UiActionTarget {
    pub element_ref: Option<String>,
    pub selector: Option<UiElementSelector>,
    pub max_depth: Option<usize>,
    pub max_elements: Option<usize>,
    #[serde(default)]
    pub allow_offscreen: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiExpandCollapseAction {
    Expand,
    Collapse,
    Toggle,
}

impl Default for UiExpandCollapseAction {
    fn default() -> Self {
        Self::Toggle
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiToggleDesiredState {
    Off,
    On,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiSelectionMode {
    Replace,
    Add,
    Remove,
}

impl Default for UiSelectionMode {
    fn default() -> Self {
        Self::Replace
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiActionOutcome {
    pub pattern_used: String,
    pub direct_uia_pattern_used: bool,
    pub before: UiElementInfo,
    pub after: Option<UiElementInfo>,
    pub value: Option<serde_json::Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiAutomationErrorCode {
    UnsupportedPlatform,
    ComInitializationFailed,
    SnapshotFailed,
    InvalidElementReference,
    ElementNotFound,
    AmbiguousElement,
    PatternUnavailable,
    ElementNotActionable,
    ActionFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct UiAutomationError {
    pub code: UiAutomationErrorCode,
    pub message: String,
}

pub fn ui_automation_snapshot(
    window: &WindowInfo,
    max_depth: usize,
    max_elements: usize,
) -> Result<UiAutomationSnapshot, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::snapshot(window, max_depth, max_elements)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, max_depth, max_elements);
        Err(UiAutomationError {
            code: UiAutomationErrorCode::UnsupportedPlatform,
            message: "UI Automation snapshots require Windows runtime".into(),
        })
    }
}

pub fn ui_invoke_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::invoke_pattern(window, target)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target);
        Err(unsupported_platform_error("uia.invoke"))
    }
}

pub fn ui_set_focus_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::set_focus_pattern(window, target)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target);
        Err(unsupported_platform_error("uia.set_focus"))
    }
}

pub fn ui_set_value_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
    value: &str,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::set_value_pattern(window, target, value)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target, value);
        Err(unsupported_platform_error("uia.set_value"))
    }
}

pub fn ui_get_value_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::get_value_pattern(window, target)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target);
        Err(unsupported_platform_error("uia.get_value"))
    }
}

pub fn ui_toggle_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
    desired_state: Option<UiToggleDesiredState>,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::toggle_pattern(window, target, desired_state)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target, desired_state);
        Err(unsupported_platform_error("uia.toggle"))
    }
}

pub fn ui_select_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
    mode: UiSelectionMode,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::select_pattern(window, target, mode)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target, mode);
        Err(unsupported_platform_error("uia.select"))
    }
}

pub fn ui_expand_collapse_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
    action: UiExpandCollapseAction,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::expand_collapse_pattern(window, target, action)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target, action);
        Err(unsupported_platform_error("uia.expand_collapse"))
    }
}

pub fn ui_range_value_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
    value: f64,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::range_value_pattern(window, target, value)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target, value);
        Err(unsupported_platform_error("uia.range_value"))
    }
}

pub fn ui_scroll_into_view_pattern(
    window: &WindowInfo,
    target: &UiActionTarget,
) -> Result<UiActionOutcome, UiAutomationError> {
    #[cfg(windows)]
    {
        windows_impl::scroll_into_view_pattern(window, target)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, target);
        Err(unsupported_platform_error("uia.scroll_into_view"))
    }
}

#[cfg(not(windows))]
fn unsupported_platform_error(tool_name: &str) -> UiAutomationError {
    UiAutomationError {
        code: UiAutomationErrorCode::UnsupportedPlatform,
        message: format!("{tool_name} requires Windows UI Automation runtime"),
    }
}

pub fn flatten_ui_elements(root: &UiElementInfo) -> Vec<&UiElementInfo> {
    fn visit<'a>(element: &'a UiElementInfo, output: &mut Vec<&'a UiElementInfo>) {
        output.push(element);
        for child in &element.children {
            visit(child, output);
        }
    }

    let mut output = Vec::new();
    visit(root, &mut output);
    output
}

pub fn find_ui_elements<'a>(
    snapshot: &'a UiAutomationSnapshot,
    selector: &UiElementSelector,
) -> Vec<&'a UiElementInfo> {
    flatten_ui_elements(&snapshot.root)
        .into_iter()
        .filter(|element| ui_element_matches(element, selector))
        .collect()
}

pub fn resolve_ui_element_ref<'a>(
    snapshot: &'a UiAutomationSnapshot,
    element_ref: &str,
) -> Option<&'a UiElementInfo> {
    flatten_ui_elements(&snapshot.root)
        .into_iter()
        .find(|element| element.element_ref == element_ref)
}

pub fn ui_element_matches(element: &UiElementInfo, selector: &UiElementSelector) -> bool {
    selector
        .element_ref
        .as_ref()
        .map(|value| element.element_ref == *value)
        .unwrap_or(true)
        && selector
            .name
            .as_ref()
            .map(|value| opt_eq_ignore_ascii_case(element.name.as_deref(), value))
            .unwrap_or(true)
        && selector
            .role
            .as_ref()
            .map(|value| opt_eq_ignore_ascii_case(element.role.as_deref(), value))
            .unwrap_or(true)
        && selector
            .automation_id
            .as_ref()
            .map(|value| opt_eq_ignore_ascii_case(element.automation_id.as_deref(), value))
            .unwrap_or(true)
        && selector
            .class_name
            .as_ref()
            .map(|value| opt_eq_ignore_ascii_case(element.class_name.as_deref(), value))
            .unwrap_or(true)
        && selector
            .text_contains
            .as_ref()
            .map(|value| {
                contains_ignore_ascii_case(element.name.as_deref(), value)
                    || contains_ignore_ascii_case(element.automation_id.as_deref(), value)
                    || contains_ignore_ascii_case(element.class_name.as_deref(), value)
            })
            .unwrap_or(true)
        && (selector.include_offscreen.unwrap_or(false) || !element.offscreen.unwrap_or(false))
}

pub fn ui_element_ref(window: &WindowInfo, path: &[usize]) -> String {
    let path = path
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join(".");
    format!("uia:{}:{}:{path}", window.hwnd_hex, window.pid)
}

pub fn control_type_name(control_type_id: i32) -> Option<&'static str> {
    Some(match control_type_id {
        50000 => "Button",
        50001 => "Calendar",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50009 => "Menu",
        50010 => "MenuBar",
        50011 => "MenuItem",
        50012 => "ProgressBar",
        50013 => "RadioButton",
        50014 => "ScrollBar",
        50015 => "Slider",
        50016 => "Spinner",
        50017 => "StatusBar",
        50018 => "Tab",
        50019 => "TabItem",
        50020 => "Text",
        50021 => "ToolBar",
        50022 => "ToolTip",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "Custom",
        50026 => "Group",
        50027 => "Thumb",
        50028 => "DataGrid",
        50029 => "DataItem",
        50030 => "Document",
        50031 => "SplitButton",
        50032 => "Window",
        50033 => "Pane",
        50034 => "Header",
        50035 => "HeaderItem",
        50036 => "Table",
        50037 => "TitleBar",
        50038 => "Separator",
        50039 => "SemanticZoom",
        50040 => "AppBar",
        _ => return None,
    })
}

fn opt_eq_ignore_ascii_case(actual: Option<&str>, expected: &str) -> bool {
    actual
        .map(|actual| actual.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn contains_ignore_ascii_case(actual: Option<&str>, expected: &str) -> bool {
    actual
        .map(|actual| {
            actual
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;

    use windows::core::{IUnknown, Interface, BSTR};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded,
        ExpandCollapseState_LeafNode, ExpandCollapseState_PartiallyExpanded, IUIAutomation,
        IUIAutomationCondition, IUIAutomationElement, IUIAutomationElementArray,
        IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
        IUIAutomationRangeValuePattern, IUIAutomationScrollItemPattern,
        IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationValuePattern,
        ToggleState_Indeterminate, ToggleState_Off, ToggleState_On, TreeScope_Children,
        UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_RangeValuePatternId,
        UIA_ScrollItemPatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId,
        UIA_ValuePatternId, UIA_PATTERN_ID,
    };

    use crate::uia::{
        control_type_name, find_ui_elements, ui_element_ref, UiActionOutcome, UiActionTarget,
        UiAutomationError, UiAutomationErrorCode, UiAutomationSnapshot, UiElementInfo,
        UiExpandCollapseAction, UiOwnerWindow, UiRect, UiSelectionMode, UiToggleDesiredState,
    };
    use crate::WindowInfo;

    pub(super) fn snapshot(
        window: &WindowInfo,
        max_depth: usize,
        max_elements: usize,
    ) -> Result<UiAutomationSnapshot, UiAutomationError> {
        let max_depth = max_depth.min(32);
        let max_elements = max_elements.clamp(1, 10_000);
        let session = AutomationSession::open(window)?;
        snapshot_from_root(
            window,
            &session.root,
            &session.condition,
            max_depth,
            max_elements,
        )
    }

    pub(super) fn invoke_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        with_action(window, target, "InvokePattern.Invoke", false, |element| {
            let pattern = current_pattern::<IUIAutomationInvokePattern>(
                element,
                UIA_InvokePatternId,
                "InvokePattern",
            )?;
            unsafe { pattern.Invoke() }.map_err(action_failed("InvokePattern.Invoke"))?;
            Ok(None)
        })
    }

    pub(super) fn set_focus_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        with_action(
            window,
            target,
            "IUIAutomationElement.SetFocus",
            false,
            |element| {
                unsafe { element.SetFocus() }
                    .map_err(action_failed("IUIAutomationElement.SetFocus"))?;
                Ok(None)
            },
        )
    }

    pub(super) fn set_value_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
        value: &str,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        with_action(window, target, "ValuePattern.SetValue", false, |element| {
            let pattern = current_pattern::<IUIAutomationValuePattern>(
                element,
                UIA_ValuePatternId,
                "ValuePattern",
            )?;
            let readonly = unsafe { pattern.CurrentIsReadOnly() }
                .map(|value| value.as_bool())
                .unwrap_or(false);
            if readonly {
                return Err(UiAutomationError {
                    code: UiAutomationErrorCode::ElementNotActionable,
                    message: "ValuePattern target is read-only".into(),
                });
            }
            let value_bstr = BSTR::from(value);
            unsafe { pattern.SetValue(&value_bstr) }
                .map_err(action_failed("ValuePattern.SetValue"))?;
            Ok(Some(serde_json::json!({"set_value": value})))
        })
    }

    pub(super) fn get_value_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        let session = AutomationSession::open(window)?;
        let resolved = resolve_target(&session, window, target)?;
        let pattern = current_pattern::<IUIAutomationValuePattern>(
            &resolved.element,
            UIA_ValuePatternId,
            "ValuePattern",
        )?;
        let current = unsafe { pattern.CurrentValue() }
            .map_err(action_failed("ValuePattern.CurrentValue"))?
            .to_string();
        let readonly = unsafe { pattern.CurrentIsReadOnly() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        Ok(UiActionOutcome {
            pattern_used: "ValuePattern.CurrentValue".into(),
            direct_uia_pattern_used: true,
            before: resolved.info.clone(),
            after: Some(read_current_element(
                window,
                &resolved.element,
                &resolved.path,
            )),
            value: Some(serde_json::json!({
                "value": current,
                "is_read_only": readonly,
            })),
            warnings: Vec::new(),
        })
    }

    pub(super) fn toggle_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
        desired_state: Option<UiToggleDesiredState>,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        with_action(window, target, "TogglePattern.Toggle", false, |element| {
            let pattern = current_pattern::<IUIAutomationTogglePattern>(
                element,
                UIA_TogglePatternId,
                "TogglePattern",
            )?;
            let before_state = unsafe { pattern.CurrentToggleState() }
                .map_err(action_failed("TogglePattern.CurrentToggleState"))?;
            let before = toggle_state_name(before_state.0);
            let mut toggle_count = 0u8;
            let desired = desired_state.map(toggle_desired_state_name);
            if let Some(desired_state) = desired_state {
                while !toggle_state_matches(before_state.0, desired_state) && toggle_count < 3 {
                    unsafe { pattern.Toggle() }.map_err(action_failed("TogglePattern.Toggle"))?;
                    toggle_count += 1;
                    let current = unsafe { pattern.CurrentToggleState() }
                        .map_err(action_failed("TogglePattern.CurrentToggleState"))?;
                    if toggle_state_matches(current.0, desired_state) {
                        break;
                    }
                }
                let reached = unsafe { pattern.CurrentToggleState() }
                    .map_err(action_failed("TogglePattern.CurrentToggleState"))?;
                if !toggle_state_matches(reached.0, desired_state) {
                    return Err(UiAutomationError {
                        code: UiAutomationErrorCode::ActionFailed,
                        message: format!(
                            "TogglePattern.Toggle did not reach desired state {}; current state is {}",
                            toggle_desired_state_name(desired_state),
                            toggle_state_name(reached.0)
                        ),
                    });
                }
            } else {
                unsafe { pattern.Toggle() }.map_err(action_failed("TogglePattern.Toggle"))?;
                toggle_count = 1;
            }
            let after_state = unsafe { pattern.CurrentToggleState() }
                .map_err(action_failed("TogglePattern.CurrentToggleState"))?;
            let after = toggle_state_name(after_state.0);
            Ok(Some(serde_json::json!({
                "before_state": before,
                "after_state": after,
                "desired_state": desired,
                "toggle_count": toggle_count,
                "actual_action": if toggle_count == 0 { "none" } else { "toggle" },
            })))
        })
    }

    pub(super) fn select_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
        mode: UiSelectionMode,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        let session = AutomationSession::open(window)?;
        let resolved = resolve_target(&session, window, target)?;
        ensure_actionable(&resolved.info, target.allow_offscreen)?;
        let pattern = current_pattern::<IUIAutomationSelectionItemPattern>(
            &resolved.element,
            UIA_SelectionItemPatternId,
            "SelectionItemPattern",
        )?;
        let before_selected = unsafe { pattern.CurrentIsSelected() }
            .map_err(action_failed("SelectionItemPattern.CurrentIsSelected"))?
            .as_bool();
        let (pattern_used, actual_action) = match mode {
            UiSelectionMode::Replace => {
                unsafe { pattern.Select() }
                    .map_err(action_failed("SelectionItemPattern.Select"))?;
                ("SelectionItemPattern.Select", "replace")
            }
            UiSelectionMode::Add if before_selected => {
                ("SelectionItemPattern.CurrentIsSelected", "none")
            }
            UiSelectionMode::Add => {
                unsafe { pattern.AddToSelection() }
                    .map_err(action_failed("SelectionItemPattern.AddToSelection"))?;
                ("SelectionItemPattern.AddToSelection", "add")
            }
            UiSelectionMode::Remove if !before_selected => {
                ("SelectionItemPattern.CurrentIsSelected", "none")
            }
            UiSelectionMode::Remove => {
                unsafe { pattern.RemoveFromSelection() }
                    .map_err(action_failed("SelectionItemPattern.RemoveFromSelection"))?;
                ("SelectionItemPattern.RemoveFromSelection", "remove")
            }
        };
        let selected = unsafe { pattern.CurrentIsSelected() }
            .map_err(action_failed("SelectionItemPattern.CurrentIsSelected"))?
            .as_bool();
        Ok(UiActionOutcome {
            pattern_used: pattern_used.into(),
            direct_uia_pattern_used: true,
            before: resolved.info.clone(),
            after: Some(read_current_element(
                window,
                &resolved.element,
                &resolved.path,
            )),
            value: Some(serde_json::json!({
                "mode": selection_mode_name(mode),
                "actual_action": actual_action,
                "before_selected": before_selected,
                "selected": selected,
            })),
            warnings: Vec::new(),
        })
    }

    pub(super) fn expand_collapse_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
        action: UiExpandCollapseAction,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        let session = AutomationSession::open(window)?;
        let resolved = resolve_target(&session, window, target)?;
        ensure_actionable(&resolved.info, target.allow_offscreen)?;
        let pattern = current_pattern::<IUIAutomationExpandCollapsePattern>(
            &resolved.element,
            UIA_ExpandCollapsePatternId,
            "ExpandCollapsePattern",
        )?;
        let before_state = unsafe { pattern.CurrentExpandCollapseState() }.map_err(
            action_failed("ExpandCollapsePattern.CurrentExpandCollapseState"),
        )?;
        let actual_action = match action {
            UiExpandCollapseAction::Expand => UiExpandCollapseAction::Expand,
            UiExpandCollapseAction::Collapse => UiExpandCollapseAction::Collapse,
            UiExpandCollapseAction::Toggle => {
                if before_state == ExpandCollapseState_Collapsed {
                    UiExpandCollapseAction::Expand
                } else if before_state == ExpandCollapseState_Expanded
                    || before_state == ExpandCollapseState_PartiallyExpanded
                {
                    UiExpandCollapseAction::Collapse
                } else {
                    return Err(UiAutomationError {
                        code: UiAutomationErrorCode::ElementNotActionable,
                        message:
                            "ExpandCollapsePattern target is a leaf node and cannot be toggled"
                                .into(),
                    });
                }
            }
        };
        match actual_action {
            UiExpandCollapseAction::Expand => unsafe { pattern.Expand() }
                .map_err(action_failed("ExpandCollapsePattern.Expand"))?,
            UiExpandCollapseAction::Collapse => unsafe { pattern.Collapse() }
                .map_err(action_failed("ExpandCollapsePattern.Collapse"))?,
            UiExpandCollapseAction::Toggle => unreachable!("toggle is resolved before dispatch"),
        }
        let after_state = unsafe { pattern.CurrentExpandCollapseState() }.ok();
        Ok(UiActionOutcome {
            pattern_used: format!("ExpandCollapsePattern.{actual_action:?}"),
            direct_uia_pattern_used: true,
            before: resolved.info.clone(),
            after: Some(read_current_element(
                window,
                &resolved.element,
                &resolved.path,
            )),
            value: Some(serde_json::json!({
                "requested_action": action,
                "actual_action": actual_action,
                "before_state": expand_state_name(before_state.0),
                "after_state": after_state.map(|state| expand_state_name(state.0)),
            })),
            warnings: Vec::new(),
        })
    }

    pub(super) fn range_value_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
        value: f64,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        with_action(
            window,
            target,
            "RangeValuePattern.SetValue",
            false,
            |element| {
                let pattern = current_pattern::<IUIAutomationRangeValuePattern>(
                    element,
                    UIA_RangeValuePatternId,
                    "RangeValuePattern",
                )?;
                let readonly = unsafe { pattern.CurrentIsReadOnly() }
                    .map(|value| value.as_bool())
                    .unwrap_or(false);
                if readonly {
                    return Err(UiAutomationError {
                        code: UiAutomationErrorCode::ElementNotActionable,
                        message: "RangeValuePattern target is read-only".into(),
                    });
                }
                let minimum = unsafe { pattern.CurrentMinimum() }.ok();
                let maximum = unsafe { pattern.CurrentMaximum() }.ok();
                if minimum.map(|minimum| value < minimum).unwrap_or(false)
                    || maximum.map(|maximum| value > maximum).unwrap_or(false)
                {
                    return Err(UiAutomationError {
                        code: UiAutomationErrorCode::ElementNotActionable,
                        message: format!(
                            "range value {value} is outside the advertised bounds {:?}..{:?}",
                            minimum, maximum
                        ),
                    });
                }
                let before = unsafe { pattern.CurrentValue() }.ok();
                unsafe { pattern.SetValue(value) }
                    .map_err(action_failed("RangeValuePattern.SetValue"))?;
                let after = unsafe { pattern.CurrentValue() }.ok();
                Ok(Some(serde_json::json!({
                    "requested_value": value,
                    "before_value": before,
                    "after_value": after,
                    "minimum": minimum,
                    "maximum": maximum,
                })))
            },
        )
    }

    pub(super) fn scroll_into_view_pattern(
        window: &WindowInfo,
        target: &UiActionTarget,
    ) -> Result<UiActionOutcome, UiAutomationError> {
        with_action(
            window,
            target,
            "ScrollItemPattern.ScrollIntoView",
            true,
            |element| {
                let pattern = current_pattern::<IUIAutomationScrollItemPattern>(
                    element,
                    UIA_ScrollItemPatternId,
                    "ScrollItemPattern",
                )?;
                unsafe { pattern.ScrollIntoView() }
                    .map_err(action_failed("ScrollItemPattern.ScrollIntoView"))?;
                Ok(None)
            },
        )
    }

    struct AutomationSession {
        _apartment: ComApartment,
        _automation: IUIAutomation,
        condition: IUIAutomationCondition,
        root: IUIAutomationElement,
    }

    impl AutomationSession {
        fn open(window: &WindowInfo) -> Result<Self, UiAutomationError> {
            let apartment = ComApartment::initialize()?;
            let automation: IUIAutomation = unsafe {
                CoCreateInstance(&CUIAutomation, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
            }
            .map_err(|error| UiAutomationError {
                code: UiAutomationErrorCode::ComInitializationFailed,
                message: format!("failed to create UI Automation COM object: {error}"),
            })?;
            let root = unsafe { automation.ElementFromHandle(HWND(window.hwnd as *mut c_void)) }
                .map_err(|error| UiAutomationError {
                    code: UiAutomationErrorCode::SnapshotFailed,
                    message: format!("failed to get UI Automation element from HWND: {error}"),
                })?;
            let condition =
                unsafe { automation.CreateTrueCondition() }.map_err(|error| UiAutomationError {
                    code: UiAutomationErrorCode::SnapshotFailed,
                    message: format!("failed to create UI Automation true condition: {error}"),
                })?;
            Ok(Self {
                _apartment: apartment,
                _automation: automation,
                condition,
                root,
            })
        }
    }

    fn snapshot_from_root(
        window: &WindowInfo,
        root: &IUIAutomationElement,
        condition: &IUIAutomationCondition,
        max_depth: usize,
        max_elements: usize,
    ) -> Result<UiAutomationSnapshot, UiAutomationError> {
        let mut state = SnapshotState {
            window,
            condition: condition.clone(),
            count: 0,
            max_depth,
            max_elements,
            truncated: false,
            warnings: Vec::new(),
        };
        let root = build_element(root, &mut state, Vec::new(), 0, 0)?;
        Ok(UiAutomationSnapshot {
            owner_window: UiOwnerWindow::from(window),
            root,
            flattened_count: state.count,
            max_depth,
            max_elements,
            truncated: state.truncated,
            warnings: state.warnings,
        })
    }

    struct ResolvedElement {
        element: IUIAutomationElement,
        path: Vec<usize>,
        info: UiElementInfo,
    }

    fn with_action<F>(
        window: &WindowInfo,
        target: &UiActionTarget,
        pattern_used: &'static str,
        allow_offscreen_for_action: bool,
        action: F,
    ) -> Result<UiActionOutcome, UiAutomationError>
    where
        F: FnOnce(&IUIAutomationElement) -> Result<Option<serde_json::Value>, UiAutomationError>,
    {
        let session = AutomationSession::open(window)?;
        let resolved = resolve_target(&session, window, target)?;
        ensure_actionable(
            &resolved.info,
            target.allow_offscreen || allow_offscreen_for_action,
        )?;
        let value = action(&resolved.element)?;
        Ok(UiActionOutcome {
            pattern_used: pattern_used.into(),
            direct_uia_pattern_used: true,
            before: resolved.info.clone(),
            after: Some(read_current_element(
                window,
                &resolved.element,
                &resolved.path,
            )),
            value,
            warnings: Vec::new(),
        })
    }

    fn resolve_target(
        session: &AutomationSession,
        window: &WindowInfo,
        target: &UiActionTarget,
    ) -> Result<ResolvedElement, UiAutomationError> {
        let path = if let Some(element_ref) = target.element_ref.as_deref() {
            parse_element_ref(window, element_ref)?
        } else if let Some(selector) = target.selector.as_ref() {
            let snapshot = snapshot_from_root(
                window,
                &session.root,
                &session.condition,
                target.max_depth.unwrap_or(8).min(32),
                target.max_elements.unwrap_or(2_000).clamp(1, 10_000),
            )?;
            let matches = find_ui_elements(&snapshot, selector);
            match matches.len() {
                1 => matches[0].path.clone(),
                0 => {
                    return Err(UiAutomationError {
                        code: UiAutomationErrorCode::ElementNotFound,
                        message: "selector did not match any UI Automation element in the current bound window".into(),
                    });
                }
                count => {
                    return Err(UiAutomationError {
                        code: UiAutomationErrorCode::AmbiguousElement,
                        message: format!(
                            "selector matched {count} UI Automation elements; refine before acting"
                        ),
                    });
                }
            }
        } else {
            return Err(UiAutomationError {
                code: UiAutomationErrorCode::ElementNotFound,
                message: "element_ref or selector is required".into(),
            });
        };
        let element = element_from_path(&session.root, &session.condition, &path)?;
        let info = read_current_element(window, &element, &path);
        Ok(ResolvedElement {
            element,
            path,
            info,
        })
    }

    fn parse_element_ref(
        window: &WindowInfo,
        element_ref: &str,
    ) -> Result<Vec<usize>, UiAutomationError> {
        let parts = element_ref.splitn(4, ':').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "uia" {
            return Err(UiAutomationError {
                code: UiAutomationErrorCode::InvalidElementReference,
                message: format!("invalid UI Automation element reference: {element_ref}"),
            });
        }
        if !parts[1].eq_ignore_ascii_case(&window.hwnd_hex) {
            return Err(UiAutomationError {
                code: UiAutomationErrorCode::InvalidElementReference,
                message: format!(
                    "element reference belongs to HWND {}, but current bound HWND is {}",
                    parts[1], window.hwnd_hex
                ),
            });
        }
        let pid = parts[2].parse::<u32>().map_err(|error| UiAutomationError {
            code: UiAutomationErrorCode::InvalidElementReference,
            message: format!("invalid UI Automation element reference PID: {error}"),
        })?;
        if pid != window.pid {
            return Err(UiAutomationError {
                code: UiAutomationErrorCode::InvalidElementReference,
                message: format!(
                    "element reference belongs to PID {pid}, but current bound PID is {}",
                    window.pid
                ),
            });
        }
        if parts[3].is_empty() {
            return Ok(Vec::new());
        }
        parts[3]
            .split('.')
            .map(|part| {
                part.parse::<usize>().map_err(|error| UiAutomationError {
                    code: UiAutomationErrorCode::InvalidElementReference,
                    message: format!(
                        "invalid UI Automation element path segment {part:?}: {error}"
                    ),
                })
            })
            .collect()
    }

    fn element_from_path(
        root: &IUIAutomationElement,
        condition: &IUIAutomationCondition,
        path: &[usize],
    ) -> Result<IUIAutomationElement, UiAutomationError> {
        let mut current = root.clone();
        let mut current_path = Vec::new();
        for index in path {
            let children =
                unsafe { current.FindAll(TreeScope_Children, condition) }.map_err(|error| {
                    UiAutomationError {
                        code: UiAutomationErrorCode::ElementNotFound,
                        message: format!(
                            "failed to enumerate UI Automation children at {:?}: {error}",
                            current_path
                        ),
                    }
                })?;
            let len = unsafe { children.Length() }.unwrap_or_default().max(0) as usize;
            if *index >= len {
                return Err(UiAutomationError {
                    code: UiAutomationErrorCode::ElementNotFound,
                    message: format!(
                        "UI Automation element path {:?} is stale; child index {index} is outside {len} children",
                        path
                    ),
                });
            }
            current = unsafe { children.GetElement(*index as i32) }.map_err(|error| {
                UiAutomationError {
                    code: UiAutomationErrorCode::ElementNotFound,
                    message: format!(
                        "failed to resolve UI Automation child {index} at {:?}: {error}",
                        current_path
                    ),
                }
            })?;
            current_path.push(*index);
        }
        Ok(current)
    }

    fn read_current_element(
        window: &WindowInfo,
        element: &IUIAutomationElement,
        path: &[usize],
    ) -> UiElementInfo {
        let depth = path.len();
        let child_index = path.last().copied().unwrap_or_default();
        read_element(element, window, path, depth, child_index)
    }

    fn ensure_actionable(
        element: &UiElementInfo,
        allow_offscreen: bool,
    ) -> Result<(), UiAutomationError> {
        if element.enabled == Some(false) {
            return Err(UiAutomationError {
                code: UiAutomationErrorCode::ElementNotActionable,
                message: "UI Automation element is disabled".into(),
            });
        }
        if element.offscreen == Some(true) && !allow_offscreen {
            return Err(UiAutomationError {
                code: UiAutomationErrorCode::ElementNotActionable,
                message: "UI Automation element is offscreen; set allow_offscreen when the action supports it".into(),
            });
        }
        Ok(())
    }

    fn current_pattern<T>(
        element: &IUIAutomationElement,
        pattern_id: UIA_PATTERN_ID,
        pattern_name: &'static str,
    ) -> Result<T, UiAutomationError>
    where
        T: Interface,
    {
        unsafe { element.GetCurrentPatternAs::<T>(pattern_id) }.map_err(|error| UiAutomationError {
            code: UiAutomationErrorCode::PatternUnavailable,
            message: format!("{pattern_name} is not available on the target element: {error}"),
        })
    }

    fn action_failed(action: &'static str) -> impl Fn(windows::core::Error) -> UiAutomationError {
        move |error| UiAutomationError {
            code: UiAutomationErrorCode::ActionFailed,
            message: format!("{action} failed: {error}"),
        }
    }

    fn toggle_state_name(value: i32) -> &'static str {
        if value == ToggleState_On.0 {
            "on"
        } else if value == ToggleState_Off.0 {
            "off"
        } else if value == ToggleState_Indeterminate.0 {
            "indeterminate"
        } else {
            "unknown"
        }
    }

    fn toggle_state_matches(value: i32, desired: UiToggleDesiredState) -> bool {
        match desired {
            UiToggleDesiredState::Off => value == ToggleState_Off.0,
            UiToggleDesiredState::On => value == ToggleState_On.0,
            UiToggleDesiredState::Indeterminate => value == ToggleState_Indeterminate.0,
        }
    }

    fn toggle_desired_state_name(value: UiToggleDesiredState) -> &'static str {
        match value {
            UiToggleDesiredState::Off => "off",
            UiToggleDesiredState::On => "on",
            UiToggleDesiredState::Indeterminate => "indeterminate",
        }
    }

    fn selection_mode_name(value: UiSelectionMode) -> &'static str {
        match value {
            UiSelectionMode::Replace => "replace",
            UiSelectionMode::Add => "add",
            UiSelectionMode::Remove => "remove",
        }
    }

    fn expand_state_name(value: i32) -> &'static str {
        if value == ExpandCollapseState_Collapsed.0 {
            "collapsed"
        } else if value == ExpandCollapseState_Expanded.0 {
            "expanded"
        } else if value == ExpandCollapseState_PartiallyExpanded.0 {
            "partially_expanded"
        } else if value == ExpandCollapseState_LeafNode.0 {
            "leaf_node"
        } else {
            "unknown"
        }
    }

    struct ComApartment;

    impl ComApartment {
        fn initialize() -> Result<Self, UiAutomationError> {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .map_err(|error| UiAutomationError {
                    code: UiAutomationErrorCode::ComInitializationFailed,
                    message: format!("failed to initialize COM apartment: {error}"),
                })?;
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    struct SnapshotState<'a> {
        window: &'a WindowInfo,
        condition: IUIAutomationCondition,
        count: usize,
        max_depth: usize,
        max_elements: usize,
        truncated: bool,
        warnings: Vec<String>,
    }

    fn build_element(
        element: &IUIAutomationElement,
        state: &mut SnapshotState<'_>,
        path: Vec<usize>,
        depth: usize,
        child_index: usize,
    ) -> Result<UiElementInfo, UiAutomationError> {
        state.count += 1;
        let mut info = read_element(element, state.window, &path, depth, child_index);
        if depth >= state.max_depth || state.count >= state.max_elements {
            state.truncated = true;
            return Ok(info);
        }

        let children = match unsafe { element.FindAll(TreeScope_Children, &state.condition) } {
            Ok(children) => children,
            Err(error) => {
                info.diagnostics
                    .push(format!("children inaccessible: {error}"));
                return Ok(info);
            }
        };
        append_children(children, state, &path, depth + 1, &mut info)?;
        Ok(info)
    }

    fn append_children(
        children: IUIAutomationElementArray,
        state: &mut SnapshotState<'_>,
        parent_path: &[usize],
        depth: usize,
        parent: &mut UiElementInfo,
    ) -> Result<(), UiAutomationError> {
        let len = unsafe { children.Length() }.unwrap_or_default().max(0) as usize;
        for index in 0..len {
            if state.count >= state.max_elements {
                state.truncated = true;
                break;
            }
            let child = match unsafe { children.GetElement(index as i32) } {
                Ok(child) => child,
                Err(error) => {
                    state.warnings.push(format!(
                        "child {index} inaccessible at {:?}: {error}",
                        parent_path
                    ));
                    continue;
                }
            };
            let mut path = parent_path.to_vec();
            path.push(index);
            parent
                .children
                .push(build_element(&child, state, path, depth, index)?);
        }
        Ok(())
    }

    fn read_element(
        element: &IUIAutomationElement,
        window: &WindowInfo,
        path: &[usize],
        depth: usize,
        child_index: usize,
    ) -> UiElementInfo {
        let mut diagnostics = Vec::new();
        let name = read_string("name", &mut diagnostics, || unsafe {
            element.CurrentName()
        });
        let automation_id = read_string("automation_id", &mut diagnostics, || unsafe {
            element.CurrentAutomationId()
        });
        let class_name = read_string("class_name", &mut diagnostics, || unsafe {
            element.CurrentClassName()
        });
        let control_type_id = match unsafe { element.CurrentControlType() } {
            Ok(value) => Some(value.0),
            Err(error) => {
                diagnostics.push(format!("control_type inaccessible: {error}"));
                None
            }
        };
        let bounds = match unsafe { element.CurrentBoundingRectangle() } {
            Ok(rect) => Some(rect_to_ui_rect(rect)),
            Err(error) => {
                diagnostics.push(format!("bounds inaccessible: {error}"));
                None
            }
        };
        let enabled = read_bool("enabled", &mut diagnostics, || unsafe {
            element.CurrentIsEnabled()
        });
        let focused = read_bool("focused", &mut diagnostics, || unsafe {
            element.CurrentHasKeyboardFocus()
        });
        let offscreen = read_bool("offscreen", &mut diagnostics, || unsafe {
            element.CurrentIsOffscreen()
        });

        UiElementInfo {
            element_ref: ui_element_ref(window, path),
            path: path.to_vec(),
            depth,
            child_index,
            name,
            role: control_type_id
                .and_then(control_type_name)
                .map(str::to_owned),
            control_type_id,
            automation_id,
            class_name,
            bounds,
            enabled,
            focused,
            offscreen,
            diagnostics,
            children: Vec::new(),
        }
    }

    fn read_string<F>(field: &str, diagnostics: &mut Vec<String>, operation: F) -> Option<String>
    where
        F: FnOnce() -> windows::core::Result<windows::core::BSTR>,
    {
        match operation() {
            Ok(value) => {
                let value = value.to_string();
                if value.is_empty() {
                    None
                } else {
                    Some(value)
                }
            }
            Err(error) => {
                diagnostics.push(format!("{field} inaccessible: {error}"));
                None
            }
        }
    }

    fn read_bool<F>(field: &str, diagnostics: &mut Vec<String>, operation: F) -> Option<bool>
    where
        F: FnOnce() -> windows::core::Result<windows::core::BOOL>,
    {
        match operation() {
            Ok(value) => Some(value.as_bool()),
            Err(error) => {
                diagnostics.push(format!("{field} inaccessible: {error}"));
                None
            }
        }
    }

    fn rect_to_ui_rect(rect: RECT) -> UiRect {
        UiRect {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0),
            height: (rect.bottom - rect.top).max(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(path: Vec<usize>, name: &str, role: &str) -> UiElementInfo {
        UiElementInfo {
            element_ref: format!("ref-{name}"),
            path,
            depth: 0,
            child_index: 0,
            name: Some(name.into()),
            role: Some(role.into()),
            control_type_id: Some(50000),
            automation_id: Some(format!("{name}-id")),
            class_name: Some("ButtonClass".into()),
            bounds: Some(UiRect {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
            enabled: Some(true),
            focused: Some(false),
            offscreen: Some(false),
            diagnostics: Vec::new(),
            children: Vec::new(),
        }
    }

    #[test]
    fn selector_matches_semantic_fields() {
        let item = element(vec![0], "Settings", "Button");
        let selector = UiElementSelector {
            name: Some("settings".into()),
            role: Some("button".into()),
            automation_id: Some("settings-id".into()),
            class_name: Some("buttonclass".into()),
            ..Default::default()
        };

        assert!(ui_element_matches(&item, &selector));
    }

    #[test]
    fn flatten_preserves_hierarchy_order() {
        let mut root = element(vec![], "Root", "Window");
        root.children.push(element(vec![0], "First", "Button"));
        root.children.push(element(vec![1], "Second", "Text"));

        let names: Vec<_> = flatten_ui_elements(&root)
            .into_iter()
            .filter_map(|element| element.name.as_deref())
            .collect();

        assert_eq!(names, vec!["Root", "First", "Second"]);
    }

    #[test]
    fn stable_element_ref_includes_owner_identity_and_path() {
        let window = WindowInfo {
            id: "hwnd:0x1".into(),
            hwnd: 1,
            hwnd_hex: "0x0000000000000001".into(),
            pid: 42,
            tid: 1,
            process_name: Some("app.exe".into()),
            exe_path: Some("C:/app.exe".into()),
            title: "App".into(),
            class_name: "Window".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        };

        assert_eq!(
            ui_element_ref(&window, &[0, 2, 1]),
            "uia:0x0000000000000001:42:0.2.1"
        );
    }
}
