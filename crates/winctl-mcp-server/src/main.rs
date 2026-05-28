mod tools;

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::schemars;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
    Json, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use winctl::{
    list_windows, revalidate_bound_window as revalidate_bound_record, select_window_for_bind,
    BoundWindow, ClickRequest, TypeTextRequest, WindowBindError, WindowControlError,
    WindowControlErrorCode, WindowIdentity, WindowInfo, WindowSelector,
};

#[derive(Default, Clone)]
pub struct AppState {
    pub bound: Arc<Mutex<HashMap<String, BoundWindow>>>,
    pub capture_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn bind_window(&self, selector: WindowSelector) -> Result<BoundWindow, WindowBindError> {
        let windows = list_windows();
        tracing::info!(
            windows_count = windows.len(),
            selector_id = ?selector.id,
            selector_hwnd = ?selector.hwnd,
            selector_pid = ?selector.pid,
            selector_process_name = ?selector.process_name,
            selector_has_title_contains = selector.title_contains.is_some(),
            selector_has_title_regex = selector.title_regex.is_some(),
            selector_class_name_contains = ?selector.class_name_contains,
            selector_exe_path_contains = ?selector.exe_path_contains,
            selector_exe_path_ends_with = ?selector.exe_path_ends_with,
            "bind selector requested"
        );

        let selected_match = match select_window_for_bind(&selector, &windows) {
            Ok(selected_match) => selected_match,
            Err(error) => {
                tracing::warn!(
                    error_code = ?error.code,
                    candidates = error.candidates.len(),
                    "bind selector rejected"
                );
                return Err(error);
            }
        };
        let selected = selected_match.window;
        let bound = BoundWindow {
            bound_id: selected.id.clone(),
            identity: WindowIdentity::from_window(&selected),
            window: selected.clone(),
            selector,
            match_score: selected_match.score,
            title_at_bind: selected.title.clone(),
            bound_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default(),
        };
        self.bound
            .lock()
            .expect("bound mutex poisoned")
            .insert(bound.bound_id.clone(), bound.clone());
        tracing::info!(
            bound_id = %bound.bound_id,
            hwnd = %bound.identity.hwnd_hex,
            pid = bound.identity.pid,
            process_name = ?bound.identity.process_name,
            exe_path = ?bound.identity.exe_path,
            match_score = bound.match_score,
            "bind succeeded"
        );
        Ok(bound)
    }

    pub fn revalidate_bound_window(
        &self,
        bound_id: &str,
    ) -> Result<WindowInfo, WindowControlError> {
        let windows = list_windows();
        self.revalidate_bound_window_against(bound_id, &windows)
    }

    pub fn revalidate_bound_window_against(
        &self,
        bound_id: &str,
        windows: &[WindowInfo],
    ) -> Result<WindowInfo, WindowControlError> {
        let bound = match self
            .bound
            .lock()
            .expect("bound mutex poisoned")
            .get(bound_id)
            .cloned()
        {
            Some(bound) => bound,
            None => {
                tracing::warn!(bound_id = %bound_id, "bound window revalidation failed: binding not found");
                return Err(WindowControlError {
                    code: WindowControlErrorCode::BindingNotFound,
                    bound_id: bound_id.into(),
                    message: "bound_id is not registered".into(),
                    expected: None,
                    actual: None,
                });
            }
        };

        let result = revalidate_bound_record(&bound, windows);
        match &result {
            Ok(window) => tracing::info!(
                bound_id = %bound_id,
                hwnd = %window.hwnd_hex,
                pid = window.pid,
                process_name = ?window.process_name,
                exe_path = ?window.exe_path,
                "bound window revalidated"
            ),
            Err(error) => tracing::warn!(
                bound_id = %bound_id,
                error_code = ?error.code,
                expected = ?error.expected,
                actual = ?error.actual,
                "bound window revalidation failed"
            ),
        }

        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct BoundIdRequest {
    pub bound_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WindowFromPointRequest {
    pub x: i32,
    pub y: i32,
    pub bound_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct DisplayScreenshotRequest {
    pub display_index: usize,
}

#[derive(Clone)]
pub struct WinctlMcpServer {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl WinctlMcpServer {
    pub fn new() -> Self {
        Self {
            state: AppState::default(),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "windows.list",
        description = "List visible and discoverable top-level Windows windows with HWND, PID, executable, class, title, and virtual desktop geometry."
    )]
    pub async fn windows_list(&self) -> Json<serde_json::Value> {
        Json(tools::windows::windows_list())
    }

    #[tool(
        name = "windows.find",
        description = "Find windows matching a selector and return scored diagnostics without binding or controlling them."
    )]
    pub async fn windows_find(
        &self,
        selector: Parameters<WindowSelector>,
    ) -> Json<serde_json::Value> {
        Json(tools::windows::windows_find(selector.0))
    }

    #[tool(
        name = "windows.bind",
        description = "Bind one strict window target by stable identity before any control action."
    )]
    pub async fn windows_bind(
        &self,
        selector: Parameters<WindowSelector>,
    ) -> Json<serde_json::Value> {
        Json(tools::windows::windows_bind(&self.state, selector.0))
    }

    #[tool(
        name = "windows.describe",
        description = "Describe an existing bound window record by bound_id."
    )]
    pub async fn windows_describe(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        Json(tools::windows::windows_describe(
            &self.state,
            request.0.bound_id,
        ))
    }

    #[tool(
        name = "windows.focus",
        description = "Focus a previously bound window after revalidating HWND, PID, and executable identity."
    )]
    pub async fn windows_focus(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        Json(tools::windows::windows_focus(
            &self.state,
            request.0.bound_id,
        ))
    }

    #[tool(
        name = "windows.window_from_point",
        description = "Resolve a screen point to top-level and child window diagnostics, optionally relative to a bound target."
    )]
    pub async fn windows_window_from_point(
        &self,
        request: Parameters<WindowFromPointRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::windows::windows_window_from_point(
            &self.state,
            request.x,
            request.y,
            request.bound_id,
        ))
    }

    #[tool(
        name = "windows.monitors",
        description = "List monitor geometry, DPI scale, primary monitor flag, and total virtual desktop bounds."
    )]
    pub async fn windows_monitors(&self) -> Json<serde_json::Value> {
        Json(tools::windows::windows_monitors())
    }

    #[tool(
        name = "input.click",
        description = "Click a bound window coordinate after identity revalidation and window-from-point preflight."
    )]
    pub async fn input_click(&self, request: Parameters<ClickRequest>) -> Json<serde_json::Value> {
        Json(tools::input::input_click(&self.state, request.0))
    }

    #[tool(
        name = "input.type_text",
        description = "Focus a bound window and type Unicode text with SendInput after identity revalidation."
    )]
    pub async fn input_type_text(
        &self,
        request: Parameters<TypeTextRequest>,
    ) -> Json<serde_json::Value> {
        Json(tools::input::input_type_text(&self.state, request.0))
    }

    #[tool(
        name = "capture.screenshot_window",
        description = "Capture a screenshot of a bound window and return exact virtual desktop region metadata."
    )]
    pub async fn screenshot_window(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        Json(tools::capture::screenshot_window(
            &self.state,
            request.0.bound_id,
        ))
    }

    #[tool(
        name = "capture.screenshot_display",
        description = "Capture a screenshot of a display by zero-based monitor index and return exact virtual desktop region metadata."
    )]
    pub async fn screenshot_display(
        &self,
        request: Parameters<DisplayScreenshotRequest>,
    ) -> Json<serde_json::Value> {
        Json(tools::capture::screenshot_display(
            &self.state,
            request.0.display_index,
        ))
    }
}

impl Default for WinctlMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for WinctlMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Strict Windows control tools. Bind a window before focus, click, type, or window capture actions."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();
    if tools::capture::is_capture_helper_mode() {
        tracing::info!("winctl capture helper starting");
        return tools::capture::run_capture_helper();
    }

    tracing::info!("winctl-mcp-server starting on stdio");
    let service = WinctlMcpServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: &str, hwnd: isize, pid: u32, exe_path: &str) -> winctl::WindowInfo {
        winctl::WindowInfo {
            id: id.into(),
            hwnd,
            hwnd_hex: format!("0x{hwnd:016x}"),
            pid,
            tid: 1,
            process_name: Some("app.exe".into()),
            exe_path: Some(exe_path.into()),
            title: "App".into(),
            class_name: "AppWindow".into(),
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
        }
    }

    fn bound(window: winctl::WindowInfo) -> BoundWindow {
        BoundWindow {
            bound_id: window.id.clone(),
            identity: WindowIdentity::from_window(&window),
            window: window.clone(),
            selector: WindowSelector {
                hwnd: Some(window.hwnd_hex.clone()),
                ..Default::default()
            },
            match_score: 100,
            title_at_bind: window.title.clone(),
            bound_at_unix_ms: 1,
        }
    }

    #[test]
    fn revalidate_bound_window_rejects_identity_mismatch() {
        let state = AppState::default();
        let original = window("hwnd:0x1", 1, 10, "C:/app.exe");
        state
            .bound
            .lock()
            .unwrap()
            .insert(original.id.clone(), bound(original.clone()));

        let current = vec![window("hwnd:0x1", 1, 11, "C:/other.exe")];
        let err = state
            .revalidate_bound_window_against(&original.id, &current)
            .unwrap_err();

        assert_eq!(err.code, winctl::WindowControlErrorCode::IdentityMismatch);
    }

    #[test]
    fn server_registers_first_milestone_tools() {
        let server = WinctlMcpServer::new();
        let mut names: Vec<_> = server
            .tool_router
            .list_all()
            .iter()
            .map(|tool| tool.name.as_ref().to_owned())
            .collect();
        names.sort();

        assert_eq!(
            names,
            vec![
                "capture.screenshot_display",
                "capture.screenshot_window",
                "input.click",
                "input.type_text",
                "windows.bind",
                "windows.describe",
                "windows.find",
                "windows.focus",
                "windows.list",
                "windows.monitors",
                "windows.window_from_point",
            ]
        );
    }
}
