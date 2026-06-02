use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{list_processes, list_windows, ProcessInfo, WindowInfo};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserKind {
    Chrome,
    Edge,
    Firefox,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BrowserState {
    pub processes: Vec<BrowserProcess>,
    pub windows: Vec<BrowserWindow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BrowserProcess {
    pub pid: u32,
    pub browser: BrowserKind,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub parent_pid: Option<u32>,
    pub command_line: Option<String>,
    pub profile_hint: Option<String>,
    pub tracked_by_server: bool,
    pub launch_id: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BrowserWindow {
    pub browser: BrowserKind,
    pub hwnd: isize,
    pub hwnd_hex: String,
    pub pid: u32,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub title: String,
    pub class_name: String,
    pub visible: bool,
    pub minimized: bool,
    pub cloaked: bool,
    pub foreground: bool,
    pub rect: BrowserWindowRect,
    pub tab_title_hint: Option<String>,
    pub profile_hint: Option<String>,
    pub tracked_by_server: bool,
    pub launch_id: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BrowserWindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct BrowserStateFilter {
    pub browser: Option<BrowserKind>,
    pub pid: Option<u32>,
    pub include_windows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BrowserSessionIdentity {
    pub browser: BrowserKind,
    pub pid: u32,
    pub hwnd_hex: Option<String>,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub profile_hint: Option<String>,
    pub tab_title_hint: Option<String>,
}

pub fn browser_kind_for_process_name(name: &str) -> Option<BrowserKind> {
    match name.to_ascii_lowercase().as_str() {
        "chrome.exe" | "google chrome" => Some(BrowserKind::Chrome),
        "msedge.exe" | "microsoftedge.exe" | "microsoft edge" => Some(BrowserKind::Edge),
        "firefox.exe" | "firefox" => Some(BrowserKind::Firefox),
        _ => None,
    }
}

pub fn browser_kind_for_process(process: &ProcessInfo) -> Option<BrowserKind> {
    process
        .process_name
        .as_deref()
        .and_then(browser_kind_for_process_name)
        .or_else(|| {
            process.exe_path.as_deref().and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(browser_kind_for_process_name)
            })
        })
}

pub fn browser_kind_for_window(window: &WindowInfo) -> Option<BrowserKind> {
    window
        .process_name
        .as_deref()
        .and_then(browser_kind_for_process_name)
        .or_else(|| {
            window.exe_path.as_deref().and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(browser_kind_for_process_name)
            })
        })
}

pub fn browser_state(filter: BrowserStateFilter) -> BrowserState {
    let mut warnings = Vec::new();
    let processes = match list_processes() {
        Ok(processes) => processes,
        Err(error) => {
            warnings.push(error.message);
            Vec::new()
        }
    };
    let mut browser_processes: Vec<_> = processes
        .into_iter()
        .filter_map(|process| process_to_browser_process(process, false, None))
        .filter(|process| browser_filter_matches(process.browser.clone(), process.pid, &filter))
        .collect();
    browser_processes.sort_by_key(|process| process.pid);

    let windows = if filter.include_windows {
        let mut windows: Vec<_> = list_windows()
            .into_iter()
            .filter_map(|window| window_to_browser_window(window, false, None))
            .filter(|window| browser_filter_matches(window.browser.clone(), window.pid, &filter))
            .collect();
        windows.sort_by(|a, b| {
            a.browser_name()
                .cmp(b.browser_name())
                .then(a.pid.cmp(&b.pid))
        });
        windows
    } else {
        Vec::new()
    };

    BrowserState {
        processes: browser_processes,
        windows,
        warnings,
    }
}

pub fn process_to_browser_process(
    process: ProcessInfo,
    tracked_by_server: bool,
    launch_id: Option<String>,
) -> Option<BrowserProcess> {
    let browser = browser_kind_for_process(&process)?;
    Some(BrowserProcess {
        pid: process.pid,
        browser,
        process_name: process.process_name,
        executable_path: process.exe_path,
        parent_pid: process.parent_pid,
        profile_hint: process.command_line.as_deref().and_then(profile_hint),
        command_line: process.command_line,
        tracked_by_server,
        launch_id,
        warnings: process.warnings,
    })
}

pub fn window_to_browser_window(
    window: WindowInfo,
    tracked_by_server: bool,
    launch_id: Option<String>,
) -> Option<BrowserWindow> {
    let browser = browser_kind_for_window(&window)?;
    Some(BrowserWindow {
        browser,
        hwnd: window.hwnd,
        hwnd_hex: window.hwnd_hex,
        pid: window.pid,
        process_name: window.process_name,
        executable_path: window.exe_path,
        tab_title_hint: (!window.title.is_empty()).then_some(window.title.clone()),
        profile_hint: None,
        title: window.title,
        class_name: window.class_name,
        visible: window.visible,
        minimized: window.minimized,
        cloaked: window.cloaked,
        foreground: window.foreground,
        rect: BrowserWindowRect {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        },
        tracked_by_server,
        launch_id,
        warnings: Vec::new(),
    })
}

pub fn browser_session_identity(window: &WindowInfo) -> Option<BrowserSessionIdentity> {
    let browser = browser_kind_for_window(window)?;
    Some(BrowserSessionIdentity {
        browser,
        pid: window.pid,
        hwnd_hex: Some(window.hwnd_hex.clone()),
        process_name: window.process_name.clone(),
        executable_path: window.exe_path.clone(),
        profile_hint: None,
        tab_title_hint: (!window.title.is_empty()).then_some(window.title.clone()),
    })
}

fn browser_filter_matches(browser: BrowserKind, pid: u32, filter: &BrowserStateFilter) -> bool {
    filter
        .browser
        .as_ref()
        .map(|wanted| *wanted == browser)
        .unwrap_or(true)
        && filter.pid.map(|wanted| wanted == pid).unwrap_or(true)
}

fn profile_hint(command_line: &str) -> Option<String> {
    command_line.split_whitespace().find_map(|part| {
        part.strip_prefix("--profile-directory=")
            .map(ToOwned::to_owned)
    })
}

impl BrowserWindow {
    fn browser_name(&self) -> &'static str {
        match self.browser {
            BrowserKind::Chrome => "chrome",
            BrowserKind::Edge => "edge",
            BrowserKind::Firefox => "firefox",
            BrowserKind::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_browser_process_names() {
        assert_eq!(
            browser_kind_for_process_name("chrome.exe"),
            Some(BrowserKind::Chrome)
        );
        assert_eq!(
            browser_kind_for_process_name("msedge.exe"),
            Some(BrowserKind::Edge)
        );
        assert_eq!(
            browser_kind_for_process_name("firefox.exe"),
            Some(BrowserKind::Firefox)
        );
        assert_eq!(browser_kind_for_process_name("notepad.exe"), None);
    }

    #[test]
    fn extracts_profile_hint_from_command_line() {
        assert_eq!(
            profile_hint(r#"chrome.exe --profile-directory=Profile 2"#),
            Some("Profile".into())
        );
    }
}
