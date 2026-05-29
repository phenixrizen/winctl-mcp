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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiAutomationErrorCode {
    UnsupportedPlatform,
    ComInitializationFailed,
    SnapshotFailed,
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

    use windows::core::IUnknown;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
        IUIAutomationElementArray, TreeScope_Children,
    };

    use crate::uia::{
        control_type_name, ui_element_ref, UiAutomationError, UiAutomationErrorCode,
        UiAutomationSnapshot, UiElementInfo, UiOwnerWindow, UiRect,
    };
    use crate::WindowInfo;

    pub(super) fn snapshot(
        window: &WindowInfo,
        max_depth: usize,
        max_elements: usize,
    ) -> Result<UiAutomationSnapshot, UiAutomationError> {
        let max_depth = max_depth.min(32);
        let max_elements = max_elements.clamp(1, 10_000);
        let _apartment = ComApartment::initialize()?;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
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

        let mut state = SnapshotState {
            window,
            condition,
            count: 0,
            max_depth,
            max_elements,
            truncated: false,
            warnings: Vec::new(),
        };
        let root = build_element(&root, &mut state, Vec::new(), 0, 0)?;
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
