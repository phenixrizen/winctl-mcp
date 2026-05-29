mod tools;

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json as AxumJson, Router};
use rmcp::schemars;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::{
        stdio, streamable_http_server::session::local::LocalSessionManager,
        StreamableHttpServerConfig, StreamableHttpService,
    },
    Json, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use tracing_subscriber::fmt::MakeWriter;
use winctl::{
    list_windows, revalidate_bound_window as revalidate_bound_record, select_window_for_bind,
    BoundWindow, ClickRequest, DelayRequest, DoubleClickRequest, DragRequest, KeyRequest,
    MouseMoveRequest, ProcessLaunchResult, ProcessLaunchSpec, ScrollRequest, ShortcutRequest,
    TypeTextRequest, WindowBindError, WindowControlError, WindowControlErrorCode, WindowIdentity,
    WindowInfo, WindowSelector,
};

#[derive(Clone)]
pub struct AppState {
    pub bound: Arc<Mutex<HashMap<String, BoundWindow>>>,
    pub capture_lock: Arc<Mutex<()>>,
    pub capture_dir: Arc<PathBuf>,
    pub launched: Arc<Mutex<HashMap<String, TrackedProcess>>>,
    launch_counter: Arc<AtomicU64>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::with_capture_dir(tools::capture::default_capture_dir())
    }
}

impl AppState {
    pub fn with_capture_dir(capture_dir: PathBuf) -> Self {
        Self {
            bound: Arc::new(Mutex::new(HashMap::new())),
            capture_lock: Arc::new(Mutex::new(())),
            capture_dir: Arc::new(capture_dir),
            launched: Arc::new(Mutex::new(HashMap::new())),
            launch_counter: Arc::new(AtomicU64::new(1)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TrackedProcess {
    pub launch_id: String,
    pub pid: u32,
    pub exe: String,
    pub executable_path: Option<String>,
    pub process_name: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub launch_time_unix_ms: u64,
    pub command_line: String,
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

    pub fn track_launch(
        &self,
        spec: &ProcessLaunchSpec,
        launch: &ProcessLaunchResult,
    ) -> TrackedProcess {
        let launch_id = format!(
            "launch-{}",
            self.launch_counter.fetch_add(1, Ordering::Relaxed)
        );
        let tracked = TrackedProcess {
            launch_id: launch_id.clone(),
            pid: launch.pid,
            exe: spec.exe.clone(),
            executable_path: launch.executable_path.clone(),
            process_name: launch.process_name.clone(),
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            launch_time_unix_ms: launch.launch_time_unix_ms,
            command_line: winctl::windows_argv_command_line(&spec.exe, &spec.args),
        };
        self.launched
            .lock()
            .expect("launched process mutex poisoned")
            .insert(launch_id, tracked.clone());
        tracked
    }

    pub fn tracked_by_pid(&self, pid: u32) -> Option<TrackedProcess> {
        self.launched
            .lock()
            .expect("launched process mutex poisoned")
            .values()
            .find(|tracked| tracked.pid == pid)
            .cloned()
    }

    pub fn tracked_by_launch_id(&self, launch_id: &str) -> Option<TrackedProcess> {
        self.launched
            .lock()
            .expect("launched process mutex poisoned")
            .get(launch_id)
            .cloned()
    }

    pub fn forget_launch(&self, launch_id: &str) {
        self.launched
            .lock()
            .expect("launched process mutex poisoned")
            .remove(launch_id);
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

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessLaunchRequest {
    pub exe: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub wait_for_window: bool,
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub allow_child_process_windows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ProcessListRequest {
    pub name_contains: Option<String>,
    pub exe_path_contains: Option<String>,
    #[serde(default)]
    pub include_windows: bool,
    #[serde(default)]
    pub only_mcp_launched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessDescribeRequest {
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessKillRequest {
    pub pid: Option<u32>,
    pub launch_id: Option<String>,
    #[serde(default = "default_force")]
    pub force: bool,
    #[serde(default)]
    pub kill_tree: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ProcessWaitForExitRequest {
    pub pid: Option<u32>,
    pub launch_id: Option<String>,
    pub timeout_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WaitForWindowRequest {
    pub pid: Option<u32>,
    pub launch_id: Option<String>,
    pub timeout_ms: Option<u64>,
    pub title_contains: Option<String>,
    pub class_name_contains: Option<String>,
    #[serde(default)]
    pub allow_child_process_windows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WaitForStateRequest {
    pub bound_id: String,
    pub timeout_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
    pub visible: Option<bool>,
    pub foreground: Option<bool>,
    pub minimized: Option<bool>,
    pub cloaked: Option<bool>,
    pub title_contains: Option<String>,
    pub class_name_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WindowImageChangeWaitRequest {
    pub bound_id: String,
    pub timeout_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiSnapshotRequest {
    pub bound_id: String,
    pub max_depth: Option<usize>,
    pub max_elements: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiFindRequest {
    pub bound_id: String,
    pub selector: winctl::UiElementSelector,
    pub max_depth: Option<usize>,
    pub max_elements: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiResolveRequest {
    pub bound_id: String,
    pub element_ref: String,
    pub max_depth: Option<usize>,
    pub max_elements: Option<usize>,
}

fn default_force() -> bool {
    true
}

#[derive(Clone)]
pub struct WinctlMcpServer {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl WinctlMcpServer {
    pub fn new() -> Self {
        Self::with_state(AppState::default())
    }

    pub fn with_state(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "server.ping",
        description = "Return a minimal health response without touching Win32 APIs."
    )]
    pub async fn server_ping(&self) -> Json<serde_json::Value> {
        tracing::info!("server.ping requested");
        Json(serde_json::json!({"ok": true, "pong": true}))
    }

    #[tool(
        name = "uia.snapshot",
        description = "Capture a UI Automation tree for a bound window with element roles, names, automation IDs, bounds, state, hierarchy, and stable element references."
    )]
    pub async fn uia_snapshot(
        &self,
        request: Parameters<UiSnapshotRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("uia.snapshot", move || {
            tools::uia::uia_snapshot(&state, request)
        })
        .await
    }

    #[tool(
        name = "uia.find",
        description = "Find UI Automation elements in a fresh bound-window snapshot by semantic selector fields."
    )]
    pub async fn uia_find(&self, request: Parameters<UiFindRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("uia.find", move || tools::uia::uia_find(&state, request)).await
    }

    #[tool(
        name = "uia.resolve",
        description = "Revalidate a UI Automation element reference path against the current bound window snapshot."
    )]
    pub async fn uia_resolve(
        &self,
        request: Parameters<UiResolveRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("uia.resolve", move || {
            tools::uia::uia_resolve(&state, request)
        })
        .await
    }

    #[tool(
        name = "windows.list",
        description = "List visible and discoverable top-level Windows windows with HWND, PID, executable, class, title, and virtual desktop geometry."
    )]
    pub async fn windows_list(&self) -> Json<serde_json::Value> {
        run_blocking_tool("windows.list", tools::windows::windows_list).await
    }

    #[tool(
        name = "windows.find",
        description = "Find windows matching a selector and return scored diagnostics without binding or controlling them."
    )]
    pub async fn windows_find(
        &self,
        selector: Parameters<WindowSelector>,
    ) -> Json<serde_json::Value> {
        let selector = selector.0;
        run_blocking_tool("windows.find", move || {
            tools::windows::windows_find(selector)
        })
        .await
    }

    #[tool(
        name = "windows.bind",
        description = "Bind one strict window target by stable identity before any control action."
    )]
    pub async fn windows_bind(
        &self,
        selector: Parameters<WindowSelector>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let selector = selector.0;
        run_blocking_tool("windows.bind", move || {
            tools::windows::windows_bind(&state, selector)
        })
        .await
    }

    #[tool(
        name = "windows.describe",
        description = "Describe an existing bound window record by bound_id."
    )]
    pub async fn windows_describe(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.describe", move || {
            tools::windows::windows_describe(&state, bound_id)
        })
        .await
    }

    #[tool(
        name = "windows.focus",
        description = "Focus a previously bound window after revalidating HWND, PID, and executable identity."
    )]
    pub async fn windows_focus(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.focus", move || {
            tools::windows::windows_focus(&state, bound_id)
        })
        .await
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
        let state = self.state.clone();
        run_blocking_tool("windows.window_from_point", move || {
            tools::windows::windows_window_from_point(
                &state,
                request.x,
                request.y,
                request.bound_id,
            )
        })
        .await
    }

    #[tool(
        name = "windows.monitors",
        description = "List monitor geometry, DPI scale, primary monitor flag, and total virtual desktop bounds."
    )]
    pub async fn windows_monitors(&self) -> Json<serde_json::Value> {
        run_blocking_tool("windows.monitors", tools::windows::windows_monitors).await
    }

    #[tool(
        name = "windows.wait_for_window",
        description = "Wait for visible top-level window candidates owned by a PID or MCP launch ID without title-only selection."
    )]
    pub async fn windows_wait_for_window(
        &self,
        request: Parameters<WaitForWindowRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("windows.wait_for_window", move || {
            tools::process::windows_wait_for_window(&state, request)
        })
        .await
    }

    #[tool(
        name = "windows.wait_for_state",
        description = "Wait for a bound window to satisfy state, foreground, title, or class conditions after identity revalidation."
    )]
    pub async fn windows_wait_for_state(
        &self,
        request: Parameters<WaitForStateRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("windows.wait_for_state", move || {
            tools::windows::windows_wait_for_state(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.launch",
        description = "Launch a Windows executable via CreateProcessW and optionally wait for visible PID-owned window candidates."
    )]
    pub async fn process_launch(
        &self,
        request: Parameters<ProcessLaunchRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.launch", move || {
            tools::process::process_launch(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.list",
        description = "List Windows process metadata, with optional top-level window candidates and MCP-launched process markers."
    )]
    pub async fn process_list(
        &self,
        request: Parameters<ProcessListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.list", move || {
            tools::process::process_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.describe",
        description = "Describe one process with executable metadata, tracked launch status, children, and top-level windows."
    )]
    pub async fn process_describe(
        &self,
        request: Parameters<ProcessDescribeRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.describe", move || {
            tools::process::process_describe(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.kill",
        description = "Terminate only a process launched and tracked by this MCP server session."
    )]
    pub async fn process_kill(
        &self,
        request: Parameters<ProcessKillRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.kill", move || {
            tools::process::process_kill(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.wait_for_exit",
        description = "Wait for a process identified by PID or MCP launch ID to exit and return lifecycle timing metadata."
    )]
    pub async fn process_wait_for_exit(
        &self,
        request: Parameters<ProcessWaitForExitRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.wait_for_exit", move || {
            tools::process::process_wait_for_exit(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.click",
        description = "Click a bound window coordinate after identity revalidation and window-from-point preflight."
    )]
    pub async fn input_click(&self, request: Parameters<ClickRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.click", move || {
            tools::input::input_click(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.mouse_move",
        description = "Move the mouse to a bound window coordinate after identity revalidation and point preflight."
    )]
    pub async fn input_mouse_move(
        &self,
        request: Parameters<MouseMoveRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.mouse_move", move || {
            tools::input::input_mouse_move(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.double_click",
        description = "Double-click a bound window coordinate after identity revalidation and point preflight."
    )]
    pub async fn input_double_click(
        &self,
        request: Parameters<DoubleClickRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.double_click", move || {
            tools::input::input_double_click(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.drag",
        description = "Drag between two bound window coordinates after identity revalidation and point preflight."
    )]
    pub async fn input_drag(&self, request: Parameters<DragRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.drag", move || {
            tools::input::input_drag(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.scroll",
        description = "Scroll at a bound window coordinate after identity revalidation and point preflight."
    )]
    pub async fn input_scroll(
        &self,
        request: Parameters<ScrollRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.scroll", move || {
            tools::input::input_scroll(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.key_down",
        description = "Focus a bound window and dispatch a virtual key-down event after identity revalidation."
    )]
    pub async fn input_key_down(&self, request: Parameters<KeyRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.key_down", move || {
            tools::input::input_key_down(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.key_up",
        description = "Focus a bound window and dispatch a virtual key-up event after identity revalidation."
    )]
    pub async fn input_key_up(&self, request: Parameters<KeyRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.key_up", move || {
            tools::input::input_key_up(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.shortcut",
        description = "Focus a bound window and dispatch a virtual-key shortcut after identity revalidation."
    )]
    pub async fn input_shortcut(
        &self,
        request: Parameters<ShortcutRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.shortcut", move || {
            tools::input::input_shortcut(&state, request)
        })
        .await
    }

    #[tool(
        name = "input.delay",
        description = "Wait for a bounded number of milliseconds and return timing metadata for replay manifests."
    )]
    pub async fn input_delay(&self, request: Parameters<DelayRequest>) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("input.delay", move || tools::input::input_delay(request)).await
    }

    #[tool(
        name = "input.type_text",
        description = "Focus a bound window and type Unicode text with SendInput after identity revalidation."
    )]
    pub async fn input_type_text(
        &self,
        request: Parameters<TypeTextRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("input.type_text", move || {
            tools::input::input_type_text(&state, request)
        })
        .await
    }

    #[tool(
        name = "capture.screenshot_window",
        description = "Capture a screenshot of a bound window and return exact virtual desktop region metadata."
    )]
    pub async fn screenshot_window(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("capture.screenshot_window", move || {
            tools::capture::screenshot_window(&state, bound_id)
        })
        .await
    }

    #[tool(
        name = "capture.screenshot_display",
        description = "Capture a screenshot of a display by zero-based monitor index and return exact virtual desktop region metadata."
    )]
    pub async fn screenshot_display(
        &self,
        request: Parameters<DisplayScreenshotRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let display_index = request.0.display_index;
        run_blocking_tool("capture.screenshot_display", move || {
            tools::capture::screenshot_display(&state, display_index)
        })
        .await
    }

    #[tool(
        name = "capture.wait_for_window_image_change",
        description = "Poll bound-window screenshots until the image bytes change, returning replay-safe capture diagnostics."
    )]
    pub async fn wait_for_window_image_change(
        &self,
        request: Parameters<WindowImageChangeWaitRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("capture.wait_for_window_image_change", move || {
            tools::capture::wait_for_window_image_change(&state, request)
        })
        .await
    }
}

async fn run_blocking_tool<F>(tool_name: &'static str, operation: F) -> Json<serde_json::Value>
where
    F: FnOnce() -> serde_json::Value + Send + 'static,
{
    match tokio::task::spawn_blocking(operation).await {
        Ok(value) => Json(value),
        Err(error) => {
            tracing::error!(
                tool_name = tool_name,
                error = %error,
                "blocking MCP tool task failed"
            );
            Json(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "internal_task_failed",
                    "message": format!("{tool_name} worker failed: {error}")
                }
            }))
        }
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

fn main() {
    install_panic_hook();
    let cli = match Cli::parse(std::env::args_os().skip(1)) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = writeln!(io::stderr(), "fatal startup error: {error:#}");
            process::exit(2);
        }
    };

    if let Err(error) = init_tracing(cli.log_file.clone()) {
        let _ = writeln!(io::stderr(), "fatal startup error: {error:#}");
        process::exit(1);
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "failed to initialize Tokio runtime");
            process::exit(1);
        }
    };

    match runtime.block_on(run(cli)) {
        Ok(()) => {}
        Err(error) => {
            tracing::error!(error = ?error, "winctl-mcp-server fatal error");
            process::exit(1);
        }
    }
}

async fn run(_cli: Cli) -> anyhow::Result<()> {
    if tools::capture::is_capture_helper_mode() {
        tracing::info!("winctl capture helper starting");
        return tools::capture::run_capture_helper();
    }

    match _cli.command {
        Command::SelfTest(command) => run_self_test(command),
        Command::Serve(config) => {
            let state = config
                .capture_dir
                .clone()
                .map(AppState::with_capture_dir)
                .unwrap_or_default();
            match config.transport {
                TransportMode::Stdio => run_mcp_stdio(state).await,
                TransportMode::Http => run_mcp_http(config, state).await,
            }
        }
    }
}

async fn run_mcp_stdio(state: AppState) -> anyhow::Result<()> {
    tools::capture::ensure_capture_dir(state.capture_dir.as_ref())?;
    tracing::info!(
        capture_dir = %state.capture_dir.as_ref().display(),
        "winctl-mcp-server starting on stdio"
    );
    let service = WinctlMcpServer::with_state(state)
        .serve(stdio())
        .await
        .context("failed to initialize MCP stdio service")?;
    tracing::info!("winctl-mcp-server stdio service initialized");
    service
        .waiting()
        .await
        .context("MCP stdio service failed")?;
    tracing::info!("winctl-mcp-server stdio service stopped");
    Ok(())
}

async fn run_mcp_http(config: ServeConfig, state: AppState) -> anyhow::Result<()> {
    validate_http_config(config.listen, config.auth_token.as_deref())?;
    tools::capture::ensure_capture_dir(state.capture_dir.as_ref())?;
    tracing::info!(
        listen = %config.listen,
        capture_dir = %state.capture_dir.as_ref().display(),
        auth_required = !config.listen.ip().is_loopback() || config.auth_token.is_some(),
        "winctl-mcp-server starting on HTTP"
    );

    let session_manager = Arc::new(LocalSessionManager::default());
    let service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(WinctlMcpServer::with_state(state.clone()))
        },
        session_manager,
        StreamableHttpServerConfig {
            sse_keep_alive: None,
            ..Default::default()
        },
    );
    let auth_state = HttpAuthState {
        required_token: if config.auth_token.is_some() || !config.listen.ip().is_loopback() {
            config.auth_token.clone()
        } else {
            None
        },
    };
    let mcp_router =
        Router::new()
            .route_service("/mcp", service)
            .route_layer(middleware::from_fn_with_state(
                auth_state,
                require_bearer_auth,
            ));
    let app = Router::new()
        .route("/healthz", get(healthz))
        .merge(mcp_router);
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("failed to bind HTTP listener {}", config.listen))?;
    tracing::info!(
        listen = %listener.local_addr().unwrap_or(config.listen),
        "winctl-mcp-server HTTP listener initialized"
    );
    axum::serve(listener, app)
        .await
        .context("HTTP MCP server failed")
}

#[derive(Clone)]
struct HttpAuthState {
    required_token: Option<String>,
}

async fn healthz() -> impl IntoResponse {
    tracing::info!("HTTP health check requested");
    AxumJson(serde_json::json!({
        "ok": true,
        "service": "winctl-mcp-server"
    }))
}

async fn require_bearer_auth(
    State(state): State<HttpAuthState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let Some(required_token) = state.required_token else {
        return next.run(request).await;
    };
    let authorized = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .map(|header| header == format!("Bearer {required_token}"))
        .unwrap_or(false);
    if authorized {
        next.run(request).await
    } else {
        tracing::warn!("HTTP MCP request rejected by bearer auth");
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }
}

fn validate_http_config(listen: SocketAddr, auth_token: Option<&str>) -> anyhow::Result<()> {
    if !listen.ip().is_loopback() && auth_token.filter(|token| !token.is_empty()).is_none() {
        anyhow::bail!(
            "HTTP listen address {listen} is not loopback; provide --auth-token before exposing MCP tools"
        );
    }
    Ok(())
}

fn init_tracing(log_file: Option<PathBuf>) -> anyhow::Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let log_file = match log_file {
        Some(path) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .with_context(|| format!("failed to open log file {}", path.display()))?;
            Some(Arc::new(Mutex::new(file)))
        }
        None => None,
    };

    tracing_subscriber::fmt()
        .with_writer(StderrAndFileMakeWriter { log_file })
        .with_env_filter(env_filter)
        .with_timer(tracing_subscriber::fmt::time::SystemTime)
        .with_level(true)
        .init();
    Ok(())
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let _ = writeln!(io::stderr(), "winctl-mcp-server panic: {panic_info}");
        let _ = writeln!(io::stderr(), "backtrace: {backtrace}");
        tracing::error!(panic = %panic_info, backtrace = %backtrace, "winctl-mcp-server panic");
    }));
}

fn run_self_test(command: SelfTestCommand) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "winctl-mcp-server self-test")?;
    writeln!(stdout, "stdout: self-test output only")?;
    writeln!(
        stdout,
        "mcp_stdio: stdout reserved for JSON-RPC in MCP mode"
    )?;
    match command {
        SelfTestCommand::Basic => {}
        SelfTestCommand::WindowsList => {
            let windows = list_windows();
            writeln!(stdout, "windows-list: {} windows", windows.len())?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct Cli {
    command: Command,
    log_file: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            command: Command::Serve(ServeConfig::stdio()),
            log_file: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Command {
    Serve(ServeConfig),
    SelfTest(SelfTestCommand),
}

#[derive(Debug, Clone)]
struct ServeConfig {
    transport: TransportMode,
    listen: SocketAddr,
    auth_token: Option<String>,
    capture_dir: Option<PathBuf>,
}

impl ServeConfig {
    fn stdio() -> Self {
        Self {
            transport: TransportMode::Stdio,
            listen: default_http_listen(),
            auth_token: None,
            capture_dir: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportMode {
    Stdio,
    Http,
}

#[derive(Debug, Clone, Copy)]
enum SelfTestCommand {
    Basic,
    WindowsList,
}

impl Cli {
    fn parse<I>(args: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut cli = Self::default();
        let args: Vec<_> = args.into_iter().collect();
        let mut index = 0;
        let mut command_seen = false;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--log-file" {
                index += 1;
                let Some(path) = args.get(index) else {
                    anyhow::bail!("--log-file requires a path");
                };
                cli.log_file = Some(PathBuf::from(path));
            } else if arg == "--self-test" {
                cli.command = Command::SelfTest(SelfTestCommand::Basic);
                command_seen = true;
            } else if arg == "self-test" {
                if command_seen {
                    anyhow::bail!("only one command may be provided");
                }
                index += 1;
                let Some(command) = args.get(index) else {
                    anyhow::bail!(
                        "self-test requires a command, for example: self-test windows-list"
                    );
                };
                cli.command = Command::SelfTest(parse_self_test_command(command)?);
                command_seen = true;
            } else if arg == "serve" {
                if command_seen {
                    anyhow::bail!("only one command may be provided");
                }
                let (serve, next_index, log_file) = parse_serve_args(&args, index + 1)?;
                cli.command = Command::Serve(serve);
                if let Some(log_file) = log_file {
                    cli.log_file = Some(log_file);
                }
                index = next_index;
                command_seen = true;
                continue;
            } else {
                anyhow::bail!("unknown argument: {}", arg.to_string_lossy());
            }
            index += 1;
        }

        Ok(cli)
    }
}

fn parse_serve_args(
    args: &[OsString],
    mut index: usize,
) -> anyhow::Result<(ServeConfig, usize, Option<PathBuf>)> {
    let mut config = ServeConfig::stdio();
    let mut log_file = None;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--transport" {
            index += 1;
            let Some(value) = args.get(index) else {
                anyhow::bail!("--transport requires stdio or http");
            };
            config.transport = parse_transport(value)?;
        } else if arg == "--listen" {
            index += 1;
            let Some(value) = args.get(index) else {
                anyhow::bail!("--listen requires an address");
            };
            config.listen = value
                .to_string_lossy()
                .parse()
                .with_context(|| format!("invalid listen address {}", value.to_string_lossy()))?;
        } else if arg == "--auth-token" {
            index += 1;
            let Some(value) = args.get(index) else {
                anyhow::bail!("--auth-token requires a value");
            };
            config.auth_token = Some(value.to_string_lossy().to_string());
        } else if arg == "--capture-dir" {
            index += 1;
            let Some(path) = args.get(index) else {
                anyhow::bail!("--capture-dir requires a path");
            };
            config.capture_dir = Some(PathBuf::from(path));
        } else if arg == "--log-file" {
            index += 1;
            let Some(path) = args.get(index) else {
                anyhow::bail!("--log-file requires a path");
            };
            log_file = Some(PathBuf::from(path));
        } else {
            anyhow::bail!("unknown serve argument: {}", arg.to_string_lossy());
        }
        index += 1;
    }
    Ok((config, index, log_file))
}

fn parse_transport(value: &OsString) -> anyhow::Result<TransportMode> {
    match value.to_string_lossy().as_ref() {
        "stdio" => Ok(TransportMode::Stdio),
        "http" => Ok(TransportMode::Http),
        other => anyhow::bail!("unsupported transport {other}; expected stdio or http"),
    }
}

fn parse_self_test_command(value: &OsString) -> anyhow::Result<SelfTestCommand> {
    match value.to_string_lossy().as_ref() {
        "windows-list" => Ok(SelfTestCommand::WindowsList),
        other => anyhow::bail!("unsupported self-test {other}; expected windows-list"),
    }
}

fn default_http_listen() -> SocketAddr {
    SocketAddr::new(IpAddr::from([127, 0, 0, 1]), 8765)
}

#[derive(Clone)]
struct StderrAndFileMakeWriter {
    log_file: Option<Arc<Mutex<File>>>,
}

struct StderrAndFileWriter {
    log_file: Option<Arc<Mutex<File>>>,
}

impl<'a> MakeWriter<'a> for StderrAndFileMakeWriter {
    type Writer = StderrAndFileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        StderrAndFileWriter {
            log_file: self.log_file.clone(),
        }
    }
}

impl Write for StderrAndFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::stderr().write_all(buf)?;
        if let Some(file) = &self.log_file {
            let mut file = file
                .lock()
                .map_err(|_| io::Error::other("log file mutex poisoned"))?;
            file.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()?;
        if let Some(file) = &self.log_file {
            let mut file = file
                .lock()
                .map_err(|_| io::Error::other("log file mutex poisoned"))?;
            file.flush()?;
        }
        Ok(())
    }
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
                "capture.wait_for_window_image_change",
                "input.click",
                "input.delay",
                "input.double_click",
                "input.drag",
                "input.key_down",
                "input.key_up",
                "input.mouse_move",
                "input.scroll",
                "input.shortcut",
                "input.type_text",
                "process.describe",
                "process.kill",
                "process.launch",
                "process.list",
                "process.wait_for_exit",
                "server.ping",
                "uia.find",
                "uia.resolve",
                "uia.snapshot",
                "windows.bind",
                "windows.describe",
                "windows.find",
                "windows.focus",
                "windows.list",
                "windows.monitors",
                "windows.wait_for_state",
                "windows.wait_for_window",
                "windows.window_from_point",
            ]
        );
    }

    #[test]
    fn process_kill_refuses_untracked_pid() {
        let state = AppState::default();
        let value = tools::process::process_kill(
            &state,
            ProcessKillRequest {
                pid: Some(12345),
                launch_id: None,
                force: true,
                kill_tree: false,
            },
        );
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "process_not_owned_by_server");
    }

    #[test]
    fn process_kill_refuses_tracked_pid_identity_mismatch() {
        let state = AppState::default();
        state.launched.lock().unwrap().insert(
            "launch-test".into(),
            TrackedProcess {
                launch_id: "launch-test".into(),
                pid: std::process::id(),
                exe: "C:/not-the-current-process.exe".into(),
                executable_path: Some("C:/not-the-current-process.exe".into()),
                process_name: Some("not-the-current-process.exe".into()),
                args: vec![],
                cwd: None,
                launch_time_unix_ms: 1,
                command_line: "C:/not-the-current-process.exe".into(),
            },
        );

        let value = tools::process::process_kill(
            &state,
            ProcessKillRequest {
                pid: None,
                launch_id: Some("launch-test".into()),
                force: true,
                kill_tree: false,
            },
        );

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "tracked_process_identity_mismatch");
    }

    #[test]
    fn http_non_loopback_requires_auth_token() {
        let addr: SocketAddr = "0.0.0.0:8765".parse().unwrap();
        assert!(validate_http_config(addr, None).is_err());
        assert!(validate_http_config(addr, Some("secret")).is_ok());
        let loopback: SocketAddr = "127.0.0.1:8765".parse().unwrap();
        assert!(validate_http_config(loopback, None).is_ok());
    }

    #[test]
    fn cli_parses_required_commands() {
        let cli = Cli::parse([
            OsString::from("serve"),
            OsString::from("--transport"),
            OsString::from("http"),
            OsString::from("--listen"),
            OsString::from("127.0.0.1:8765"),
            OsString::from("--capture-dir"),
            OsString::from("D:/winctl-captures"),
        ])
        .unwrap();
        match cli.command {
            Command::Serve(config) => {
                assert_eq!(config.transport, TransportMode::Http);
                assert_eq!(config.listen, default_http_listen());
                assert_eq!(
                    config.capture_dir,
                    Some(PathBuf::from("D:/winctl-captures"))
                );
            }
            _ => panic!("expected serve command"),
        }

        let cli =
            Cli::parse([OsString::from("self-test"), OsString::from("windows-list")]).unwrap();
        assert!(matches!(
            cli.command,
            Command::SelfTest(SelfTestCommand::WindowsList)
        ));
    }
}
