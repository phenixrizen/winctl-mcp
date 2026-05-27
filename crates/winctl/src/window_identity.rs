use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowInfo {
    pub id: String,
    pub hwnd: isize,
    pub hwnd_hex: String,
    pub pid: u32,
    pub tid: u32,
    pub process_name: Option<String>,
    pub exe_path: Option<String>,
    pub title: String,
    pub class_name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
    pub enabled: bool,
    pub foreground: bool,
    pub minimized: bool,
    pub cloaked: bool,
    pub top_level: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowIdentity {
    pub hwnd: isize,
    pub hwnd_hex: String,
    pub pid: u32,
    pub process_name: Option<String>,
    pub exe_path: Option<String>,
}

impl WindowIdentity {
    pub fn from_window(window: &WindowInfo) -> Self {
        Self {
            hwnd: window.hwnd,
            hwnd_hex: window.hwnd_hex.clone(),
            pid: window.pid,
            process_name: window.process_name.clone(),
            exe_path: window.exe_path.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BoundWindow {
    pub bound_id: String,
    pub identity: WindowIdentity,
    pub window: WindowInfo,
    pub selector: WindowSelector,
    pub match_score: i32,
    pub title_at_bind: String,
    pub bound_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct WindowSelector {
    pub id: Option<String>,
    pub hwnd: Option<String>,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub exe_path_contains: Option<String>,
    pub exe_path_ends_with: Option<String>,
    pub title_contains: Option<String>,
    pub title_regex: Option<String>,
    pub class_name_contains: Option<String>,
    pub must_be_visible: Option<bool>,
    pub allow_minimized: Option<bool>,
    pub allow_cloaked: Option<bool>,
}

impl WindowSelector {
    pub fn is_strong_selector(&self) -> bool {
        self.id.is_some()
            || self.hwnd.is_some()
            || self.pid.is_some()
            || self.process_name.is_some()
            || self.exe_path_contains.is_some()
            || self.exe_path_ends_with.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScoredWindow {
    pub score: i32,
    pub window: WindowInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowFromPointResult {
    pub screen_x: i32,
    pub screen_y: i32,
    pub top_level: Option<WindowInfo>,
    pub child: Option<WindowInfo>,
    pub belongs_to_bound_window: Option<bool>,
}
