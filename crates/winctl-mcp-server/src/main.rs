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
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header::CONTENT_TYPE, HeaderMap, StatusCode, Uri};
use axum::middleware;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
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
    pub policy: Arc<SecurityPolicy>,
    pub launched: Arc<Mutex<HashMap<String, TrackedProcess>>>,
    pub memory: Arc<Mutex<winctl_memory::MemoryStore>>,
    pub macro_runtime: Arc<Mutex<tools::macros::MacroRuntimeState>>,
    pub recorder_runtime: Arc<Mutex<tools::recorder::RecorderRuntimeState>>,
    pub video_runtime: Arc<Mutex<tools::capture::VideoRuntimeState>>,
    pub control_runtime: Arc<Mutex<tools::control::ControlRuntimeState>>,
    launch_counter: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SecurityPolicy {
    pub filesystem_roots: Vec<PathBuf>,
    pub artifact_dir: Option<PathBuf>,
    pub enable_filesystem_mutation: bool,
    pub enable_clipboard_write: bool,
    pub enable_registry_mutation: bool,
    pub allow_private_network: bool,
    pub memory_mutation_enabled: bool,
    pub macro_execution_enabled: bool,
    pub macro_destructive_tools_allowed: bool,
    pub max_macro_runtime_ms: Option<u64>,
    pub max_macro_steps: Option<usize>,
    pub screenshot_retention_count: Option<usize>,
    pub embedding_model_path: Option<PathBuf>,
    pub embedding_dimension: Option<usize>,
    pub tool_allowlist: Vec<String>,
    pub tool_denylist: Vec<String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            filesystem_roots: env_paths("WINCTL_FS_ROOTS"),
            artifact_dir: std::env::var_os("WINCTL_ARTIFACT_DIR").map(PathBuf::from),
            enable_filesystem_mutation: env_flag("WINCTL_ENABLE_FILESYSTEM_MUTATION"),
            enable_clipboard_write: env_flag("WINCTL_ENABLE_CLIPBOARD_WRITE"),
            enable_registry_mutation: env_flag("WINCTL_ENABLE_REGISTRY_MUTATION"),
            allow_private_network: env_flag("WINCTL_ALLOW_PRIVATE_NETWORK"),
            memory_mutation_enabled: true,
            macro_execution_enabled: true,
            macro_destructive_tools_allowed: false,
            max_macro_runtime_ms: None,
            max_macro_steps: None,
            screenshot_retention_count: None,
            embedding_model_path: None,
            embedding_dimension: Some(winctl_memory::DEFAULT_EMBEDDING_DIM),
            tool_allowlist: Vec::new(),
            tool_denylist: Vec::new(),
        }
    }
}

impl SecurityPolicy {
    fn with_runtime_roots(mut self, capture_dir: &PathBuf) -> Self {
        self.filesystem_roots.push(capture_dir.clone());
        self.filesystem_roots.push(std::env::temp_dir());
        self.filesystem_roots = self
            .filesystem_roots
            .into_iter()
            .filter_map(|path| std::fs::canonicalize(&path).ok().or(Some(path)))
            .collect();
        self.filesystem_roots.sort();
        self.filesystem_roots.dedup();
        self
    }

    pub fn denies_tool(&self, tool_name: &str) -> bool {
        self.tool_denylist
            .iter()
            .any(|tool| tool.eq_ignore_ascii_case(tool_name))
            || (!self.tool_allowlist.is_empty()
                && !self
                    .tool_allowlist
                    .iter()
                    .any(|tool| tool.eq_ignore_ascii_case(tool_name)))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::with_capture_dir(tools::capture::default_capture_dir())
    }
}

impl AppState {
    pub fn with_capture_dir(capture_dir: PathBuf) -> Self {
        Self::with_capture_dir_and_memory(capture_dir, default_memory_store())
    }

    pub fn with_capture_dir_and_memory(
        capture_dir: PathBuf,
        memory_store: winctl_memory::MemoryStore,
    ) -> Self {
        Self::with_capture_dir_memory_policy(capture_dir, memory_store, SecurityPolicy::default())
    }

    pub fn with_capture_dir_memory_policy(
        capture_dir: PathBuf,
        memory_store: winctl_memory::MemoryStore,
        policy: SecurityPolicy,
    ) -> Self {
        let policy = policy.with_runtime_roots(&capture_dir);
        let control_runtime = Arc::new(Mutex::new(tools::control::ControlRuntimeState::default()));
        tools::control::configure_audit_log(&control_runtime, &capture_dir);
        tools::control::start_emergency_hotkey(control_runtime.clone());
        Self {
            bound: Arc::new(Mutex::new(HashMap::new())),
            capture_lock: Arc::new(Mutex::new(())),
            capture_dir: Arc::new(capture_dir),
            policy: Arc::new(policy),
            launched: Arc::new(Mutex::new(HashMap::new())),
            memory: Arc::new(Mutex::new(memory_store)),
            macro_runtime: Arc::new(Mutex::new(tools::macros::MacroRuntimeState::default())),
            recorder_runtime: Arc::new(
                Mutex::new(tools::recorder::RecorderRuntimeState::default()),
            ),
            video_runtime: Arc::new(Mutex::new(tools::capture::VideoRuntimeState::default())),
            control_runtime,
            launch_counter: Arc::new(AtomicU64::new(1)),
        }
    }
}

fn default_memory_store() -> winctl_memory::MemoryStore {
    match winctl_memory::MemoryStore::open_default() {
        Ok(store) => store,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to open default memory store; falling back to in-memory store"
            );
            winctl_memory::MemoryStore::open_in_memory()
                .expect("in-memory memory store initialization failed")
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

/// Identify a single bound window by its stable bound id.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct BoundIdRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
}

/// Identify the top-level window located at a screen coordinate.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WindowFromPointRequest {
    /// X screen-pixel coordinate of the point to hit-test.
    pub x: i32,
    /// Y screen-pixel coordinate of the point to hit-test.
    pub y: i32,
    /// Optional stable bound-window id returned by `windows.bind`; when set, the point is interpreted relative to that window rather than the screen.
    pub bound_id: Option<String>,
}

/// Capture a full-display screenshot.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct DisplayScreenshotRequest {
    /// Zero-based index of the display/monitor to capture, as enumerated by the display tools.
    pub display_index: usize,
}

/// Launch a process directly from an executable path.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessLaunchRequest {
    /// Path to the executable to launch.
    pub exe: String,
    /// Command-line arguments passed to the executable. Defaults to an empty list.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the new process. Defaults to the server's current directory when omitted.
    pub cwd: Option<String>,
    /// Environment variables to set for the new process, merged over the inherited environment.
    pub env: Option<HashMap<String, String>>,
    /// If true, block until a visible top-level window owned by the launched PID appears (or `timeout_ms` elapses).
    #[serde(default)]
    pub wait_for_window: bool,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Also consider windows owned by child processes of the target, not just the launched PID.
    #[serde(default)]
    pub allow_child_process_windows: bool,
}

/// Launch an application by URI, app id, or other resolved target.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct AppLaunchRequest {
    /// How to interpret `target`, e.g. an executable path, shell/URI target, or registered app id.
    pub mode: winctl::AppLaunchMode,
    /// The launch target, interpreted according to `mode`.
    pub target: String,
    /// Command-line arguments passed to the launched application. Defaults to an empty list.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the launched application. Defaults to the server's current directory when omitted.
    pub cwd: Option<String>,
    /// If true, block until a visible top-level window owned by the launched PID appears (or `timeout_ms` elapses).
    #[serde(default)]
    pub wait_for_window: bool,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Also consider windows owned by child processes of the target, not just the launched PID.
    #[serde(default)]
    pub allow_child_process_windows: bool,
}

/// List running processes, optionally filtered.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ProcessListRequest {
    /// Case-insensitive substring the process name must contain to be included.
    pub name_contains: Option<String>,
    /// Case-insensitive substring the process executable path must contain to be included.
    pub exe_path_contains: Option<String>,
    /// If true, include the list of top-level windows owned by each process.
    #[serde(default)]
    pub include_windows: bool,
    /// If true, restrict results to processes this server launched (those with an MCP launch id).
    #[serde(default)]
    pub only_mcp_launched: bool,
}

/// Describe a single process by PID.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessDescribeRequest {
    /// Target process id (PID).
    pub pid: u32,
}

/// Terminate a process by PID or launch id.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessKillRequest {
    /// Target process id (PID). Provide either `pid` or `launch_id`.
    pub pid: Option<u32>,
    /// MCP launch id returned by `process.launch`/`app.launch` for a process this server started. Provide either `pid` or `launch_id`.
    pub launch_id: Option<String>,
    /// If true, force-terminate the process instead of requesting a graceful close. Defaults to true.
    #[serde(default = "default_force")]
    pub force: bool,
    /// If true, also terminate the target's descendant processes (the whole process tree). Defaults to false.
    #[serde(default)]
    pub kill_tree: bool,
}

/// Wait for a process to exit.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ProcessWaitForExitRequest {
    /// Target process id (PID). Provide either `pid` or `launch_id`.
    pub pid: Option<u32>,
    /// MCP launch id returned by `process.launch`/`app.launch` for a process this server started. Provide either `pid` or `launch_id`.
    pub launch_id: Option<String>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Polling interval between checks, in milliseconds.
    pub poll_interval_ms: Option<u64>,
}

/// Wait for a top-level window owned by a process to appear.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WaitForWindowRequest {
    /// Target process id (PID). Provide either `pid` or `launch_id`.
    pub pid: Option<u32>,
    /// MCP launch id returned by `process.launch`/`app.launch` for a process this server started. Provide either `pid` or `launch_id`.
    pub launch_id: Option<String>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Case-insensitive substring the window title must contain.
    pub title_contains: Option<String>,
    /// Case-insensitive substring the window class must contain.
    pub class_name_contains: Option<String>,
    /// Also consider windows owned by child processes of the target, not just the launched PID.
    #[serde(default)]
    pub allow_child_process_windows: bool,
}

/// Wait until a bound window reaches a desired state.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WaitForStateRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Polling interval between checks, in milliseconds.
    pub poll_interval_ms: Option<u64>,
    /// If set, wait until the window's visible state matches this value.
    pub visible: Option<bool>,
    /// If set, wait until the window's foreground state matches this value.
    pub foreground: Option<bool>,
    /// If set, wait until the window's minimized state matches this value.
    pub minimized: Option<bool>,
    /// If set, wait until the window's DWM-cloaked state matches this value.
    pub cloaked: Option<bool>,
    /// Case-insensitive substring the window title must contain before the wait succeeds.
    pub title_contains: Option<String>,
    /// Case-insensitive substring the window class must contain before the wait succeeds.
    pub class_name_contains: Option<String>,
}

/// Move a bound window to a new screen position.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WindowMoveRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// New X position of the window's top-left corner, in screen pixels.
    pub x: i32,
    /// New Y position of the window's top-left corner, in screen pixels.
    pub y: i32,
}

/// Resize a bound window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WindowResizeRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// New window width, in pixels.
    pub width: i32,
    /// New window height, in pixels.
    pub height: i32,
}

/// List the top-level windows owned by a process.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WindowsForProcessRequest {
    /// Target process id (PID). Provide either `pid` or `launch_id`.
    pub pid: Option<u32>,
    /// MCP launch id returned by `process.launch`/`app.launch` for a process this server started. Provide either `pid` or `launch_id`.
    pub launch_id: Option<String>,
    /// Also consider windows owned by child processes of the target, not just the launched PID.
    #[serde(default)]
    pub include_child_process_windows: bool,
}

/// List detected browser processes/windows.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserListRequest {
    /// If set, restrict results to this browser kind (e.g. Chrome, Edge, Firefox).
    pub browser: Option<winctl::BrowserKind>,
    /// If set, restrict results to the browser instance with this process id (PID).
    pub pid: Option<u32>,
    /// If true, include the list of top-level windows for each browser process.
    #[serde(default)]
    pub include_windows: bool,
    /// If true, restrict results to browsers this server launched (those with an MCP launch id).
    #[serde(default)]
    pub only_mcp_launched: bool,
}

/// Describe a browser window/instance.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserDescribeRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: Option<String>,
    /// Target process id (PID) of the browser to describe.
    pub pid: Option<u32>,
    /// Raw window handle (HWND), as a string, of the browser window to describe.
    pub hwnd: Option<String>,
}

/// Wait for a browser window's title to change, indicating navigation.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserWaitForNavigationRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Case-insensitive substring the window title must contain for the wait to succeed.
    pub title_contains: Option<String>,
    /// Case-insensitive substring the window title must NOT contain for the wait to succeed.
    pub title_not_contains: Option<String>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Polling interval between checks, in milliseconds.
    pub poll_interval_ms: Option<u64>,
}

/// Assert properties of a browser window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserAssertRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// If set, assert the window belongs to this browser kind (e.g. Chrome, Edge, Firefox).
    pub browser: Option<winctl::BrowserKind>,
    /// Case-insensitive substring the window title must contain for the assertion to pass.
    pub title_contains: Option<String>,
    /// Case-insensitive substring the window class must contain for the assertion to pass.
    pub class_name_contains: Option<String>,
}

/// Extract readable content from a browser window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserExtractContentRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
}

/// Read the current text contents of the clipboard.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ClipboardReadRequest {
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_chars: Option<usize>,
}

/// Write text to the clipboard.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ClipboardWriteRequest {
    /// Text to place on the clipboard.
    pub text: String,
}

/// Read the contents of a file.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemReadRequest {
    /// Absolute path of the file to read.
    pub path: String,
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_bytes: Option<usize>,
}

/// List the entries of a directory.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemListRequest {
    /// Absolute path of the directory to list.
    pub path: String,
    /// If true, descend into subdirectories recursively. Defaults to false.
    #[serde(default)]
    pub recursive: bool,
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_entries: Option<usize>,
}

/// Search files under a root for a pattern.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemSearchRequest {
    /// Absolute path of the directory to search under.
    pub root: String,
    /// Pattern to match (file glob and/or content pattern as supported by the tool).
    pub pattern: String,
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_results: Option<usize>,
    /// Maximum size, in bytes, of an individual file that will be opened and scanned; larger files are skipped.
    pub max_file_bytes: Option<usize>,
}

/// Copy a file or directory.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemCopyRequest {
    /// Absolute source path to copy from.
    pub from: String,
    /// Absolute destination path to copy to.
    pub to: String,
    /// If true, overwrite the destination if it already exists. Defaults to false.
    #[serde(default)]
    pub overwrite: bool,
}

/// Move or rename a file or directory.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemMoveRequest {
    /// Absolute source path to move from.
    pub from: String,
    /// Absolute destination path to move to.
    pub to: String,
    /// If true, overwrite the destination if it already exists. Defaults to false.
    #[serde(default)]
    pub overwrite: bool,
}

/// Delete a file or directory.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemDeleteRequest {
    /// Absolute path of the file or directory to delete.
    pub path: String,
    /// If true, delete directories recursively along with their contents. Defaults to false.
    #[serde(default)]
    pub recursive: bool,
}

/// Export an artifact file to a destination.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ArtifactExportRequest {
    /// Absolute path of the artifact file to export.
    pub source_path: String,
    /// Absolute destination path to export to. Defaults to a server-chosen export location when omitted.
    pub destination_path: Option<String>,
}

/// List registry subkeys (and optionally values) under a key.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryListRequest {
    /// Registry hive, e.g. HKEY_LOCAL_MACHINE / HKEY_CURRENT_USER.
    pub hive: winctl::RegistryHive,
    /// Registry key path within the hive.
    pub path: String,
    /// If true, also return the values stored directly under the key, not just subkey names. Defaults to false.
    #[serde(default)]
    pub include_values: bool,
}

/// Read a registry value.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryReadRequest {
    /// Registry hive, e.g. HKEY_LOCAL_MACHINE / HKEY_CURRENT_USER.
    pub hive: winctl::RegistryHive,
    /// Registry key path within the hive.
    pub path: String,
    /// Value name to read. Omit or null to read the key's default (unnamed) value.
    pub name: Option<String>,
}

/// Write a registry value.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryWriteRequest {
    /// Registry hive, e.g. HKEY_LOCAL_MACHINE / HKEY_CURRENT_USER.
    pub hive: winctl::RegistryHive,
    /// Registry key path within the hive.
    pub path: String,
    /// Value name to write. Omit or null to write the key's default (unnamed) value.
    pub name: Option<String>,
    /// Registry value type, e.g. REG_SZ, REG_DWORD, REG_BINARY.
    pub kind: winctl::RegistryValueKind,
    /// The value data to write, interpreted according to `kind`.
    pub data: serde_json::Value,
}

/// Delete a registry value.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryDeleteRequest {
    /// Registry hive, e.g. HKEY_LOCAL_MACHINE / HKEY_CURRENT_USER.
    pub hive: winctl::RegistryHive,
    /// Registry key path within the hive.
    pub path: String,
    /// Value name to delete. Omit or null to delete the key's default (unnamed) value.
    pub name: Option<String>,
}

/// List recent system notifications/toasts.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct NotificationsListRequest {
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_items: Option<usize>,
}

/// Collect diagnostic information about a process.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessDiagnosticsRequest {
    /// Target process id (PID).
    pub pid: u32,
    /// If true, include the process's top-level windows in the diagnostics. Defaults to false.
    #[serde(default)]
    pub include_windows: bool,
    /// If true, include child processes in the diagnostics. Defaults to false.
    #[serde(default)]
    pub include_children: bool,
}

/// Perform an HTTP request and return the response.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct NetworkFetchRequest {
    /// URL to request.
    pub url: String,
    /// HTTP method, e.g. GET, POST, PUT, DELETE. Defaults to GET when omitted.
    pub method: Option<String>,
    /// HTTP request headers to send.
    pub headers: Option<HashMap<String, String>>,
    /// Request body to send (for methods that accept one).
    pub body: Option<String>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_bytes: Option<usize>,
    /// If true, automatically follow HTTP redirects. Defaults to false.
    #[serde(default)]
    pub follow_redirects: bool,
}

/// Fetch a URL and extract its text and/or links.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct NetworkScrapeRequest {
    /// URL to scrape.
    pub url: String,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Upper bound on the amount returned; results are truncated beyond it.
    pub max_bytes: Option<usize>,
    /// If true, automatically follow HTTP redirects. Defaults to false.
    #[serde(default)]
    pub follow_redirects: bool,
    /// If true, include extracted hyperlinks in the result. Defaults to true.
    #[serde(default = "default_true")]
    pub include_links: bool,
    /// If true, include extracted page text in the result. Defaults to true.
    #[serde(default = "default_true")]
    pub include_text: bool,
}

/// Start a new macro recording session.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderStartRequest {
    /// Human-readable title for the recorded macro.
    pub title: String,
    /// Optional longer description of the macro being recorded.
    pub description: Option<String>,
    /// Tags to associate with the recorded macro for later lookup. Defaults to an empty list.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional app identity to associate the recording with a specific application.
    pub app_identity: Option<winctl_macro::AppIdentity>,
    /// If true or omitted, the native Windows hook recorder captures human input into steps. If false, the session accepts only explicit `recorder.record_step` calls until resumed.
    pub capture_input: Option<bool>,
}

/// Pause or resume the active recording session.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderPauseRequest {
    /// If true, pause native input capture; if false, resume capture for the active session.
    pub paused: bool,
    /// Optional human-readable reason recorded in the durable audit log and session notes.
    pub reason: Option<String>,
}

/// Append a step to the active recording session.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RecorderRecordStepRequest {
    /// Optional explicit step id; auto-generated when omitted.
    pub id: Option<String>,
    /// Name of the tool this step invokes.
    pub tool: String,
    /// Arguments passed to the tool for this step.
    pub args: Option<serde_json::Value>,
    /// Optional macro target describing the window/element the step acts on.
    pub target: Option<winctl_macro::MacroTarget>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// If true, failure of this step fails the macro run. Defaults to true.
    #[serde(default = "default_true")]
    pub required: bool,
    /// If true, continue running subsequent steps even if this step fails. Defaults to false.
    #[serde(default)]
    pub continue_on_failure: bool,
    /// Optional metadata describing a coordinate-based fallback if the primary target cannot be resolved.
    pub coordinate_fallback: Option<winctl_macro::CoordinateFallbackMetadata>,
    /// Optional audit metadata recorded for the step.
    pub audit: Option<winctl_macro::StepAudit>,
    /// Optional free-form note attached to the step.
    pub note: Option<String>,
}

/// Stop the active recording session.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderStopRequest {
    /// If true, persist the recorded macro to memory for later reuse. Defaults to false.
    #[serde(default)]
    pub save_to_memory: bool,
}

/// Export a recorded session's manifest.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderExportRequest {
    /// Id of the recording session to export. Defaults to the most recent/active session when omitted.
    pub session_id: Option<String>,
}

/// Validate a test manifest without running it.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TestManifestRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md).
    pub manifest: winctl_macro::TestManifest,
}

/// Run a test manifest.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TestRunRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md).
    pub manifest: winctl_macro::TestManifest,
    /// Maximum number of steps to execute before stopping.
    pub max_steps: Option<usize>,
    /// Optional video-capture configuration to record while the test runs.
    pub video: Option<VideoStartRequest>,
}

/// Wait until a bound window's rendered image changes.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WindowImageChangeWaitRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Polling interval between checks, in milliseconds.
    pub poll_interval_ms: Option<u64>,
}

/// Start capturing a video recording of a window or display.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct VideoStartRequest {
    /// Stable bound-window id returned by `windows.bind` to record. Provide either `bound_id` or `display_index`.
    pub bound_id: Option<String>,
    /// Zero-based display/monitor index to record. Provide either `bound_id` or `display_index`.
    pub display_index: Option<usize>,
    /// Interval between captured frames, in milliseconds.
    pub frame_interval_ms: Option<u64>,
    /// Maximum total recording duration, in milliseconds.
    pub max_duration_ms: Option<u64>,
    /// Maximum frame width, in pixels; frames are downscaled to fit.
    pub max_frame_width: Option<u32>,
    /// Maximum frame height, in pixels; frames are downscaled to fit.
    pub max_frame_height: Option<u32>,
    /// Optional name for the output recording file/artifact.
    pub output_name: Option<String>,
}

/// Stop an active video recording.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct VideoStopRequest {
    /// Id of the recording to stop, as returned by the video start tool. Defaults to the active recording when omitted.
    pub recording_id: Option<String>,
}

/// Capture a UI Automation tree snapshot of a bound window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiSnapshotRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Maximum depth of the UI Automation tree to traverse; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements returned; results are truncated beyond it.
    pub max_elements: Option<usize>,
}

/// Find UI elements in a bound window matching a selector.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiFindRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Selector describing which UI element(s) to match (by name, automation id, control type, etc.).
    pub selector: winctl::UiElementSelector,
    /// Maximum depth of the UI Automation tree to traverse; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements returned; results are truncated beyond it.
    pub max_elements: Option<usize>,
}

/// Resolve a previously returned element reference to its current element.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiResolveRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Opaque element reference returned by a prior `ui.find`/`ui.snapshot` call.
    pub element_ref: String,
    /// Maximum depth of the UI Automation tree to traverse; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements returned; results are truncated beyond it.
    pub max_elements: Option<usize>,
}

/// Invoke an action (click, expand/collapse, toggle, select) on a UI element.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct UiElementActionRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Opaque element reference returned by a prior `ui.find`/`ui.snapshot` call. Provide either `element_ref` or `selector`.
    pub element_ref: Option<String>,
    /// Selector describing which UI element to act on. Provide either `element_ref` or `selector`.
    pub selector: Option<winctl::UiElementSelector>,
    /// Maximum depth of the UI Automation tree to traverse when resolving the selector; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned while resolving the selector.
    pub max_elements: Option<usize>,
    /// If true, allow acting on elements that are off-screen rather than failing. Defaults to false.
    #[serde(default)]
    pub allow_offscreen: bool,
    /// For expand/collapse-capable elements, whether to expand or collapse.
    pub expand_collapse_action: Option<winctl::UiExpandCollapseAction>,
    /// For toggle-capable elements, the desired toggle state to set.
    pub desired_state: Option<winctl::UiToggleDesiredState>,
    /// For selectable elements, the selection mode to apply (e.g. select, add, remove).
    pub mode: Option<winctl::UiSelectionMode>,
}

/// Set the text value of a UI element.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiSetValueRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Opaque element reference returned by a prior `ui.find`/`ui.snapshot` call. Provide either `element_ref` or `selector`.
    pub element_ref: Option<String>,
    /// Selector describing which UI element to set. Provide either `element_ref` or `selector`.
    pub selector: Option<winctl::UiElementSelector>,
    /// The text value to set on the element.
    pub value: String,
    /// Maximum depth of the UI Automation tree to traverse when resolving the selector; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned while resolving the selector.
    pub max_elements: Option<usize>,
    /// If true, allow setting elements that are off-screen rather than failing. Defaults to false.
    #[serde(default)]
    pub allow_offscreen: bool,
    /// If true, replace the existing value; if false, append/insert. Defaults to true.
    #[serde(default = "default_true")]
    pub replace_existing: bool,
}

/// Set the numeric (range) value of a UI element such as a slider.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct UiRangeValueRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Opaque element reference returned by a prior `ui.find`/`ui.snapshot` call. Provide either `element_ref` or `selector`.
    pub element_ref: Option<String>,
    /// Selector describing which UI element to set. Provide either `element_ref` or `selector`.
    pub selector: Option<winctl::UiElementSelector>,
    /// The numeric value to set, within the element's supported range.
    pub value: f64,
    /// Maximum depth of the UI Automation tree to traverse when resolving the selector; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned while resolving the selector.
    pub max_elements: Option<usize>,
    /// If true, allow setting elements that are off-screen rather than failing. Defaults to false.
    #[serde(default)]
    pub allow_offscreen: bool,
}

/// Wait until a UI element matching the criteria appears/becomes ready.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct UiWaitForElementRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Opaque element reference returned by a prior `ui.find`/`ui.snapshot` call. Provide either `element_ref` or `selector`.
    pub element_ref: Option<String>,
    /// Selector describing which UI element to wait for. Provide either `element_ref` or `selector`.
    pub selector: Option<winctl::UiElementSelector>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Polling interval between checks, in milliseconds.
    pub poll_interval_ms: Option<u64>,
    /// Maximum depth of the UI Automation tree to traverse when resolving the selector; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned while resolving the selector.
    pub max_elements: Option<usize>,
    /// If set, only succeed once the matched element's enabled state matches this value.
    pub require_enabled: Option<bool>,
    /// If set, only succeed once the matched element's visible state matches this value.
    pub require_visible: Option<bool>,
    /// Case-insensitive substring the matched element's name must contain.
    pub name_contains: Option<String>,
}

/// List currently open dialog windows.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct DialogListRequest {
    /// Maximum depth of the UI Automation tree to traverse per dialog; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements returned; results are truncated beyond it.
    pub max_elements: Option<usize>,
    /// If true, include dialogs that are not in the foreground. Defaults to false.
    #[serde(default)]
    pub include_non_foreground: bool,
    /// If true, also include the foreground window even when it is not recognized as a dialog. Defaults to false.
    #[serde(default)]
    pub include_non_dialog_foreground: bool,
}

/// Invoke a button within a dialog window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct DialogInvokeButtonRequest {
    /// Raw window handle (HWND) of the dialog, as a string.
    pub hwnd: String,
    /// Target process id (PID) that owns the dialog.
    pub pid: u32,
    /// Name/caption of the button to invoke (e.g. "OK", "Cancel"). Provide one of `button_name`, `element_ref`, or `selector`.
    pub button_name: Option<String>,
    /// Opaque element reference for the button, returned by a prior `ui.find`/`ui.snapshot` call.
    pub element_ref: Option<String>,
    /// Selector describing which button element to invoke.
    pub selector: Option<winctl::UiElementSelector>,
    /// Maximum depth of the UI Automation tree to traverse when resolving the button; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned while resolving the button.
    pub max_elements: Option<usize>,
    /// If true, allow invoking buttons that are off-screen rather than failing. Defaults to false.
    #[serde(default)]
    pub allow_offscreen: bool,
    /// If true, allow invoking even when the target window is not recognized as a dialog. Defaults to false.
    #[serde(default)]
    pub allow_non_dialog: bool,
}

/// Assert properties of a UI element.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct AssertElementRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Opaque element reference returned by a prior `ui.find`/`ui.snapshot` call. Provide either `element_ref` or `selector`.
    pub element_ref: Option<String>,
    /// Selector describing which UI element to assert on. Provide either `element_ref` or `selector`.
    pub selector: Option<winctl::UiElementSelector>,
    /// Maximum depth of the UI Automation tree to traverse when resolving the selector; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned while resolving the selector.
    pub max_elements: Option<usize>,
    /// If set, assert the element exists (true) or does not exist (false).
    pub exists: Option<bool>,
    /// If set, assert the element's enabled state matches this value.
    pub enabled: Option<bool>,
    /// If set, assert the element's name equals this value exactly.
    pub name: Option<String>,
    /// Case-insensitive substring the element's name must contain for the assertion to pass.
    pub name_contains: Option<String>,
}

/// Assert that text is visible somewhere in a bound window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct AssertTextVisibleRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// The text that must be visible within the window for the assertion to pass.
    pub text: String,
    /// Maximum depth of the UI Automation tree to traverse; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements scanned; results are truncated beyond it.
    pub max_elements: Option<usize>,
}

/// Assert the color of a pixel in an image or bound window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct AssertPixelColorRequest {
    /// Path to an image file to sample. Provide either `image_path` or `bound_id`.
    pub image_path: Option<String>,
    /// Stable bound-window id returned by `windows.bind` to capture and sample. Provide either `image_path` or `bound_id`.
    pub bound_id: Option<String>,
    /// X coordinate of the pixel to sample, in pixels within the image/window.
    pub x: u32,
    /// Y coordinate of the pixel to sample, in pixels within the image/window.
    pub y: u32,
    /// Expected pixel color as [R, G, B] (0-255 each).
    pub expected_rgb: Option<[u8; 3]>,
    /// Per-channel tolerance (0-255) allowed when comparing to `expected_rgb`.
    pub tolerance: Option<u8>,
}

/// Assert how many windows match a selector.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct AssertWindowCountRequest {
    /// Window selector describing which windows to count (by title, class, PID, etc.).
    pub selector: WindowSelector,
    /// If set, assert the matching window count equals this value exactly.
    pub expected: Option<usize>,
    /// If set, assert the matching window count is at least this value.
    pub min: Option<usize>,
    /// If set, assert the matching window count is at most this value.
    pub max: Option<usize>,
}

/// Assert the clipboard's current text contents.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct AssertClipboardRequest {
    /// If set, assert the clipboard text equals this value exactly.
    pub expected: Option<String>,
    /// If set, assert the clipboard text contains this substring.
    pub contains: Option<String>,
    /// Upper bound on the number of clipboard characters read; comparison is performed against the truncated text.
    pub max_chars: Option<usize>,
}

/// Run OCR over a region of an image or bound window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct CaptureOcrRegionRequest {
    /// Path to an image file to OCR. Provide either `image_path` or `bound_id`.
    pub image_path: Option<String>,
    /// Stable bound-window id returned by `windows.bind` to capture and OCR. Provide either `image_path` or `bound_id`.
    pub bound_id: Option<String>,
    /// X coordinate of the region's top-left corner, in pixels. Defaults to 0 (full width) when omitted.
    pub x: Option<u32>,
    /// Y coordinate of the region's top-left corner, in pixels. Defaults to 0 (full height) when omitted.
    pub y: Option<u32>,
    /// Width of the region, in pixels. Defaults to the remaining width when omitted.
    pub width: Option<u32>,
    /// Height of the region, in pixels. Defaults to the remaining height when omitted.
    pub height: Option<u32>,
}

/// Read text from a bound window's accessibility tree.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct CaptureReadTextRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity (HWND+PID+executable) is revalidated before the action; not a raw HWND or PID.
    pub bound_id: String,
    /// Maximum depth of the UI Automation tree to traverse; deeper elements are omitted.
    pub max_depth: Option<usize>,
    /// Upper bound on the number of elements read; results are truncated beyond it.
    pub max_elements: Option<usize>,
}

/// Compare a captured image against a baseline image.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct CaptureCompareBaselineRequest {
    /// Path to the actual/captured image to compare.
    pub actual_path: String,
    /// Path to the baseline/reference image to compare against.
    pub baseline_path: String,
    /// Per-channel tolerance (0-255) allowed when comparing pixels.
    pub tolerance: Option<u8>,
    /// Maximum number of differing pixels permitted before the comparison fails.
    pub max_different_pixels: Option<u64>,
    /// Optional path to write a visual diff image to.
    pub diff_path: Option<String>,
}

/// Run a build/program command and capture its output.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BuildRunRequest {
    /// Path or name of the program/command to run.
    pub program: String,
    /// Command-line arguments passed to the program. Defaults to an empty list.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the command. Defaults to the server's current directory when omitted.
    pub cwd: Option<String>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// Collect performance metrics for a process.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessMetricsRequest {
    /// Target process id (PID).
    pub pid: u32,
}

/// Generate a crash/diagnostic report for a process or window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct CrashReportRequest {
    /// Target process id (PID). Provide either `pid` or `bound_id`.
    pub pid: Option<u32>,
    /// Stable bound-window id returned by `windows.bind` whose owning process to report on. Provide either `pid` or `bound_id`.
    pub bound_id: Option<String>,
}

/// Export a stored test report.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TestReportExportRequest {
    /// Id of the test run whose report to export.
    pub run_id: String,
    /// Output format for the report, e.g. json or html. Defaults to a server-chosen format when omitted.
    pub format: Option<String>,
    /// Absolute path to write the exported report to. Defaults to a server-chosen location when omitted.
    pub output_path: Option<String>,
}

/// Probe a Chrome DevTools Protocol debugger endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct CdpEndpointRequest {
    /// URL of the Chrome DevTools Protocol debugger endpoint (e.g. http://127.0.0.1:9222).
    pub debugger_url: String,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// Evaluate a JavaScript expression in a CDP target.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct CdpEvaluateRequest {
    /// URL of the Chrome DevTools Protocol debugger endpoint (e.g. http://127.0.0.1:9222).
    pub debugger_url: String,
    /// Id of the specific CDP target (tab) to evaluate in. Defaults to the active target when omitted.
    pub target_id: Option<String>,
    /// JavaScript expression to evaluate in the target.
    pub expression: String,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// Introspect a web page's DOM via CDP.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WebIntrospectionRequest {
    /// URL of the Chrome DevTools Protocol debugger endpoint (e.g. http://127.0.0.1:9222).
    pub debugger_url: String,
    /// Id of the specific CDP target (tab) to introspect. Defaults to the active target when omitted.
    pub target_id: Option<String>,
    /// Optional CSS selector to scope introspection to matching DOM elements.
    pub selector: Option<String>,
    /// Maximum time to wait, in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// Arm the control consent gate, allowing gated actions for a window.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ControlArmRequest {
    /// Consent-gate session id to arm. Defaults to a new/global session when omitted.
    pub session_id: Option<String>,
    /// Stable bound-window id returned by `windows.bind` to scope the consent grant to. Identity is revalidated before gated actions.
    pub bound_id: Option<String>,
    /// Time-to-live of the consent grant, in milliseconds; gated actions are permitted until this elapses.
    pub allow_for_ms: Option<u64>,
    /// Human-readable reason for arming the gate, recorded in the consent audit trail.
    pub reason: Option<String>,
}

/// Record a consent decision for the control gate.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ControlConsentRequest {
    /// Consent decision, e.g. `allow` or `deny`, controlling whether gated actions may proceed.
    pub decision: String,
    /// Consent-gate session id the decision applies to. Defaults to the active/global session when omitted.
    pub session_id: Option<String>,
    /// Stable bound-window id returned by `windows.bind` the consent decision is scoped to.
    pub bound_id: Option<String>,
    /// Time-to-live of the consent grant, in milliseconds, when the decision allows gated actions.
    pub allow_for_ms: Option<u64>,
}

/// Revoke an active control consent grant.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ControlRevokeRequest {
    /// Consent-gate session id to revoke. Defaults to the active/global session when omitted.
    pub session_id: Option<String>,
    /// Human-readable reason for revoking, recorded in the consent audit trail.
    pub reason: Option<String>,
}

/// Notify the user before a gated control action runs.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ControlNotifyRequest {
    /// Name of the tool whose gated action is about to run.
    pub tool_name: String,
    /// Stable bound-window id returned by `windows.bind` the pending action targets.
    pub bound_id: Option<String>,
    /// Category of the pending action (e.g. click, type), shown in the consent notification.
    pub action_kind: Option<String>,
    /// Consent-gate session id this notification is associated with. Defaults to the active/global session when omitted.
    pub session_id: Option<String>,
    /// Countdown before the action proceeds, in milliseconds, during which the user can intervene.
    pub countdown_ms: Option<u64>,
}

/// Validate a macro manifest without running it.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroManifestRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md).
    pub manifest: winctl_macro::MacroManifest,
}

/// Dry-run a macro to preview its steps without performing actions.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroDryRunRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md). Provide either `manifest` or `memory_id`.
    pub manifest: Option<winctl_macro::MacroManifest>,
    /// Id of a macro stored in memory to load. Provide either `manifest` or `memory_id`.
    pub memory_id: Option<String>,
}

/// Run a macro.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroRunRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md). Provide either `manifest` or `memory_id`.
    pub manifest: Option<winctl_macro::MacroManifest>,
    /// Id of a macro stored in memory to load. Provide either `manifest` or `memory_id`.
    pub memory_id: Option<String>,
    /// Maximum number of steps to execute before stopping.
    pub max_steps: Option<usize>,
    /// Optional video-capture configuration to record while the macro runs.
    pub video: Option<VideoStartRequest>,
}

/// Run a single step of a macro.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroRunStepRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md). Provide either `manifest` or `memory_id`.
    pub manifest: Option<winctl_macro::MacroManifest>,
    /// Id of a macro stored in memory to load. Provide either `manifest` or `memory_id`.
    pub memory_id: Option<String>,
    /// Id of the step within the macro to execute.
    pub step_id: String,
    /// Optional JSON context passed to the step for variable substitution. Defaults to none.
    #[serde(default)]
    pub context_json: Option<serde_json::Value>,
}

/// Type a named secret into a bound window without exposing the plaintext to the model.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroTypeSecretRequest {
    /// Stable bound-window id returned by `windows.bind`. Identity is revalidated before typing.
    pub bound_id: String,
    /// Name of the encrypted secret to resolve server-side at replay time.
    pub secret_ref: String,
}

/// Abort an in-progress macro run.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroAbortRequest {
    /// Id of the macro run to abort, as returned by `macro.run`.
    pub run_id: String,
}

/// Retrieve a stored macro by id.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroGetRequest {
    /// Id of the stored macro to retrieve.
    pub id: String,
}

/// List stored macros, optionally filtered.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct MacroListRequest {
    /// If set, restrict results to macros of this kind/category.
    pub kind: Option<String>,
    /// If set, restrict results to macros tagged with all of these tags. Defaults to an empty list (no tag filter).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Upper bound on the number of macros returned; results are truncated beyond it.
    pub limit: Option<usize>,
}

/// Promote a macro manifest (e.g. from a recording) into the stored library.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroPromoteRequest {
    /// A winctl macro/test manifest object (see docs/MACRO_MANIFEST.md / docs/TEST_MANIFEST.md).
    pub manifest: winctl_macro::MacroManifest,
    /// If true, persist the promoted macro to memory. Defaults to true.
    #[serde(default = "default_true")]
    pub remember: bool,
}

/// Export the result of a macro run.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroExportResultRequest {
    /// Id of the macro run whose result to export, as returned by `macro.run`.
    pub run_id: String,
}

/// Store or replace one encrypted secret.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SecretSetRequest {
    /// Stable secret name referenced later by manifest `macro.type_secret` steps.
    pub name: String,
    /// Plaintext secret value. The server encrypts it immediately with Windows DPAPI and never returns or logs it.
    pub value: String,
    /// Optional human-readable description shown in metadata-only listings.
    pub description: Option<String>,
    /// Optional metadata tags for grouping secrets. Defaults to an empty list.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Delete one encrypted secret by name.
#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SecretDeleteRequest {
    /// Secret name to delete. The value is not returned before deletion.
    pub name: String,
}

fn default_force() -> bool {
    true
}

fn default_true() -> bool {
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
        description = "Return a minimal health response without touching Win32 APIs. Read-only; no bound target or armed session needed. Returns `{ok, pong}` to confirm the server is reachable; use it as a liveness probe before other calls."
    )]
    pub async fn server_ping(&self) -> Json<serde_json::Value> {
        tracing::info!("server.ping requested");
        Json(serde_json::json!({"ok": true, "pong": true}))
    }

    #[tool(
        name = "server.config",
        description = "Return effective runtime configuration and security-policy diagnostics: capture paths, filesystem roots, mutation policy, network policy, memory settings, macro execution policy, and embedding metadata. Read-only; no bound target or armed session needed. Call after startup to confirm which mutation gates (clipboard/filesystem/registry) and roots are enabled before attempting gated operations."
    )]
    pub async fn server_config(&self) -> Json<serde_json::Value> {
        tracing::info!("server.config requested");
        Json(serde_json::json!({
            "ok": true,
            "capture_dir": self.state.capture_dir.as_ref(),
            "policy": self.state.policy.as_ref(),
            "memory": {
                "embedding_model": winctl_memory::DEFAULT_EMBEDDING_MODEL,
                "embedding_dimension": winctl_memory::DEFAULT_EMBEDDING_DIM,
                "schema_version": winctl_memory::MEMORY_SCHEMA_VERSION,
            }
        }))
    }

    #[tool(
        name = "control.state",
        description = "Return the desktop-control gate state (idle, armed, active, blocked, or revoked), active target identity, consent decision, recent in-memory control events, and durable audit-log entries. Read-only; no bound target or armed session needed. Use it to check whether control is armed before calling gated input/window/mutation tools."
    )]
    pub async fn control_state(&self) -> Json<serde_json::Value> {
        let state = self.state.clone();
        run_blocking_tool("control.state", move || {
            tools::control::control_state(&state)
        })
        .await
    }

    #[tool(
        name = "control.arm",
        description = "Arm the desktop-control gate for a session or bound target so subsequent control-gated tools (input.*, uia action tools, windows.focus/close, process.kill, registry/filesystem/clipboard mutation, macro.run, test.run) are permitted; pass an optional `bound_id`, `allow_for_ms`, and audit `reason`. This is the precondition every gated tool checks; arming clears emergency-stop state and records an auditable event. Itself not gated."
    )]
    pub async fn control_arm(
        &self,
        request: Parameters<ControlArmRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("control.arm", move || {
            tools::control::control_arm(&state, request)
        })
        .await
    }

    #[tool(
        name = "control.consent",
        description = "Record an `allow_once`, `allow_session`, `deny`, or `revoke_session` consent decision for desktop-control actions, optionally scoped to a `session_id` or `bound_id`. Decisions are logged and surfaced through `control.state` for dashboard/tray display; use alongside `control.arm` to grant or withdraw control. Itself not gated."
    )]
    pub async fn control_consent(
        &self,
        request: Parameters<ControlConsentRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("control.consent", move || {
            tools::control::control_consent(&state, request)
        })
        .await
    }

    #[tool(
        name = "control.notify",
        description = "Record a pending desktop-control notification event (tool name, optional `bound_id`, `action_kind`, cancelable `countdown_ms`) for tray or dashboard display before a control action runs. The event is recorded even when no native toast provider is enabled. Read-only side effect; not gated."
    )]
    pub async fn control_notify(
        &self,
        request: Parameters<ControlNotifyRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("control.notify", move || {
            tools::control::control_notify(&state, request)
        })
        .await
    }

    #[tool(
        name = "control.revoke",
        description = "Emergency-stop desktop control: fail-closed all sensitive control tools until rearmed. After this, gated tools return `control_consent_required` until `control.arm`/`control.consent` is called again. Itself not gated; use as the abort path."
    )]
    pub async fn control_revoke(
        &self,
        request: Parameters<ControlRevokeRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("control.revoke", move || {
            tools::control::control_revoke(&state, request)
        })
        .await
    }

    #[tool(
        name = "control.emergency_stop",
        description = "Alias for `control.revoke`: immediately fail-closes desktop control and requires a new arm/consent decision before any gated tool runs. Intended as the tray/dashboard stop action; on Windows the `Ctrl+Alt+Esc` global hotkey triggers the same revoked state. Itself not gated."
    )]
    pub async fn control_emergency_stop(
        &self,
        request: Parameters<ControlRevokeRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("control.emergency_stop", move || {
            tools::control::control_revoke(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.remember",
        description = "Explicitly store a structured memory item (`kind`, `title`, searchable `text`, optional `manifest_json`, `tags`, app/target identity) with sqlite-vec embedding and FTS5 indexing. Memory is opt-in: the server never remembers procedures without this call. Returns the new item ID for later `memory.get`/`memory.update`. Read-only with respect to the desktop; not gated."
    )]
    pub async fn memory_remember(
        &self,
        request: Parameters<winctl_memory::RememberRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("memory.remember", move || {
            tools::memory::memory_remember(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.search",
        description = "Search remembered procedures, observations, macros, and recipes via hybrid sqlite-vec + FTS5 + tag + identity + recency + usefulness ranking; filter by `query`, `tags`, `kind`, or app/target identity. Read-only; not gated. Returns ranked items with per-signal score components; fetch full content with `memory.get`."
    )]
    pub async fn memory_search(
        &self,
        request: Parameters<winctl_memory::MemorySearchRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("memory.search", move || {
            tools::memory::memory_search(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.get",
        description = "Fetch one memory item by `id` (typically from `memory.search`/`memory.list`) and return its full content and manifest. Side effect: increments `use_count` and updates `last_used_at`. Read-only with respect to the desktop; not gated."
    )]
    pub async fn memory_get(
        &self,
        request: Parameters<winctl_memory::MemoryIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("memory.get", move || {
            tools::memory::memory_get(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.update",
        description = "Update a remembered item by `id`, replacing only the fields supplied (`kind`, `title`, `text`, `manifest_json`, `tags`, identity) and rebuilding its FTS5 and sqlite-vec indexes. Requires an existing item ID. Not gated; fails if the ID does not exist."
    )]
    pub async fn memory_update(
        &self,
        request: Parameters<winctl_memory::MemoryUpdateRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("memory.update", move || {
            tools::memory::memory_update(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.delete",
        description = "Delete one remembered item by `id`, removing it from the SQLite table, FTS5 index, and sqlite-vec vector index. Requires an existing item ID. Not gated; irreversible."
    )]
    pub async fn memory_delete(
        &self,
        request: Parameters<winctl_memory::MemoryIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("memory.delete", move || {
            tools::memory::memory_delete(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.list",
        description = "List remembered items newest-updated first, with optional `kind` and `tags` filters and a `limit`. Read-only; not gated. Use to browse stored procedures/macros when you do not have a search query."
    )]
    pub async fn memory_list(
        &self,
        request: Parameters<winctl_memory::MemoryListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("memory.list", move || {
            tools::memory::memory_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "memory.reindex",
        description = "Rebuild the FTS5 and sqlite-vec indexes for the local memory database. No inputs. Use after manual database inspection or migration recovery; not gated."
    )]
    pub async fn memory_reindex(&self) -> Json<serde_json::Value> {
        let state = self.state.clone();
        run_blocking_tool("memory.reindex", move || {
            tools::memory::memory_reindex(&state)
        })
        .await
    }

    #[tool(
        name = "secret.set",
        description = "Store or replace one encrypted secret by `name`, encrypting `value` immediately with the Windows current-user DPAPI provider. Preconditions: memory mutation policy must allow writes and the server must be running on Windows for DPAPI. Returns metadata only; never returns plaintext or ciphertext. Fails with a policy/provider error rather than writing an unsafe fallback."
    )]
    pub async fn secret_set(
        &self,
        request: Parameters<SecretSetRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("secret.set", move || {
            tools::secrets::secret_set(&state, request)
        })
        .await
    }

    #[tool(
        name = "secret.list",
        description = "List encrypted secret metadata: names, descriptions, tags, provider, timestamps, and use counters only. Read-only; no desktop control or armed session needed. There is intentionally no `secret.get`; plaintext is decrypted only by the server-internal macro replay resolver."
    )]
    pub async fn secret_list(&self) -> Json<serde_json::Value> {
        let state = self.state.clone();
        run_blocking_tool("secret.list", move || tools::secrets::secret_list(&state)).await
    }

    #[tool(
        name = "secret.delete",
        description = "Delete one encrypted secret by `name`. Preconditions: memory mutation policy must allow writes. Returns whether a row was removed and never exposes the secret value. Fails with policy or storage diagnostics."
    )]
    pub async fn secret_delete(
        &self,
        request: Parameters<SecretDeleteRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("secret.delete", move || {
            tools::secrets::secret_delete(&state, request)
        })
        .await
    }

    #[tool(
        name = "macro.validate",
        description = "Validate a `winctl.macro.v1` manifest: schema version, tool names, target identity requirements, and coordinate-fallback metadata, without running anything. Read-only; not gated. Returns structured validation errors; run before `macro.dry_run`/`macro.run` to catch problems early."
    )]
    pub async fn macro_validate(
        &self,
        request: Parameters<MacroManifestRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("macro.validate", move || {
            tools::macros::macro_validate(request)
        })
        .await
    }

    #[tool(
        name = "macro.dry_run",
        description = "Build a dry-run execution plan for a macro manifest (passed inline as `manifest` or loaded from a `memory_id`) without performing any mutating UI action. Read-only; not gated. Returns the resolved per-step plan so you can preview ordering and targets before `macro.run`."
    )]
    pub async fn macro_dry_run(
        &self,
        request: Parameters<MacroDryRunRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.dry_run", move || {
            tools::macros::macro_dry_run(&state, request)
        })
        .await
    }

    #[tool(
        name = "macro.run",
        description = "Execute a macro manifest (inline `manifest` or `memory_id`) through the underlying MCP tools, revalidating each target identity before every control action; optional `max_steps` and run `video` capture. Requires an armed control session (`control.arm`); fails closed otherwise. Returns a `run_id` — fetch the structured result and artifacts with `macro.export_result`."
    )]
    pub async fn macro_run(&self, request: Parameters<MacroRunRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.run", move || {
            tools::control::with_control_gate(
                &state,
                "macro.run",
                None,
                "macro_replay",
                true,
                || tools::macros::macro_run(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "macro.run_step",
        description = "Execute a single macro step by `step_id` for stepwise debugging, with optional `context_json` carrying values such as `launch_id`, `pid`, and `bound_id`. Requires an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated before any control action."
    )]
    pub async fn macro_run_step(
        &self,
        request: Parameters<MacroRunStepRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.run_step", move || {
            tools::control::with_control_gate(
                &state,
                "macro.run_step",
                None,
                "macro_replay",
                true,
                || tools::macros::macro_run_step(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "macro.type_secret",
        description = "Resolve a named encrypted `secret_ref` server-side and type it into a revalidated `bound_id` via SendInput without returning plaintext, ciphertext, or typed length. Preconditions: the secret must exist in the DPAPI vault and desktop control must be armed (`control.arm`). Returns only `{ok, bound_id, typed_secret, replay}` on success; failures report missing secret, provider, control-gate, or target identity diagnostics."
    )]
    pub async fn macro_type_secret(
        &self,
        request: Parameters<MacroTypeSecretRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("macro.type_secret", move || {
            tools::control::with_control_gate(
                &state,
                "macro.type_secret",
                Some(&bound_id),
                "secret_input",
                true,
                || tools::secrets::macro_type_secret(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "macro.abort",
        description = "Request a safe abort of an active macro run identified by `run_id` (from `macro.run`). Read-only control signal; not itself gated. The run stops at the next safe step boundary; partial results remain retrievable via `macro.export_result`."
    )]
    pub async fn macro_abort(
        &self,
        request: Parameters<MacroAbortRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.abort", move || {
            tools::macros::macro_abort(&state, request)
        })
        .await
    }

    #[tool(
        name = "macro.list",
        description = "List session-promoted macros and memory-backed macro items, with optional `kind`, `tags`, and `limit` filters. Read-only; not gated. Returns IDs usable with `macro.get`."
    )]
    pub async fn macro_list(
        &self,
        request: Parameters<MacroListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.list", move || {
            tools::macros::macro_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "macro.get",
        description = "Get a promoted macro manifest by `id` (session macro ID or memory item ID, e.g. from `macro.list`). Read-only; not gated. Returns the full `winctl.macro.v1` manifest, ready to pass to `macro.dry_run`/`macro.run`."
    )]
    pub async fn macro_get(&self, request: Parameters<MacroGetRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.get", move || {
            tools::macros::macro_get(&state, request)
        })
        .await
    }

    #[tool(
        name = "macro.promote",
        description = "Promote a valid `winctl.macro.v1` manifest into the session registry and, when `remember` is true (default), into the local memory store. Not gated. Returns the promoted macro ID for later `macro.get`/`macro.run`."
    )]
    pub async fn macro_promote(
        &self,
        request: Parameters<MacroPromoteRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.promote", move || {
            tools::macros::macro_promote(&state, request)
        })
        .await
    }

    #[tool(
        name = "macro.export_result",
        description = "Export the structured result and artifact metadata for a completed macro run by `run_id` (returned by `macro.run`). Read-only; not gated. This is the standard way to read per-step outcomes, captures, and timing after a run."
    )]
    pub async fn macro_export_result(
        &self,
        request: Parameters<MacroExportResultRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("macro.export_result", move || {
            tools::macros::macro_export_result(&state, request)
        })
        .await
    }

    #[tool(
        name = "uia.snapshot",
        description = "Capture the UI Automation tree of a window previously bound with `windows.bind`, with element roles, names, automation IDs, bounds, state, hierarchy, and stable `element_ref` values; tune with `max_depth` (default 8) and `max_elements` (default 2000). Requires `bound_id`. Read-only. To locate a SPECIFIC control prefer the targeted `uia.find` — a full snapshot can be large/token-heavy. The returned `element_ref`s feed `uia.find`/`uia.resolve` and the uia action tools."
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
        description = "Find UI Automation elements in a fresh snapshot of a bound window by semantic `selector` (name, role, automation_id, class_name, text_contains, include_offscreen). Requires `bound_id` from `windows.bind`. Read-only. PREFERRED way to locate controls: returns stable `element_ref`s to drive with the uia action tools (`uia.invoke`/`set_value`/`toggle`/`select`) — no coordinates and no screenshots needed. Only screenshot/OCR when a control is not in the UIA tree."
    )]
    pub async fn uia_find(&self, request: Parameters<UiFindRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("uia.find", move || tools::uia::uia_find(&state, request)).await
    }

    #[tool(
        name = "uia.resolve",
        description = "Revalidate an `element_ref` (from `uia.snapshot`/`uia.find`) against a current snapshot of the bound window. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Use to confirm an element still exists/resolves before acting; fails if the reference no longer matches."
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
        name = "uia.invoke",
        description = "Invoke a UI Automation element (by `element_ref` or `selector`) via `InvokePattern` on a bound window, after revalidating its identity; coordinate fallback is returned only as a hint, not auto-clicked. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn uia_invoke(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.invoke", move || {
            tools::control::with_control_gate(
                &state,
                "uia.invoke",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_invoke(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.set_value",
        description = "Set text on a UI Automation element (by `element_ref` or `selector`) via `ValuePattern.SetValue`, returning before/after state diagnostics. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated before the write."
    )]
    pub async fn uia_set_value(
        &self,
        request: Parameters<UiSetValueRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.set_value", move || {
            tools::control::with_control_gate(
                &state,
                "uia.set_value",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_set_value(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.get_value",
        description = "Read `ValuePattern.CurrentValue` from a UI Automation element (by `element_ref` or `selector`) after revalidating it. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Returns the current text/value of the element."
    )]
    pub async fn uia_get_value(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("uia.get_value", move || {
            tools::uia::uia_get_value(&state, request)
        })
        .await
    }

    #[tool(
        name = "uia.toggle",
        description = "Toggle a UI Automation element (by `element_ref` or `selector`) via `TogglePattern`, optionally to a `desired_state` (off/on/indeterminate). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated first; if no safe action path exists it fails closed rather than guessing."
    )]
    pub async fn uia_toggle(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.toggle", move || {
            tools::control::with_control_gate(
                &state,
                "uia.toggle",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_toggle(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.expand_collapse",
        description = "Expand or collapse a UI Automation element (by `element_ref` or `selector`) via `ExpandCollapsePattern`, with `expand_collapse_action` of expand/collapse/toggle (default toggle). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated first; fails closed if no supported action path exists."
    )]
    pub async fn uia_expand_collapse(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.expand_collapse", move || {
            tools::control::with_control_gate(
                &state,
                "uia.expand_collapse",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_expand_collapse(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.select",
        description = "Select a UI Automation element (by `element_ref` or `selector`) via `SelectionItemPattern`, with `mode` replace/add/remove (default replace). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Strict target resolution with identity revalidation; fails closed if no supported action path exists."
    )]
    pub async fn uia_select(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.select", move || {
            tools::control::with_control_gate(
                &state,
                "uia.select",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_select(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.set_focus",
        description = "Set keyboard focus on a UI Automation element (by `element_ref` or `selector`) via strict target resolution, with center-click fallback. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated before focusing."
    )]
    pub async fn uia_set_focus(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.set_focus", move || {
            tools::control::with_control_gate(
                &state,
                "uia.set_focus",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_set_focus(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.range_value",
        description = "Set a numeric `value` on a UI Automation range element (by `element_ref` or `selector`) via `RangeValuePattern` (sliders, progress, spinners). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated first; fails closed if no supported action path exists."
    )]
    pub async fn uia_range_value(
        &self,
        request: Parameters<UiRangeValueRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.range_value", move || {
            tools::control::with_control_gate(
                &state,
                "uia.range_value",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_range_value(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.scroll_into_view",
        description = "Scroll a UI Automation element (by `element_ref` or `selector`) into view via `ScrollItemPattern`. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Identity is revalidated first; fails closed if no supported action path exists. Useful to make an offscreen element actionable before invoking it."
    )]
    pub async fn uia_scroll_into_view(
        &self,
        request: Parameters<UiElementActionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("uia.scroll_into_view", move || {
            tools::control::with_control_gate(
                &state,
                "uia.scroll_into_view",
                Some(&bound_id),
                "uia_action",
                true,
                || tools::uia::uia_scroll_into_view(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "uia.wait_for_element",
        description = "Poll fresh snapshots of a bound window until a UI Automation `selector` or `element_ref` resolves (optionally requiring enabled/visible/name conditions), bounded by `timeout_ms`/`poll_interval_ms`. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Use to wait for UI to appear before acting; returns the resolved element or times out."
    )]
    pub async fn uia_wait_for_element(
        &self,
        request: Parameters<UiWaitForElementRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("uia.wait_for_element", move || {
            tools::uia::uia_wait_for_element(&state, request)
        })
        .await
    }

    #[tool(
        name = "dialogs.list",
        description = "Enumerate the foreground native dialog, its UI Automation button candidates, and secure-desktop/UAC status; optional flags include non-foreground or non-dialog windows. Read-only; no bound target or armed session needed. Returns each button's `hwnd`, `pid`, name, and `element_ref` for `dialogs.invoke_button`."
    )]
    pub async fn dialogs_list(
        &self,
        request: Parameters<DialogListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("dialogs.list", move || {
            tools::dialogs::dialogs_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "dialogs.invoke_button",
        description = "Invoke an explicit foreground dialog Button/SplitButton via `InvokePattern`, targeted by required `hwnd`+`pid` (from `dialogs.list`) plus `button_name`, `element_ref`, or `selector`. Requires an armed control session (`control.arm`); fails closed otherwise. UAC secure-desktop prompts are reported, never automated."
    )]
    pub async fn dialogs_invoke_button(
        &self,
        request: Parameters<DialogInvokeButtonRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("dialogs.invoke_button", move || {
            tools::control::with_control_gate(
                &state,
                "dialogs.invoke_button",
                None,
                "dialog_action",
                true,
                || tools::dialogs::dialogs_invoke_button(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "assert.element",
        description = "Assert UI Automation element conditions (`exists`, `enabled`, `name`, `name_contains`) for an element (by `element_ref` or `selector`) on a bound window. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Returns structured pass/fail for test assertions."
    )]
    pub async fn assert_element(
        &self,
        request: Parameters<AssertElementRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("assert.element", move || {
            tools::assertions::assert_element(&state, request)
        })
        .await
    }

    #[tool(
        name = "assert.text_visible",
        description = "Assert that `text` is visible via the bound window's title/class or UI Automation tree. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Returns structured pass/fail for test assertions."
    )]
    pub async fn assert_text_visible(
        &self,
        request: Parameters<AssertTextVisibleRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("assert.text_visible", move || {
            tools::assertions::assert_text_visible(&state, request)
        })
        .await
    }

    #[tool(
        name = "assert.pixel_color",
        description = "Sample a pixel at (`x`,`y`) from an `image_path` or a freshly captured `bound_id` screenshot and optionally assert `expected_rgb` within per-channel `tolerance`. If `bound_id` is given the target must first be bound via `windows.bind`. Read-only (no armed session needed). Returns the sampled RGB and pass/fail."
    )]
    pub async fn assert_pixel_color(
        &self,
        request: Parameters<AssertPixelColorRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("assert.pixel_color", move || {
            tools::assertions::assert_pixel_color(&state, request)
        })
        .await
    }

    #[tool(
        name = "assert.window_count",
        description = "Assert the count of current windows matching a `WindowSelector` against `expected`, `min`, and/or `max`. Read-only; no bound target or armed session needed. Returns the actual count and pass/fail for test assertions."
    )]
    pub async fn assert_window_count(
        &self,
        request: Parameters<AssertWindowCountRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("assert.window_count", move || {
            tools::assertions::assert_window_count(request)
        })
        .await
    }

    #[tool(
        name = "assert.clipboard",
        description = "Assert the current clipboard text equals `expected` or contains `contains` (optionally truncated to `max_chars`). Read-only clipboard read; no armed session needed (does not write). Returns the assertion result for tests."
    )]
    pub async fn assert_clipboard(
        &self,
        request: Parameters<AssertClipboardRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("assert.clipboard", move || {
            tools::assertions::assert_clipboard(request)
        })
        .await
    }

    #[tool(
        name = "capture.ocr_region",
        description = "Run OCR over an `image_path` region or a freshly captured `bound_id` window region (`x`,`y`,`width`,`height`), returning text plus per-word boxes each with a ready-to-click `center`; the response `coordinate_space` is `screen_pixels` (directly usable with `input.click`) when `bound_id` is given, else `image_pixels`. Read-only. Use only as a FALLBACK for text/controls not in the accessibility tree (custom-rendered/canvas UIs) — try `uia.find`/`capture.read_text` first; they are cheaper and exact."
    )]
    pub async fn capture_ocr_region(
        &self,
        request: Parameters<CaptureOcrRegionRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("capture.ocr_region", move || {
            tools::assertions::capture_ocr_region(&state, request)
        })
        .await
    }

    #[tool(
        name = "capture.read_text",
        description = "Extract readable text from a bound window via its UI Automation snapshot text surface (no OCR). Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Faster and more reliable than `capture.ocr_region` when text is exposed through accessibility."
    )]
    pub async fn capture_read_text(
        &self,
        request: Parameters<CaptureReadTextRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("capture.read_text", move || {
            tools::assertions::capture_read_text(&state, request)
        })
        .await
    }

    #[tool(
        name = "capture.compare_baseline",
        description = "Compare `actual_path` against `baseline_path` with per-channel RGB `tolerance` and `max_different_pixels`, optionally writing a `diff_path` artifact. Read-only; no bound target or armed session needed. Returns the differing-pixel count and pass/fail for visual regression."
    )]
    pub async fn capture_compare_baseline(
        &self,
        request: Parameters<CaptureCompareBaselineRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("capture.compare_baseline", move || {
            tools::assertions::capture_compare_baseline(&state, request)
        })
        .await
    }

    #[tool(
        name = "build.run",
        description = "Run an allowlisted build tool (`cargo`, `dotnet`, `msbuild`, `cmake`, `ctest`) directly via `program`+`args` without cmd.exe/PowerShell, in optional `cwd`, bounded by `timeout_ms`. Not gated, but only allowlisted programs are accepted (arbitrary shell is rejected). Returns exit code, stdout/stderr, and timing diagnostics."
    )]
    pub async fn build_run(&self, request: Parameters<BuildRunRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("build.run", move || {
            tools::diagnostics::build_run(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.metrics",
        description = "Return native Windows resource counters (CPU, memory, handles, GDI, USER) for the process identified by `pid`. Read-only; no bound target or armed session needed. Fails if the PID does not exist or is inaccessible."
    )]
    pub async fn process_metrics(
        &self,
        request: Parameters<ProcessMetricsRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("process.metrics", move || {
            tools::diagnostics::process_metrics(request)
        })
        .await
    }

    #[tool(
        name = "diagnostics.crash_report",
        description = "Collect a crash/hang investigation bundle: process metadata for an optional `pid`, window state, a screenshot of an optional `bound_id`, and platform context. If `bound_id` is given the target must first be bound via `windows.bind`. Read-only (no armed session needed). Returns the aggregated diagnostics and any capture artifact id."
    )]
    pub async fn diagnostics_crash_report(
        &self,
        request: Parameters<CrashReportRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("diagnostics.crash_report", move || {
            tools::diagnostics::crash_report(&state, request)
        })
        .await
    }

    #[tool(
        name = "test.report_export",
        description = "Export a completed test or macro run (`run_id` from `test.run`/`macro.run`) as `json`, `junit` XML, or `html` to an optional `output_path`. Read-only; not gated. Returns the written artifact path; fails if the run ID is unknown."
    )]
    pub async fn test_report_export(
        &self,
        request: Parameters<TestReportExportRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("test.report_export", move || {
            tools::diagnostics::test_report_export(&state, request)
        })
        .await
    }

    #[tool(
        name = "windows.list",
        description = "List visible, discoverable top-level windows with HWND, PID, executable, class, title, and virtual-desktop geometry. No inputs. Read-only; no bound target or armed session needed. Use as the starting point to identify a target, then narrow with `windows.find` and bind it with `windows.bind`."
    )]
    pub async fn windows_list(&self) -> Json<serde_json::Value> {
        run_blocking_tool("windows.list", tools::windows::windows_list).await
    }

    #[tool(
        name = "windows.find",
        description = "Find windows matching a `WindowSelector` (id, hwnd, pid, process_name, exe-path/title/class filters, visibility/minimized/cloaked constraints) and return scored match diagnostics, without binding or controlling. Read-only; no armed session needed. Pick the best match and pass the same selector to `windows.bind` to obtain a `bound_id`."
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
        description = "Bind one window by stable identity (HWND+PID+executable) so later control/capture calls can revalidate it; pass a `WindowSelector` (ideally a strong one like hwnd/pid/process_name from `windows.find`). Read-only. Returns a `bound_id` required by nearly all window, input, uia, capture, and browser tools."
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
        description = "Describe an existing bound window record by `bound_id` (from `windows.bind`): its identity, current geometry, and state. Read-only (no armed session needed). Fails if the `bound_id` is unknown."
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
        description = "Bring a bound window to the foreground after revalidating its HWND+PID+executable identity. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Fails closed if the window changed process or closed."
    )]
    pub async fn windows_focus(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.focus", move || {
            tools::control::with_control_gate(
                &state,
                "windows.focus",
                Some(&bound_id),
                "focus",
                true,
                || tools::windows::windows_focus(&state, bound_id.clone()),
            )
        })
        .await
    }

    #[tool(
        name = "windows.window_from_point",
        description = "Resolve a virtual-desktop screen point (`x`,`y`) to the top-level and child window under it, optionally reporting whether it belongs to a given `bound_id`. Read-only; no armed session needed. Use to preflight that a coordinate lands on your intended target before `input.click`."
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
        description = "List each monitor's geometry, DPI scale, primary flag, and the total virtual-desktop bounds. No inputs. Read-only; no armed session needed. Use to map `display_index` for `capture.screenshot_display` and to interpret virtual-desktop coordinates."
    )]
    pub async fn windows_monitors(&self) -> Json<serde_json::Value> {
        run_blocking_tool("windows.monitors", tools::windows::windows_monitors).await
    }

    #[tool(
        name = "windows.wait_for_window",
        description = "Wait (up to `timeout_ms`) for a visible top-level window owned by a `pid` or `launch_id` (from `process.launch`), with optional post-match title/class filters. Read-only; no armed session needed. Use right after launching an app, then bind a returned candidate with `windows.bind`."
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
        description = "Poll a bound window (revalidating identity each time) until it satisfies the given visible/foreground/minimized/cloaked/title/class conditions, bounded by `timeout_ms`/`poll_interval_ms`. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Returns the final state or times out."
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
        name = "windows.move",
        description = "Move a bound window to virtual-desktop (`x`,`y`) after revalidating its HWND+PID+executable identity. Requires `bound_id` from `windows.bind`. Window-mutation tool: fails closed if the window changed process or closed. (Not consent-gated, but identity-strict.)"
    )]
    pub async fn windows_move(
        &self,
        request: Parameters<WindowMoveRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("windows.move", move || {
            tools::windows::windows_move(&state, request)
        })
        .await
    }

    #[tool(
        name = "windows.resize",
        description = "Resize a bound window to `width`x`height` after revalidating its HWND+PID+executable identity. Requires `bound_id` from `windows.bind`. Window-mutation tool: fails closed if the window changed process or closed. (Not consent-gated, but identity-strict.)"
    )]
    pub async fn windows_resize(
        &self,
        request: Parameters<WindowResizeRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("windows.resize", move || {
            tools::windows::windows_resize(&state, request)
        })
        .await
    }

    #[tool(
        name = "windows.minimize",
        description = "Minimize a bound window after revalidating its stable HWND+PID+executable identity. Requires `bound_id` from `windows.bind`. Fails closed if the window changed process or closed. (Not consent-gated, but identity-strict.)"
    )]
    pub async fn windows_minimize(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.minimize", move || {
            tools::windows::windows_minimize(&state, bound_id)
        })
        .await
    }

    #[tool(
        name = "windows.maximize",
        description = "Maximize a bound window after revalidating its stable HWND+PID+executable identity. Requires `bound_id` from `windows.bind`. Fails closed if the window changed process or closed. (Not consent-gated, but identity-strict.)"
    )]
    pub async fn windows_maximize(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.maximize", move || {
            tools::windows::windows_maximize(&state, bound_id)
        })
        .await
    }

    #[tool(
        name = "windows.restore",
        description = "Restore a bound window from minimized/maximized to its normal state after revalidating its stable HWND+PID+executable identity. Requires `bound_id` from `windows.bind`. Fails closed if the window changed process or closed. (Not consent-gated, but identity-strict.)"
    )]
    pub async fn windows_restore(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.restore", move || {
            tools::windows::windows_restore(&state, bound_id)
        })
        .await
    }

    #[tool(
        name = "windows.close",
        description = "Post `WM_CLOSE` to a bound window (a graceful close request, not a process kill) after revalidating its stable HWND+PID+executable identity. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Fails closed if the window changed process or closed."
    )]
    pub async fn windows_close(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.close", move || {
            tools::control::with_control_gate(
                &state,
                "windows.close",
                Some(&bound_id),
                "close",
                true,
                || tools::windows::windows_close(&state, bound_id.clone()),
            )
        })
        .await
    }

    #[tool(
        name = "windows.foreground_diagnostics",
        description = "Return foreground and replay diagnostics for a bound window (foreground state, focusability, replay hints) after revalidating identity. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Useful to debug why focus/input may not be reaching the target."
    )]
    pub async fn windows_foreground_diagnostics(
        &self,
        request: Parameters<BoundIdRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let bound_id = request.0.bound_id;
        run_blocking_tool("windows.foreground_diagnostics", move || {
            tools::windows::windows_foreground_diagnostics(&state, bound_id)
        })
        .await
    }

    #[tool(
        name = "windows.for_process",
        description = "List visible top-level windows owned by a `pid` or `launch_id`, with `include_child_process_windows` controlling whether child-process windows are returned. Read-only; no armed session needed. Use to enumerate an app's windows, then bind one with `windows.bind`."
    )]
    pub async fn windows_for_process(
        &self,
        request: Parameters<WindowsForProcessRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("windows.for_process", move || {
            tools::windows::windows_for_process(&state, request)
        })
        .await
    }

    #[tool(
        name = "app.launch",
        description = "Launch a target by `mode` (`executable`, `protocol`, `packaged_app`, or `start_menu`) plus `target`/`args`/`cwd`, with no shell concatenation; optionally `wait_for_window`. Not gated. Returns process/launch identifiers and any window candidates; for raw exe control prefer `process.launch` (which returns a tracked `pid`+`launch_id`)."
    )]
    pub async fn app_launch(
        &self,
        request: Parameters<AppLaunchRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("app.launch", move || {
            tools::process::app_launch(&state, request)
        })
        .await
    }

    #[tool(
        name = "browser.list",
        description = "List Chrome/Edge/Firefox processes and windows with explicit PID/HWND identity, filterable by `browser`, `pid`, `include_windows`, and `only_mcp_launched`. Read-only; no armed session needed. Use to locate a browser window, then `windows.bind` it for browser.* control/capture."
    )]
    pub async fn browser_list(
        &self,
        request: Parameters<BrowserListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("browser.list", move || {
            tools::browser::browser_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "browser.describe",
        description = "Describe one browser target identified by `bound_id`, `pid`, or `hwnd` (no tab-title matching), returning kind and identity metadata. Read-only; no armed session needed. If using `bound_id`, the window must first be bound via `windows.bind`."
    )]
    pub async fn browser_describe(
        &self,
        request: Parameters<BrowserDescribeRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("browser.describe", move || {
            tools::browser::browser_describe(&state, request)
        })
        .await
    }

    #[tool(
        name = "browser.wait_for_navigation",
        description = "Wait (up to `timeout_ms`) for a bound browser window's title to gain `title_contains` and/or lose `title_not_contains`, revalidating browser PID/HWND/executable identity each poll. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Approximates navigation completion via title transitions."
    )]
    pub async fn browser_wait_for_navigation(
        &self,
        request: Parameters<BrowserWaitForNavigationRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("browser.wait_for_navigation", move || {
            tools::browser::browser_wait_for_navigation(&state, request)
        })
        .await
    }

    #[tool(
        name = "browser.assert",
        description = "Assert expected `browser` kind and window `title_contains`/`class_name_contains` against a bound browser window, revalidating identity first. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Returns structured pass/fail for tests."
    )]
    pub async fn browser_assert(
        &self,
        request: Parameters<BrowserAssertRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("browser.assert", move || {
            tools::browser::browser_assert(&state, request)
        })
        .await
    }

    #[tool(
        name = "browser.extract_content",
        description = "Return safe content hints and identity metadata (not full page DOM) for a bound browser window, revalidating identity first. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). For real page DOM/JS use the loopback `web.cdp.*` tools instead."
    )]
    pub async fn browser_extract_content(
        &self,
        request: Parameters<BrowserExtractContentRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("browser.extract_content", move || {
            tools::browser::browser_extract_content(&state, request)
        })
        .await
    }

    #[tool(
        name = "browser.screenshot_checkpoint",
        description = "Capture a screenshot checkpoint of a bound browser window, revalidating identity first. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Returns the capture artifact id; equivalent to `capture.screenshot_window` scoped to browser flows."
    )]
    pub async fn browser_screenshot_checkpoint(
        &self,
        request: Parameters<BrowserExtractContentRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("browser.screenshot_checkpoint", move || {
            tools::browser::browser_screenshot_checkpoint(&state, request)
        })
        .await
    }

    #[tool(
        name = "clipboard.read",
        description = "Read the current Unicode clipboard text, optionally truncated to `max_chars`. Read-only; no bound target or armed session needed. Returns the text; pair with `clipboard.write` (which is mutation-gated) for round-trips."
    )]
    pub async fn clipboard_read(
        &self,
        request: Parameters<ClipboardReadRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("clipboard.read", move || {
            tools::system::clipboard_read(&state, request)
        })
        .await
    }

    #[tool(
        name = "clipboard.write",
        description = "Write Unicode `text` to the clipboard. Requires an armed control session (`control.arm`); fails closed otherwise. Additionally denied unless `WINCTL_ENABLE_CLIPBOARD_WRITE=1` is set in policy. Not included in macro replay."
    )]
    pub async fn clipboard_write(
        &self,
        request: Parameters<ClipboardWriteRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("clipboard.write", move || {
            tools::control::with_control_gate(
                &state,
                "clipboard.write",
                None,
                "clipboard_write",
                true,
                || tools::system::clipboard_write(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "filesystem.read",
        description = "Read a UTF-8 file at `path` (under an allowlisted root: `WINCTL_FS_ROOTS`, capture dir, or temp), up to `max_bytes`. Read-only; no armed session needed. Binary files return `utf8: false` and omit text. Fails if the path is outside allowlisted roots."
    )]
    pub async fn filesystem_read(
        &self,
        request: Parameters<FilesystemReadRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("filesystem.read", move || {
            tools::system::filesystem_read(&state, request)
        })
        .await
    }

    #[tool(
        name = "filesystem.list",
        description = "List files and directories under `path`, optionally `recursive`, up to `max_entries`. Read-only; no armed session needed. `path` must be under an allowlisted root (configure with `WINCTL_FS_ROOTS`); fails closed otherwise."
    )]
    pub async fn filesystem_list(
        &self,
        request: Parameters<FilesystemListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("filesystem.list", move || {
            tools::system::filesystem_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "filesystem.search",
        description = "Literal-search file names and bounded UTF-8 file content under `root` for `pattern`, up to `max_results`/`max_file_bytes`. Read-only; no armed session needed. `root` must be allowlisted; non-UTF-8 or oversized files are skipped."
    )]
    pub async fn filesystem_search(
        &self,
        request: Parameters<FilesystemSearchRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("filesystem.search", move || {
            tools::system::filesystem_search(&state, request)
        })
        .await
    }

    #[tool(
        name = "filesystem.copy",
        description = "Copy a file `from`→`to` (optionally `overwrite`). Requires an armed control session (`control.arm`); fails closed otherwise. Additionally denied unless `WINCTL_ENABLE_FILESYSTEM_MUTATION=1` and both paths are under allowlisted roots."
    )]
    pub async fn filesystem_copy(
        &self,
        request: Parameters<FilesystemCopyRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("filesystem.copy", move || {
            tools::control::with_control_gate(
                &state,
                "filesystem.copy",
                None,
                "filesystem_mutation",
                true,
                || tools::system::filesystem_copy(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "filesystem.move",
        description = "Move/rename a file `from`→`to` (optionally `overwrite`). Requires an armed control session (`control.arm`); fails closed otherwise. Additionally denied unless `WINCTL_ENABLE_FILESYSTEM_MUTATION=1` and both paths are under allowlisted roots."
    )]
    pub async fn filesystem_move(
        &self,
        request: Parameters<FilesystemMoveRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("filesystem.move", move || {
            tools::control::with_control_gate(
                &state,
                "filesystem.move",
                None,
                "filesystem_mutation",
                true,
                || tools::system::filesystem_move(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "filesystem.delete",
        description = "Delete `path` (optionally `recursive` for directories). Requires an armed control session (`control.arm`); fails closed otherwise. Additionally denied unless `WINCTL_ENABLE_FILESYSTEM_MUTATION=1` and the path is under an allowlisted root. Irreversible."
    )]
    pub async fn filesystem_delete(
        &self,
        request: Parameters<FilesystemDeleteRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("filesystem.delete", move || {
            tools::control::with_control_gate(
                &state,
                "filesystem.delete",
                None,
                "filesystem_mutation",
                true,
                || tools::system::filesystem_delete(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "artifact.export",
        description = "Export a captured artifact from `source_path` to `destination_path`, or to the capture dir's `exports` folder when destination is omitted. Not gated, but an explicit destination must be under an allowlisted filesystem root. Returns the written path."
    )]
    pub async fn artifact_export(
        &self,
        request: Parameters<ArtifactExportRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("artifact.export", move || {
            tools::system::artifact_export(&state, request)
        })
        .await
    }

    #[tool(
        name = "registry.list",
        description = "List registry subkeys (and optional value metadata when `include_values`) under `path` in `hive` (current_user/local_machine/classes_root/users/current_config). Read-only; no armed session needed. Registry mutation tools are separate and gated."
    )]
    pub async fn registry_list(
        &self,
        request: Parameters<RegistryListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("registry.list", move || {
            tools::system::registry_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "registry.read",
        description = "Read one registry value `name` (empty/omitted for the default value) under `path` in `hive`. Read-only; no armed session needed. Returns the decoded value and kind; fails if the key/value is missing."
    )]
    pub async fn registry_read(
        &self,
        request: Parameters<RegistryReadRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("registry.read", move || {
            tools::system::registry_read(&state, request)
        })
        .await
    }

    #[tool(
        name = "registry.write",
        description = "Write a registry value (`hive`, `path`, `name`, typed `kind`+`data`). Requires an armed control session (`control.arm`); fails closed otherwise. Additionally denied unless `WINCTL_ENABLE_REGISTRY_MUTATION=1`. Not included in macro replay."
    )]
    pub async fn registry_write(
        &self,
        request: Parameters<RegistryWriteRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("registry.write", move || {
            tools::control::with_control_gate(
                &state,
                "registry.write",
                None,
                "registry_mutation",
                true,
                || tools::system::registry_write(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "registry.delete",
        description = "Delete a single registry value `name` under `path` in `hive` (values only, never keys or trees). Requires an armed control session (`control.arm`); fails closed otherwise. Additionally denied unless `WINCTL_ENABLE_REGISTRY_MUTATION=1`."
    )]
    pub async fn registry_delete(
        &self,
        request: Parameters<RegistryDeleteRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("registry.delete", move || {
            tools::control::with_control_gate(
                &state,
                "registry.delete",
                None,
                "registry_mutation",
                true,
                || tools::system::registry_delete(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "notifications.list",
        description = "Return Windows notification inspection status and any provider-backed notifications, up to `max_items`. Read-only; no bound target or armed session needed. May report that no provider is available."
    )]
    pub async fn notifications_list(
        &self,
        request: Parameters<NotificationsListRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("notifications.list", move || {
            tools::system::notifications_list(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.diagnostics",
        description = "Return diagnostics for process `pid`, optionally with `include_windows` (owned top-level windows) and `include_children` (child-process metadata). Read-only; no armed session needed. Fails if the PID does not exist."
    )]
    pub async fn process_diagnostics(
        &self,
        request: Parameters<ProcessDiagnosticsRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.diagnostics", move || {
            tools::system::process_diagnostics(&state, request)
        })
        .await
    }

    #[tool(
        name = "network.fetch",
        description = "Fetch an HTTP/HTTPS `url` (`method` GET/HEAD/POST, optional `headers`/`body`), bounded by server-capped `timeout_ms`/`max_bytes` and optional `follow_redirects`. Read-only; not gated. Private/loopback/link-local hosts are blocked unless `WINCTL_ALLOW_PRIVATE_NETWORK=1`; URLs with credentials are rejected."
    )]
    pub async fn network_fetch(
        &self,
        request: Parameters<NetworkFetchRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::network::network_fetch(request, self.state.policy.allow_private_network).await)
    }

    #[tool(
        name = "network.scrape",
        description = "Fetch an HTTP/HTTPS `url` and extract its title, optional `include_links` hrefs, and optional `include_text` (tag-stripped text), under the same network policy as `network.fetch` (private hosts blocked unless `WINCTL_ALLOW_PRIVATE_NETWORK=1`). Read-only; not gated. Bounded by server-capped `timeout_ms`/`max_bytes`."
    )]
    pub async fn network_scrape(
        &self,
        request: Parameters<NetworkScrapeRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::network::network_scrape(request, self.state.policy.allow_private_network).await)
    }

    #[tool(
        name = "web.cdp.list_targets",
        description = "List Chrome DevTools Protocol targets (tabs/pages) from a loopback `debugger_url` such as `http://127.0.0.1:9222`. Read-only; no armed session needed. Requires the browser to be running with remote debugging on loopback. Returns `target_id`s for the other `web.*` CDP tools."
    )]
    pub async fn web_cdp_list_targets(
        &self,
        request: Parameters<CdpEndpointRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::web::cdp_list_targets(request).await)
    }

    #[tool(
        name = "web.cdp.evaluate",
        description = "Evaluate a JavaScript `expression` in a loopback CDP `target_id` (from `web.cdp.list_targets`) over WebSocket at `debugger_url`. Read-only side of the gate (not consent-gated), but loopback-only. Returns the evaluation result; fails if the debugger endpoint is unreachable or non-loopback."
    )]
    pub async fn web_cdp_evaluate(
        &self,
        request: Parameters<CdpEvaluateRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::web::cdp_evaluate(request).await)
    }

    #[tool(
        name = "web.dom.snapshot",
        description = "Capture a DOM snapshot from a loopback CDP `target_id` (from `web.cdp.list_targets`) at `debugger_url`, optionally scoped by `selector`. Read-only; not gated, loopback-only. Returns the DOM tree; fails if the endpoint is unreachable or non-loopback."
    )]
    pub async fn web_dom_snapshot(
        &self,
        request: Parameters<WebIntrospectionRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::web::dom_snapshot(request).await)
    }

    #[tool(
        name = "web.network.events",
        description = "Enable CDP Network domain on a loopback `target_id` (from `web.cdp.list_targets`) and buffer network events for the `timeout_ms` window at `debugger_url`. Read-only; not gated, loopback-only. Returns the captured events; fails if the endpoint is unreachable or non-loopback."
    )]
    pub async fn web_network_events(
        &self,
        request: Parameters<WebIntrospectionRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::web::network_events(request).await)
    }

    #[tool(
        name = "web.a11y.snapshot",
        description = "Capture the CDP accessibility tree from a loopback `target_id` (from `web.cdp.list_targets`) at `debugger_url`, optionally scoped by `selector`. Read-only; not gated, loopback-only. Returns the a11y tree; fails if the endpoint is unreachable or non-loopback."
    )]
    pub async fn web_a11y_snapshot(
        &self,
        request: Parameters<WebIntrospectionRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::web::a11y_snapshot(request).await)
    }

    #[tool(
        name = "web.style.inspect",
        description = "Inspect computed CSS style for a `selector` in a loopback CDP `target_id` (from `web.cdp.list_targets`) at `debugger_url`. Read-only; not gated, loopback-only. Returns computed style properties; fails if the endpoint is unreachable or non-loopback."
    )]
    pub async fn web_style_inspect(
        &self,
        request: Parameters<WebIntrospectionRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::web::style_inspect(request).await)
    }

    #[tool(
        name = "recorder.start",
        description = "Start a local recording session (`title`, optional `description`/`tags`/`app_identity`) that accumulates steps for export as a macro manifest. Not gated. Returns a session ID; append steps with `recorder.record_step`, then finish with `recorder.stop`/`recorder.export_manifest`."
    )]
    pub async fn recorder_start(
        &self,
        request: Parameters<RecorderStartRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("recorder.start", move || {
            tools::recorder::recorder_start(&state, request)
        })
        .await
    }

    #[tool(
        name = "recorder.record_step",
        description = "Append one step to the active recording: an MCP `tool` name plus JSON `args`, optional semantic `target`, `timeout_ms`, `required`/`continue_on_failure`, coordinate fallback, and notes. Requires an active session from `recorder.start`. Not gated; records intent only (does not execute the tool)."
    )]
    pub async fn recorder_record_step(
        &self,
        request: Parameters<RecorderRecordStepRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("recorder.record_step", move || {
            tools::recorder::recorder_record_step(&state, request)
        })
        .await
    }

    #[tool(
        name = "recorder.stop",
        description = "Stop the active recording session and return the generated `winctl.macro.v1` manifest; with `save_to_memory` true it is also stored if memory policy allows. Requires an active session from `recorder.start`. The returned manifest can be passed to `macro.validate`/`macro.run`."
    )]
    pub async fn recorder_stop(
        &self,
        request: Parameters<RecorderStopRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("recorder.stop", move || {
            tools::recorder::recorder_stop(&state, request)
        })
        .await
    }

    #[tool(
        name = "recorder.pause",
        description = "Pause or resume the active human recording session without executing desktop input. Not gated; the recorder is passive. Records a durable audit event and returns the updated session; fails when no recording is active."
    )]
    pub async fn recorder_pause(
        &self,
        request: Parameters<RecorderPauseRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("recorder.pause", move || {
            tools::recorder::recorder_pause(&state, request)
        })
        .await
    }

    #[tool(
        name = "recorder.export_manifest",
        description = "Export a recording session as a `winctl.macro.v1` manifest by `session_id`, or the active session when omitted, without stopping it. Read-only; not gated. Use `recorder.state` to find session IDs."
    )]
    pub async fn recorder_export_manifest(
        &self,
        request: Parameters<RecorderExportRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("recorder.export_manifest", move || {
            tools::recorder::recorder_export(&state, request)
        })
        .await
    }

    #[tool(
        name = "recorder.state",
        description = "Return active and completed local recording sessions and their step counts. No inputs. Read-only; not gated. Use to obtain `session_id`s for `recorder.export_manifest`."
    )]
    pub async fn recorder_state(&self) -> Json<serde_json::Value> {
        let state = self.state.clone();
        run_blocking_tool("recorder.state", move || {
            tools::recorder::recorder_state(&state)
        })
        .await
    }

    #[tool(
        name = "test.validate",
        description = "Validate a `winctl.test.v1` manifest and its aligned macro manifest (schema, tool names, identity/coordinate metadata) without running anything. Read-only; not gated. Returns structured validation errors; run before `test.dry_run`/`test.run`."
    )]
    pub async fn test_validate(
        &self,
        request: Parameters<TestManifestRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("test.validate", move || {
            tools::tests::test_validate(request)
        })
        .await
    }

    #[tool(
        name = "test.dry_run",
        description = "Build a dry-run plan for a `winctl.test.v1` manifest without mutating UI state. Read-only; not gated. Returns the resolved per-step plan so you can preview ordering and targets before `test.run`."
    )]
    pub async fn test_dry_run(
        &self,
        request: Parameters<TestManifestRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("test.dry_run", move || tools::tests::test_dry_run(request)).await
    }

    #[tool(
        name = "test.run",
        description = "Run a `winctl.test.v1` manifest through the macro execution engine, revalidating targets before control actions; optional `max_steps` and run `video`. Requires an armed control session (`control.arm`); fails closed otherwise. Returns a `run_id`; fetch results with `test.export_result` (or `test.report_export`)."
    )]
    pub async fn test_run(&self, request: Parameters<TestRunRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("test.run", move || {
            tools::control::with_control_gate(&state, "test.run", None, "test_replay", true, || {
                tools::tests::test_run(&state, request)
            })
        })
        .await
    }

    #[tool(
        name = "test.export_result",
        description = "Export the structured result of a completed test run by `run_id` (from `test.run`). Read-only; not gated. Returns per-step outcomes, assertions, and artifacts; fails if the run ID is unknown."
    )]
    pub async fn test_export_result(
        &self,
        request: Parameters<MacroExportResultRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("test.export_result", move || {
            tools::tests::test_export_result(&state, request)
        })
        .await
    }

    #[tool(
        name = "process.launch",
        description = "Launch a Windows executable `exe` via `CreateProcessW` with optional `args`/`cwd`/`env`, optionally `wait_for_window` for visible PID-owned windows. Not gated. Returns the tracked `pid` and `launch_id` used by `process.kill`/`process.wait_for_exit`/`windows.wait_for_window`; the spawned process is owned by this session."
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
        description = "List Windows processes with metadata, filterable by `name_contains`/`exe_path_contains`, optionally with `include_windows` and `only_mcp_launched`. Read-only; no armed session needed. Returns PIDs and (when requested) owned windows for further binding/control."
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
        description = "Describe process `pid`: executable metadata, whether it is a tracked MCP-launched process, child processes, and top-level windows. Read-only; no armed session needed. Fails if the PID does not exist."
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
        description = "Terminate a process identified by `pid` or `launch_id`, optionally `kill_tree` for its child tree — but only if it was launched and is tracked by this MCP session. Requires an armed control session (`control.arm`); fails closed otherwise. Refuses to kill untracked/arbitrary processes."
    )]
    pub async fn process_kill(
        &self,
        request: Parameters<ProcessKillRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("process.kill", move || {
            tools::control::with_control_gate(
                &state,
                "process.kill",
                None,
                "process_kill",
                true,
                || tools::process::process_kill(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "process.wait_for_exit",
        description = "Wait (up to `timeout_ms`, polling `poll_interval_ms`) for the process identified by `pid` or `launch_id` (from `process.launch`) to exit. Read-only; no armed session needed. Returns exit/lifecycle timing metadata, or indicates timeout if still running."
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
        description = "Click at (`x`,`y`) in the given `coordinate_space` on a bound window via SendInput, with identity revalidation and window-from-point preflight (optionally `fail_if_outside_bound`); `button` defaults to left. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_click(&self, request: Parameters<ClickRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.click", move || {
            tools::control::with_control_gate(
                &state,
                "input.click",
                Some(&bound_id),
                "pointer_input",
                true,
                || tools::input::input_click(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.mouse_move",
        description = "Move the mouse to (`x`,`y`) in the given `coordinate_space` on a bound window via SendInput, with identity revalidation and point preflight (optionally `fail_if_outside_bound`). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_mouse_move(
        &self,
        request: Parameters<MouseMoveRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.mouse_move", move || {
            tools::control::with_control_gate(
                &state,
                "input.mouse_move",
                Some(&bound_id),
                "pointer_input",
                false,
                || tools::input::input_mouse_move(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.double_click",
        description = "Double-click at (`x`,`y`) in the given `coordinate_space` on a bound window via SendInput (optional `button`, `interval_ms`), with identity revalidation and point preflight (optionally `fail_if_outside_bound`). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_double_click(
        &self,
        request: Parameters<DoubleClickRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.double_click", move || {
            tools::control::with_control_gate(
                &state,
                "input.double_click",
                Some(&bound_id),
                "pointer_input",
                true,
                || tools::input::input_double_click(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.drag",
        description = "Drag from (`start_x`,`start_y`) to (`end_x`,`end_y`) in the given `coordinate_space` on a bound window via SendInput (optional `button`, `duration_ms`), with identity revalidation and point preflight on both points (optionally `fail_if_outside_bound`). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_drag(&self, request: Parameters<DragRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.drag", move || {
            tools::control::with_control_gate(
                &state,
                "input.drag",
                Some(&bound_id),
                "pointer_input",
                true,
                || tools::input::input_drag(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.scroll",
        description = "Scroll the wheel by `delta_x`/`delta_y` (default `delta_y` -120) at (`x`,`y`) in the given `coordinate_space` on a bound window via SendInput, with identity revalidation and point preflight (optionally `fail_if_outside_bound`). Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_scroll(
        &self,
        request: Parameters<ScrollRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.scroll", move || {
            tools::control::with_control_gate(
                &state,
                "input.scroll",
                Some(&bound_id),
                "pointer_input",
                true,
                || tools::input::input_scroll(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.key_down",
        description = "Focus a bound window and dispatch a virtual key-down for `key` (e.g. `ctrl`, `shift`, `enter`, `a`, `f5`) via SendInput after identity revalidation. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Pair with `input.key_up` to release held keys."
    )]
    pub async fn input_key_down(&self, request: Parameters<KeyRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.key_down", move || {
            tools::control::with_control_gate(
                &state,
                "input.key_down",
                Some(&bound_id),
                "keyboard_input",
                true,
                || tools::input::input_key_down(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.key_up",
        description = "Focus a bound window and dispatch a virtual key-up for `key` via SendInput after identity revalidation, releasing a key held by `input.key_down`. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_key_up(&self, request: Parameters<KeyRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.key_up", move || {
            tools::control::with_control_gate(
                &state,
                "input.key_up",
                Some(&bound_id),
                "keyboard_input",
                true,
                || tools::input::input_key_up(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.shortcut",
        description = "Focus a bound window and dispatch an ordered key chord `keys` (e.g. `[\"ctrl\",\"shift\",\"s\"]`) via SendInput after identity revalidation, optionally holding `hold_ms` before releasing in reverse order. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise."
    )]
    pub async fn input_shortcut(
        &self,
        request: Parameters<ShortcutRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.shortcut", move || {
            tools::control::with_control_gate(
                &state,
                "input.shortcut",
                Some(&bound_id),
                "keyboard_input",
                true,
                || tools::input::input_shortcut(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "input.delay",
        description = "Sleep for a bounded `duration_ms` and return timing metadata. No bound target; not gated. Used as an explicit pacing step inside macro/test replay manifests."
    )]
    pub async fn input_delay(&self, request: Parameters<DelayRequest>) -> Json<serde_json::Value> {
        let request = request.0;
        run_blocking_tool("input.delay", move || tools::input::input_delay(request)).await
    }

    #[tool(
        name = "input.type_text",
        description = "Focus a bound window and type Unicode `text` via SendInput after identity revalidation. Requires `bound_id` from `windows.bind` and an armed control session (`control.arm`); fails closed otherwise. Prefer `uia.set_value` when the target field exposes a ValuePattern."
    )]
    pub async fn input_type_text(
        &self,
        request: Parameters<TypeTextRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        let bound_id = request.bound_id.clone();
        run_blocking_tool("input.type_text", move || {
            tools::control::with_control_gate(
                &state,
                "input.type_text",
                Some(&bound_id),
                "text_input",
                true,
                || tools::input::input_type_text(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "capture.screenshot_window",
        description = "Capture a PNG screenshot of a window previously bound with `windows.bind`. Read-only; identity is revalidated and fails closed if the window changed process or closed. Returns the artifact id and exact virtual-desktop region. Use mainly for VISUAL verification (`capture.compare_baseline`) — screenshots are slow and token-heavy, so to find or act on controls prefer `uia.find` + `uia.invoke` (structured, no pixels), and fall back to `capture.ocr_region` only when UIA cannot see the element."
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
        description = "Capture a PNG screenshot of a whole display by zero-based `display_index` (see `windows.monitors`). Read-only; no bound target or armed session needed. Returns the capture artifact id and exact virtual-desktop region (x, y, width, height); fails if the index is out of range."
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
        description = "Poll screenshots of a bound window until the image bytes change, bounded by `timeout_ms`/`poll_interval_ms`. Requires `bound_id` from `windows.bind`. Read-only (no armed session needed). Use to wait for a UI repaint after an action; returns replay-safe capture diagnostics or times out."
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

    #[tool(
        name = "capture.video_start",
        description = "Start recording a bound window (`bound_id`) or a `display_index` (default 0) to an animated GIF, with clamped `frame_interval_ms`/`max_duration_ms` and frame-size caps. If `bound_id` is given the target must first be bound via `windows.bind`. Read-only (no armed session needed). Returns a `recording_id`; stop with `capture.video_stop`."
    )]
    pub async fn video_start(
        &self,
        request: Parameters<VideoStartRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("capture.video_start", move || {
            tools::capture::video_start(&state, request)
        })
        .await
    }

    #[tool(
        name = "capture.video_stop",
        description = "Stop a video recording (by `recording_id` from `capture.video_start`, or the active one if omitted) and finalize the GIF. Read-only (no armed session needed). Returns the replay artifact id and metadata; fails if there is no matching active recording."
    )]
    pub async fn video_stop(
        &self,
        request: Parameters<VideoStopRequest>,
    ) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("capture.video_stop", move || {
            tools::capture::video_stop(&state, request)
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
                concat!(
                    "Strict, identity-validated Windows control. Typical flow: `windows.find` -> ",
                    "`windows.bind` (returns bound_id) -> `control.arm` -> act -> verify. ",
                    "All input/click/type, window focus/close, process.kill, registry/filesystem/",
                    "clipboard mutation, and macro/test replay are gated: call `control.arm` first ",
                    "or they fail closed (control_consent_required); `control.revoke`/",
                    "`control.emergency_stop` stops control. ",
                    "To interact with controls, PREFER UI Automation over pixels: `uia.find` to ",
                    "locate (targeted; cheaper than a full `uia.snapshot`), then `uia.invoke`/",
                    "`set_value`/`toggle`/`select`/`set_focus` to act by control pattern - no ",
                    "coordinates, no screenshot, lowest cost. Use `capture.screenshot_window` only ",
                    "for visual verification (screenshots are slow and token-heavy). Use ",
                    "`capture.ocr_region`/`capture.read_text` only as a fallback to read text or ",
                    "find controls UIA cannot expose (custom-rendered/canvas UIs); ",
                    "`capture.ocr_region` returns screen-pixel `center` points usable with ",
                    "`input.click`. Verify results with `uia.get_value`/`assert.*` rather than ",
                    "screenshots. Read-only tools (windows.list/find/describe, *.list, uia.snapshot/",
                    "find/resolve, assert.*, capture.*) need no armed session. Identity (HWND+PID+exe) ",
                    "is revalidated before every action; if a call reports a stale target, re-bind.",
                )
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
            let capture_dir = config
                .capture_dir
                .clone()
                .unwrap_or_else(tools::capture::default_capture_dir);
            let memory_store = if let Some(path) = &config.memory_db_path {
                winctl_memory::MemoryStore::open(path.clone()).unwrap_or_else(|error| {
                    tracing::warn!(
                        memory_db_path = %path.display(),
                        error = %error,
                        "failed to open configured memory store; falling back to default"
                    );
                    default_memory_store()
                })
            } else {
                default_memory_store()
            };
            let state = AppState::with_capture_dir_memory_policy(
                capture_dir,
                memory_store,
                config.policy.clone(),
            );
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
        memory_mutation_enabled = state.policy.memory_mutation_enabled,
        macro_execution_enabled = state.policy.macro_execution_enabled,
        filesystem_roots = ?state.policy.filesystem_roots,
        "winctl-mcp-server starting on stdio"
    );
    log_startup_diagnostics("stdio", None, &state);
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
    log_startup_diagnostics("http", Some(config.listen), &state);

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
        allow_query_token: false,
    };
    let mcp_router =
        Router::new()
            .route_service("/mcp", service)
            .route_layer(middleware::from_fn_with_state(
                auth_state,
                require_bearer_auth,
            ));
    let dashboard_state = DashboardState {
        app_state: state.clone(),
    };
    let dashboard_router = Router::new()
        .route("/dashboard", get(dashboard_html))
        .route("/dashboard/docs", get(dashboard_docs_json))
        .route("/dashboard/memory/delete", post(dashboard_memory_delete))
        .route("/dashboard/state", get(dashboard_state_json))
        .route("/dashboard/uia", get(dashboard_uia_json))
        .route("/dashboard/screenshot", get(dashboard_screenshot_json))
        .route("/dashboard/capture-file", get(dashboard_capture_file))
        .route("/recorder", get(recorder_html))
        .route("/recorder/state", get(recorder_state_json))
        .with_state(dashboard_state);
    let dashboard_router = if config.auth_token.is_some() || !config.listen.ip().is_loopback() {
        dashboard_router.route_layer(middleware::from_fn_with_state(
            HttpAuthState {
                required_token: config.auth_token.clone(),
                allow_query_token: true,
            },
            require_bearer_auth,
        ))
    } else {
        dashboard_router
    };
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/dashboard/assets/{*path}", get(dashboard_asset))
        .merge(dashboard_router)
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
    allow_query_token: bool,
}

#[derive(Clone)]
struct DashboardState {
    app_state: AppState,
}

async fn healthz() -> impl IntoResponse {
    tracing::info!("HTTP health check requested");
    AxumJson(serde_json::json!({
        "ok": true,
        "service": "winctl-mcp-server"
    }))
}

async fn dashboard_html() -> impl IntoResponse {
    Html(DASHBOARD_HTML)
}
async fn dashboard_docs_json() -> impl IntoResponse {
    let docs: Vec<_> = DASHBOARD_DOCS
        .iter()
        .map(
            |(slug, title, html)| serde_json::json!({ "slug": slug, "title": title, "html": html }),
        )
        .collect();
    AxumJson(serde_json::json!({ "ok": true, "docs": docs }))
}
#[derive(serde::Deserialize)]
struct DashboardMemoryDeleteBody {
    id: String,
}
async fn dashboard_memory_delete(
    State(state): State<DashboardState>,
    AxumJson(body): AxumJson<DashboardMemoryDeleteBody>,
) -> impl IntoResponse {
    tracing::info!(memory_id = %body.id, "dashboard memory delete requested");
    AxumJson(tools::memory::memory_delete(
        &state.app_state,
        winctl_memory::MemoryIdRequest { id: body.id },
    ))
}

async fn dashboard_asset(AxumPath(path): AxumPath<String>) -> Response {
    match DASHBOARD_ASSETS.get_file(&path) {
        Some(file) => {
            let content_type = match path.rsplit('.').next() {
                Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("svg") => "image/svg+xml",
                Some("json") => "application/json",
                Some("woff2") => "font/woff2",
                Some("woff") => "font/woff",
                Some("ttf") => "font/ttf",
                Some("png") => "image/png",
                _ => "application/octet-stream",
            };
            ([(CONTENT_TYPE, content_type)], file.contents()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn recorder_html() -> impl IntoResponse {
    Html(RECORDER_HTML)
}

async fn dashboard_state_json(State(state): State<DashboardState>) -> impl IntoResponse {
    let bound: Vec<_> = state
        .app_state
        .bound
        .lock()
        .expect("bound mutex poisoned")
        .values()
        .cloned()
        .collect();
    let launched: Vec<_> = state
        .app_state
        .launched
        .lock()
        .expect("launched process mutex poisoned")
        .values()
        .cloned()
        .collect();
    let memory = tools::memory::memory_list(
        &state.app_state,
        winctl_memory::MemoryListRequest {
            kind: None,
            tags: Vec::new(),
            limit: Some(20),
        },
    );
    let macros = tools::macros::macro_list(
        &state.app_state,
        MacroListRequest {
            kind: None,
            tags: Vec::new(),
            limit: Some(20),
        },
    );
    let macro_results = tools::macros::macro_results_snapshot(&state.app_state, 20);
    let control = tools::control::control_state(&state.app_state)
        .get("control")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    AxumJson(serde_json::json!({
        "ok": true,
        "service": "winctl-mcp-server",
        "version": env!("CARGO_PKG_VERSION"),
        "capture_dir": state.app_state.capture_dir.as_ref(),
        "policy": state.app_state.policy.as_ref(),
        "bound_windows": bound,
        "launched_processes": launched,
        "memory": memory,
        "macros": macros,
        "macro_results": macro_results,
        "control": control,
        "connected_clients": serde_json::Value::Null,
        "recent_requests": [],
        "warnings": [
            "connected client and request-history tracking are not enabled yet"
        ]
    }))
}

async fn dashboard_uia_json(
    State(state): State<DashboardState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(bound_id) = params.get("bound_id").cloned() else {
        return (
            StatusCode::BAD_REQUEST,
            AxumJson(serde_json::json!({
                "ok": false,
                "error": {"code": "bound_id_required", "message": "bound_id query parameter is required"}
            })),
        )
            .into_response();
    };
    let app_state = state.app_state.clone();
    match tokio::task::spawn_blocking(move || {
        tools::uia::uia_snapshot(
            &app_state,
            UiSnapshotRequest {
                bound_id,
                max_depth: Some(8),
                max_elements: Some(1_000),
            },
        )
    })
    .await
    {
        Ok(value) => AxumJson(value).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(serde_json::json!({
                "ok": false,
                "error": {"code": "dashboard_uia_task_failed", "message": error.to_string()}
            })),
        )
            .into_response(),
    }
}

async fn dashboard_screenshot_json(
    State(state): State<DashboardState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(bound_id) = params.get("bound_id").cloned() else {
        return (
            StatusCode::BAD_REQUEST,
            AxumJson(serde_json::json!({
                "ok": false,
                "error": {"code": "bound_id_required", "message": "bound_id query parameter is required"}
            })),
        )
            .into_response();
    };
    let app_state = state.app_state.clone();
    match tokio::task::spawn_blocking(move || {
        tools::capture::screenshot_window(&app_state, bound_id)
    })
    .await
    {
        Ok(value) => AxumJson(value).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(serde_json::json!({
                "ok": false,
                "error": {"code": "dashboard_screenshot_task_failed", "message": error.to_string()}
            })),
        )
            .into_response(),
    }
}

async fn dashboard_capture_file(
    State(state): State<DashboardState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(path) = params.get("path") else {
        return (StatusCode::BAD_REQUEST, "path query parameter is required").into_response();
    };
    let requested = PathBuf::from(path);
    let capture_dir = match std::fs::canonicalize(state.app_state.capture_dir.as_ref()) {
        Ok(path) => path,
        Err(_) => return (StatusCode::NOT_FOUND, "capture directory unavailable").into_response(),
    };
    let requested = match std::fs::canonicalize(&requested) {
        Ok(path) => path,
        Err(_) => return (StatusCode::NOT_FOUND, "capture not found").into_response(),
    };
    if !requested.starts_with(&capture_dir) {
        tracing::warn!(
            requested = %requested.display(),
            capture_dir = %capture_dir.display(),
            "dashboard capture-file rejected path outside capture dir"
        );
        return (StatusCode::FORBIDDEN, "capture path not allowed").into_response();
    }
    let content_type = capture_file_content_type(&requested);
    match tokio::fs::read(&requested).await {
        Ok(bytes) => ([(CONTENT_TYPE, content_type)], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "capture not found").into_response(),
    }
}

fn capture_file_content_type(path: &PathBuf) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("gif") => "image/gif",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

async fn recorder_state_json(State(state): State<DashboardState>) -> impl IntoResponse {
    AxumJson(serde_json::json!({
        "ok": true,
        "windows": winctl::list_windows(),
        "recording": tools::recorder::recorder_state(&state.app_state),
    }))
}

const DASHBOARD_HTML: &str = include_str!("../dashboard/dist/index.html");
// Embed the whole built assets directory so hashed/code-split chunks (e.g. the
// lazy-loaded Mermaid diagram modules) are served alongside `dashboard.js`/`.css`.
static DASHBOARD_ASSETS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/dashboard/dist/assets");

// Generated by build.rs: `pub static DASHBOARD_DOCS: &[(slug, title, html)]`, the
// comrak-rendered tool docs from `docs/*.md` served at `/dashboard/docs`.
include!(concat!(env!("OUT_DIR"), "/dashboard_docs.rs"));

const RECORDER_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>winctl-mcp recorder</title>
  <style>
    :root {
      color-scheme: light dark;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #f7f8fa;
      color: #15171a;
    }
    body { margin: 0; min-height: 100vh; }
    header {
      border-bottom: 1px solid #d9dde3;
      background: #ffffff;
      padding: 16px 24px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 16px;
    }
    h1 { font-size: 18px; margin: 0; letter-spacing: 0; }
    main {
      padding: 20px 24px 32px;
      display: grid;
      grid-template-columns: minmax(280px, 420px) 1fr;
      gap: 16px;
    }
    section {
      background: #ffffff;
      border: 1px solid #d9dde3;
      border-radius: 6px;
      overflow: hidden;
      min-height: 220px;
    }
    h2 {
      font-size: 13px;
      text-transform: uppercase;
      margin: 0;
      padding: 12px 14px;
      border-bottom: 1px solid #e4e7ec;
      color: #5f6b7a;
      letter-spacing: 0;
    }
    label { display: block; font-size: 12px; color: #4e5967; margin: 12px 14px 6px; }
    input, textarea {
      box-sizing: border-box;
      width: calc(100% - 28px);
      margin: 0 14px;
      border: 1px solid #aeb7c2;
      border-radius: 6px;
      padding: 8px 9px;
      font: inherit;
      background: #ffffff;
      color: inherit;
    }
    textarea { min-height: 120px; resize: vertical; }
    button {
      appearance: none;
      border: 1px solid #aeb7c2;
      background: #ffffff;
      color: inherit;
      border-radius: 6px;
      padding: 8px 10px;
      font-size: 13px;
      cursor: pointer;
      margin: 12px 0 0 14px;
    }
    button:hover { background: #eef3f8; }
    pre {
      margin: 0;
      padding: 14px;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      font-size: 12px;
      line-height: 1.5;
    }
    .status { font-size: 13px; color: #4e5967; }
    @media (max-width: 840px) { main { grid-template-columns: 1fr; } }
    @media (prefers-color-scheme: dark) {
      :root { background: #111418; color: #e9edf2; }
      header, section, button, input, textarea { background: #181c22; }
      header, section, h2, button, input, textarea { border-color: #303741; }
      h2 { background: #15191f; color: #aeb7c2; }
      button:hover { background: #202731; }
      label, .status { color: #aeb7c2; }
    }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>winctl-mcp recorder</h1>
      <div class="status" id="status">Loading</div>
    </div>
    <button type="button" onclick="loadState()">Refresh</button>
  </header>
  <main>
    <section>
      <h2>Draft Step</h2>
      <label for="tool">Tool</label>
      <input id="tool" value="input.click">
      <label for="args">Arguments JSON</label>
      <textarea id="args">{"bound_id":"${bound_id}","x":0,"y":0}</textarea>
      <button type="button" onclick="copyStep()">Copy step JSON</button>
      <pre id="draft"></pre>
    </section>
    <section><h2>Recorder State</h2><pre id="state"></pre></section>
    <section><h2>Windows</h2><pre id="windows"></pre></section>
  </main>
  <script>
    const authToken = new URLSearchParams(window.location.search).get('token');
    function authFetch(path) {
      const options = { cache: 'no-store' };
      if (authToken) options.headers = { Authorization: 'Bearer ' + authToken };
      return fetch(path, options);
    }
    function pretty(value) { return JSON.stringify(value, null, 2); }
    async function loadState() {
      const status = document.getElementById('status');
      status.textContent = 'Loading';
      try {
        const response = await authFetch('/recorder/state');
        if (!response.ok) throw new Error('HTTP ' + response.status);
        const data = await response.json();
        document.getElementById('state').textContent = pretty(data.recording);
        document.getElementById('windows').textContent = pretty(data.windows);
        status.textContent = 'Ready';
      } catch (error) {
        status.textContent = String(error);
      }
    }
    async function copyStep() {
      let args = {};
      try { args = JSON.parse(document.getElementById('args').value); } catch (_) {}
      const step = { tool: document.getElementById('tool').value, args };
      document.getElementById('draft').textContent = pretty(step);
      await navigator.clipboard.writeText(pretty(step)).catch(() => {});
    }
    loadState();
  </script>
</body>
</html>"#;

async fn require_bearer_auth(
    State(state): State<HttpAuthState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let Some(required_token) = state.required_token else {
        return next.run(request).await;
    };
    let authorized = http_request_authorized(
        &headers,
        request.uri(),
        &required_token,
        state.allow_query_token,
    );
    if authorized {
        next.run(request).await
    } else {
        tracing::warn!("HTTP MCP request rejected by bearer auth");
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }
}

fn http_request_authorized(
    headers: &HeaderMap,
    uri: &Uri,
    required_token: &str,
    allow_query_token: bool,
) -> bool {
    if headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .map(|header| header == format!("Bearer {required_token}"))
        .unwrap_or(false)
    {
        return true;
    }

    allow_query_token && query_token_authorized(uri.query(), required_token)
}

fn query_token_authorized(query: Option<&str>, required_token: &str) -> bool {
    let Some(query) = query else {
        return false;
    };
    query.split('&').any(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        percent_decode_query_component(name).as_deref() == Some("token")
            && percent_decode_query_component(value).as_deref() == Some(required_token)
    })
}

fn percent_decode_query_component(component: &str) -> Option<String> {
    let bytes = component.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return None;
                }
                let high = hex_value(bytes[index + 1])?;
                let low = hex_value(bytes[index + 2])?;
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
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
    config_file: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            command: Command::Serve(ServeConfig::stdio()),
            log_file: None,
            config_file: None,
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
    memory_db_path: Option<PathBuf>,
    policy: SecurityPolicy,
    config_file: Option<PathBuf>,
}

impl ServeConfig {
    fn stdio() -> Self {
        Self {
            transport: TransportMode::Stdio,
            listen: default_http_listen(),
            auth_token: None,
            capture_dir: None,
            memory_db_path: std::env::var_os("WINCTL_MEMORY_DB").map(PathBuf::from),
            policy: SecurityPolicy::default(),
            config_file: None,
        }
    }

    fn from_file(file: &WinctlConfigFile) -> anyhow::Result<Self> {
        let mut config = Self::stdio();
        if let Some(transport) = &file.transport {
            if let Some(mode) = &transport.mode {
                config.transport = parse_transport_str(mode)?;
            }
            if let Some(listen) = &transport.listen {
                config.listen = listen
                    .parse()
                    .with_context(|| format!("invalid configured listen address {listen}"))?;
            }
        }
        if let Some(auth) = &file.auth {
            if let Some(token) = &auth.token {
                config.auth_token = Some(token.clone());
            }
        }
        if let Some(paths) = &file.paths {
            if let Some(path) = &paths.capture_dir {
                config.capture_dir = Some(path.clone());
            }
            if let Some(path) = &paths.memory_db {
                config.memory_db_path = Some(path.clone());
            }
            if let Some(path) = &paths.artifact_dir {
                config.policy.artifact_dir = Some(path.clone());
            }
            if let Some(roots) = &paths.filesystem_roots {
                config.policy.filesystem_roots = roots.clone();
            }
        }
        if let Some(policy) = &file.policy {
            if let Some(value) = policy.enable_filesystem_mutation {
                config.policy.enable_filesystem_mutation = value;
            }
            if let Some(value) = policy.enable_clipboard_write {
                config.policy.enable_clipboard_write = value;
            }
            if let Some(value) = policy.enable_registry_mutation {
                config.policy.enable_registry_mutation = value;
            }
            if let Some(value) = policy.allow_private_network {
                config.policy.allow_private_network = value;
            }
            if let Some(value) = policy.memory_mutation_enabled {
                config.policy.memory_mutation_enabled = value;
            }
            if let Some(value) = policy.macro_execution_enabled {
                config.policy.macro_execution_enabled = value;
            }
            if let Some(value) = policy.macro_destructive_tools_allowed {
                config.policy.macro_destructive_tools_allowed = value;
            }
            if let Some(value) = policy.max_macro_runtime_ms {
                config.policy.max_macro_runtime_ms = Some(value);
            }
            if let Some(value) = policy.max_macro_steps {
                config.policy.max_macro_steps = Some(value);
            }
            if let Some(value) = policy.screenshot_retention_count {
                config.policy.screenshot_retention_count = Some(value);
            }
            if let Some(values) = &policy.tool_allowlist {
                config.policy.tool_allowlist = values.clone();
            }
            if let Some(values) = &policy.tool_denylist {
                config.policy.tool_denylist = values.clone();
            }
        }
        if let Some(embedding) = &file.embedding {
            if let Some(path) = &embedding.model_path {
                config.policy.embedding_model_path = Some(path.clone());
            }
            if let Some(dimension) = embedding.dimension {
                config.policy.embedding_dimension = Some(dimension);
            }
        }
        if let Some(macros) = &file.macro_execution {
            if let Some(value) = macros.enabled {
                config.policy.macro_execution_enabled = value;
            }
            if let Some(value) = macros.allow_destructive_tools {
                config.policy.macro_destructive_tools_allowed = value;
            }
            if let Some(value) = macros.max_runtime_ms {
                config.policy.max_macro_runtime_ms = Some(value);
            }
            if let Some(value) = macros.max_steps {
                config.policy.max_macro_steps = Some(value);
            }
        }
        Ok(config)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct WinctlConfigFile {
    transport: Option<TransportFileConfig>,
    auth: Option<AuthFileConfig>,
    logging: Option<LoggingFileConfig>,
    paths: Option<PathsFileConfig>,
    policy: Option<PolicyFileConfig>,
    embedding: Option<EmbeddingFileConfig>,
    macro_execution: Option<MacroExecutionFileConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TransportFileConfig {
    mode: Option<String>,
    listen: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AuthFileConfig {
    token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LoggingFileConfig {
    log_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PathsFileConfig {
    capture_dir: Option<PathBuf>,
    artifact_dir: Option<PathBuf>,
    filesystem_roots: Option<Vec<PathBuf>>,
    memory_db: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PolicyFileConfig {
    enable_filesystem_mutation: Option<bool>,
    enable_clipboard_write: Option<bool>,
    enable_registry_mutation: Option<bool>,
    allow_private_network: Option<bool>,
    memory_mutation_enabled: Option<bool>,
    macro_execution_enabled: Option<bool>,
    macro_destructive_tools_allowed: Option<bool>,
    max_macro_runtime_ms: Option<u64>,
    max_macro_steps: Option<usize>,
    screenshot_retention_count: Option<usize>,
    tool_allowlist: Option<Vec<String>>,
    tool_denylist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct EmbeddingFileConfig {
    model_path: Option<PathBuf>,
    dimension: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MacroExecutionFileConfig {
    enabled: Option<bool>,
    allow_destructive_tools: Option<bool>,
    max_runtime_ms: Option<u64>,
    max_steps: Option<usize>,
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
            } else if arg == "--config" {
                index += 1;
                let Some(path) = args.get(index) else {
                    anyhow::bail!("--config requires a path");
                };
                cli.config_file = Some(PathBuf::from(path));
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
                let (serve, next_index, log_file, config_file) =
                    parse_serve_args(&args, index + 1, cli.config_file.clone())?;
                cli.command = Command::Serve(serve);
                if let Some(log_file) = log_file {
                    cli.log_file = Some(log_file);
                }
                if let Some(config_file) = config_file {
                    cli.config_file = Some(config_file);
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
    inherited_config_file: Option<PathBuf>,
) -> anyhow::Result<(ServeConfig, usize, Option<PathBuf>, Option<PathBuf>)> {
    let config_file = scan_config_file(args, index).or(inherited_config_file);
    let file_config = match &config_file {
        Some(path) => Some(load_config_file(path)?),
        None => None,
    };
    let mut config = file_config
        .as_ref()
        .map(ServeConfig::from_file)
        .transpose()?
        .unwrap_or_else(ServeConfig::stdio);
    config.config_file = config_file.clone();
    let mut log_file = file_config
        .as_ref()
        .and_then(|config| config.logging.as_ref())
        .and_then(|logging| logging.log_file.clone());
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
        } else if arg == "--config" {
            index += 1;
            if args.get(index).is_none() {
                anyhow::bail!("--config requires a path");
            }
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
    Ok((config, index, log_file, config_file))
}

fn parse_transport(value: &OsString) -> anyhow::Result<TransportMode> {
    parse_transport_str(value.to_string_lossy().as_ref())
}

fn parse_transport_str(value: &str) -> anyhow::Result<TransportMode> {
    match value {
        "stdio" => Ok(TransportMode::Stdio),
        "http" => Ok(TransportMode::Http),
        other => anyhow::bail!("unsupported transport {other}; expected stdio or http"),
    }
}

fn scan_config_file(args: &[OsString], mut index: usize) -> Option<PathBuf> {
    while index < args.len() {
        if args[index] == "--config" {
            return args.get(index + 1).map(PathBuf::from);
        }
        index += 1;
    }
    None
}

fn load_config_file(path: &PathBuf) -> anyhow::Result<WinctlConfigFile> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse config file {}", path.display()))
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

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn env_paths(name: &str) -> Vec<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn log_startup_diagnostics(transport: &str, listen: Option<SocketAddr>, state: &AppState) {
    tracing::info!(
        transport = transport,
        listen = ?listen,
        capture_dir = %state.capture_dir.as_ref().display(),
        filesystem_roots = ?state.policy.filesystem_roots,
        artifact_dir = ?state.policy.artifact_dir,
        filesystem_mutation = state.policy.enable_filesystem_mutation,
        clipboard_write = state.policy.enable_clipboard_write,
        registry_mutation = state.policy.enable_registry_mutation,
        allow_private_network = state.policy.allow_private_network,
        memory_mutation_enabled = state.policy.memory_mutation_enabled,
        macro_execution_enabled = state.policy.macro_execution_enabled,
        macro_destructive_tools_allowed = state.policy.macro_destructive_tools_allowed,
        tool_allowlist = ?state.policy.tool_allowlist,
        tool_denylist = ?state.policy.tool_denylist,
        "effective winctl-mcp startup policy"
    );
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
                "app.launch",
                "artifact.export",
                "assert.clipboard",
                "assert.element",
                "assert.pixel_color",
                "assert.text_visible",
                "assert.window_count",
                "browser.assert",
                "browser.describe",
                "browser.extract_content",
                "browser.list",
                "browser.screenshot_checkpoint",
                "browser.wait_for_navigation",
                "build.run",
                "capture.compare_baseline",
                "capture.ocr_region",
                "capture.read_text",
                "capture.screenshot_display",
                "capture.screenshot_window",
                "capture.video_start",
                "capture.video_stop",
                "capture.wait_for_window_image_change",
                "clipboard.read",
                "clipboard.write",
                "control.arm",
                "control.consent",
                "control.emergency_stop",
                "control.notify",
                "control.revoke",
                "control.state",
                "diagnostics.crash_report",
                "dialogs.invoke_button",
                "dialogs.list",
                "filesystem.copy",
                "filesystem.delete",
                "filesystem.list",
                "filesystem.move",
                "filesystem.read",
                "filesystem.search",
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
                "macro.abort",
                "macro.dry_run",
                "macro.export_result",
                "macro.get",
                "macro.list",
                "macro.promote",
                "macro.run",
                "macro.run_step",
                "macro.type_secret",
                "macro.validate",
                "memory.delete",
                "memory.get",
                "memory.list",
                "memory.reindex",
                "memory.remember",
                "memory.search",
                "memory.update",
                "network.fetch",
                "network.scrape",
                "notifications.list",
                "process.describe",
                "process.diagnostics",
                "process.kill",
                "process.launch",
                "process.list",
                "process.metrics",
                "process.wait_for_exit",
                "recorder.export_manifest",
                "recorder.pause",
                "recorder.record_step",
                "recorder.start",
                "recorder.state",
                "recorder.stop",
                "registry.delete",
                "registry.list",
                "registry.read",
                "registry.write",
                "secret.delete",
                "secret.list",
                "secret.set",
                "server.config",
                "server.ping",
                "test.dry_run",
                "test.export_result",
                "test.report_export",
                "test.run",
                "test.validate",
                "uia.expand_collapse",
                "uia.find",
                "uia.get_value",
                "uia.invoke",
                "uia.range_value",
                "uia.resolve",
                "uia.scroll_into_view",
                "uia.select",
                "uia.set_focus",
                "uia.set_value",
                "uia.snapshot",
                "uia.toggle",
                "uia.wait_for_element",
                "web.a11y.snapshot",
                "web.cdp.evaluate",
                "web.cdp.list_targets",
                "web.dom.snapshot",
                "web.network.events",
                "web.style.inspect",
                "windows.bind",
                "windows.close",
                "windows.describe",
                "windows.find",
                "windows.focus",
                "windows.for_process",
                "windows.foreground_diagnostics",
                "windows.list",
                "windows.maximize",
                "windows.minimize",
                "windows.monitors",
                "windows.move",
                "windows.resize",
                "windows.restore",
                "windows.wait_for_state",
                "windows.wait_for_window",
                "windows.window_from_point",
            ]
        );
    }

    #[test]
    fn memory_tools_round_trip() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );

        let remembered = tools::memory::memory_remember(
            &state,
            winctl_memory::RememberRequest {
                kind: "test_procedure".into(),
                title: "Betty settings smoke test".into(),
                text: "Launch Betty and verify Settings opens.".into(),
                manifest_json: Some(serde_json::json!({"version": "winctl.macro.v1"})),
                tags: vec!["betty".into(), "settings".into()],
                app_identity_json: Some(serde_json::json!({"executable_name": "Betty.exe"})),
                target_identity_json: None,
            },
        );
        assert_eq!(remembered["ok"], true);
        let id = remembered["item"]["id"].as_str().unwrap().to_owned();

        let search = tools::memory::memory_search(
            &state,
            winctl_memory::MemorySearchRequest {
                query: Some("open settings".into()),
                tags: vec!["betty".into()],
                limit: Some(5),
                ..Default::default()
            },
        );
        assert_eq!(search["ok"], true);
        assert_eq!(search["results"][0]["item"]["id"], id);
        assert!(search["results"][0]["vector_score"].as_f64().unwrap() > 0.0);

        let deleted =
            tools::memory::memory_delete(&state, winctl_memory::MemoryIdRequest { id: id.clone() });
        assert_eq!(deleted["ok"], true);
        assert_eq!(deleted["deleted"], true);
    }

    #[test]
    fn macro_run_executes_delay_step_and_exports_result() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let value = tools::macros::macro_run(
            &state,
            MacroRunRequest {
                manifest: Some(delay_manifest()),
                memory_id: None,
                max_steps: None,
                video: None,
            },
        );
        assert_eq!(value["ok"], true);
        let run_id = value["run_id"].as_str().unwrap().to_owned();
        assert_eq!(value["result"]["status"], "succeeded");
        assert_eq!(value["result"]["step_results"][0]["tool"], "input.delay");

        let exported =
            tools::macros::macro_export_result(&state, MacroExportResultRequest { run_id });
        assert_eq!(exported["ok"], true);
        assert_eq!(exported["result"]["status"], "succeeded");
    }

    #[test]
    fn recorder_builds_macro_manifest() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let started = tools::recorder::recorder_start(
            &state,
            RecorderStartRequest {
                title: "Recorded flow".into(),
                description: Some("Recorded by test".into()),
                tags: vec!["test".into()],
                app_identity: None,
                capture_input: Some(false),
            },
        );
        assert_eq!(started["ok"], true);

        let recorded = tools::recorder::recorder_record_step(
            &state,
            RecorderRecordStepRequest {
                id: Some("delay".into()),
                tool: "input.delay".into(),
                args: Some(serde_json::json!({"duration_ms": 1})),
                target: None,
                timeout_ms: None,
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: None,
                note: Some("wait briefly".into()),
            },
        );
        assert_eq!(recorded["ok"], true);
        assert_eq!(recorded["step_count"], 1);

        let stopped = tools::recorder::recorder_stop(
            &state,
            RecorderStopRequest {
                save_to_memory: false,
            },
        );
        assert_eq!(stopped["ok"], true);
        assert_eq!(stopped["manifest"]["steps"][0]["tool"], "input.delay");
        assert_eq!(stopped["validation"]["valid"], true);
    }

    #[test]
    fn recorder_defaults_target_bound_steps_to_current_target() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let started = tools::recorder::recorder_start(
            &state,
            RecorderStartRequest {
                title: "Recorded click".into(),
                description: None,
                tags: Vec::new(),
                app_identity: None,
                capture_input: Some(false),
            },
        );
        assert_eq!(started["ok"], true);

        let recorded = tools::recorder::recorder_record_step(
            &state,
            RecorderRecordStepRequest {
                id: Some("click".into()),
                tool: "input.click".into(),
                args: Some(serde_json::json!({
                    "x": 0.5,
                    "y": 0.5,
                    "coordinate_space": "normalized_window"
                })),
                target: None,
                timeout_ms: None,
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: None,
                note: None,
            },
        );

        assert_eq!(recorded["ok"], true);
        assert_eq!(recorded["step"]["target"]["type"], "current");
    }

    #[test]
    fn test_manifest_runs_through_macro_engine() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let manifest = winctl_macro::TestManifest {
            version: winctl_macro::TEST_MANIFEST_VERSION.into(),
            kind: "test_procedure".into(),
            title: "Delay test".into(),
            description: "Delay-only test manifest".into(),
            tags: vec!["test".into()],
            macro_manifest: delay_manifest(),
            artifact_paths: Vec::new(),
            diagnostics: serde_json::Value::Null,
        };
        let dry_run = tools::tests::test_dry_run(TestManifestRequest {
            manifest: manifest.clone(),
        });
        assert_eq!(dry_run["ok"], true);
        let run = tools::tests::test_run(
            &state,
            TestRunRequest {
                manifest,
                max_steps: None,
                video: None,
            },
        );
        assert_eq!(run["ok"], true);
        assert_eq!(run["result"]["status"], "succeeded");
    }

    #[test]
    fn macro_promote_stores_session_and_memory_backed_macro() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let promoted = tools::macros::macro_promote(
            &state,
            MacroPromoteRequest {
                manifest: delay_manifest(),
                remember: true,
            },
        );
        assert_eq!(promoted["ok"], true);
        assert!(promoted["macro"]["memory_id"].as_str().is_some());

        let listed = tools::macros::macro_list(&state, MacroListRequest::default());
        assert_eq!(listed["ok"], true);
        assert_eq!(listed["session_macros"].as_array().unwrap().len(), 1);
        assert_eq!(listed["memory_items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn memory_backed_macro_dry_run_and_run_update_use_metadata() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let promoted = tools::macros::macro_promote(
            &state,
            MacroPromoteRequest {
                manifest: delay_manifest(),
                remember: true,
            },
        );
        let memory_id = promoted["macro"]["memory_id"].as_str().unwrap().to_owned();

        let dry_run = tools::macros::macro_dry_run(
            &state,
            MacroDryRunRequest {
                manifest: None,
                memory_id: Some(memory_id.clone()),
            },
        );
        assert_eq!(dry_run["ok"], true);
        assert_eq!(dry_run["source_memory_id"], memory_id);

        let run = tools::macros::macro_run(
            &state,
            MacroRunRequest {
                manifest: None,
                memory_id: Some(memory_id.clone()),
                max_steps: None,
                video: None,
            },
        );
        assert_eq!(run["ok"], true);
        assert_eq!(run["source_memory_id"], memory_id);

        let item = state
            .memory
            .lock()
            .unwrap()
            .get_without_touch(&memory_id)
            .unwrap()
            .unwrap();
        assert_eq!(item.use_count, 1);
        assert!(item.last_used_at.is_some());
    }

    #[test]
    fn macro_image_checkpoint_runs_baseline_comparison() {
        let capture_dir = std::env::temp_dir().join(format!(
            "winctl-mcp-image-checkpoint-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&capture_dir).unwrap();
        let baseline_path = capture_dir.join("baseline.png");
        let actual_path = capture_dir.join("actual.png");
        let image = image::ImageBuffer::from_pixel(3, 3, image::Rgba([20u8, 40, 60, 255]));
        image.save(&baseline_path).unwrap();
        image.save(&actual_path).unwrap();

        let state = AppState::with_capture_dir_and_memory(
            capture_dir.clone(),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let manifest = checkpoint_manifest(vec![winctl_macro::MacroStep {
            id: "image-checkpoint".into(),
            tool: "macro.assert_image_checkpoint".into(),
            args: serde_json::json!({
                "actual_path": actual_path,
                "baseline_path": baseline_path,
                "max_different_pixels": 0
            }),
            target: Some(winctl_macro::MacroTarget::ImageCheckpoint {
                checkpoint_id: "baseline".into(),
                path: None,
                checksum_sha256: None,
            }),
            timeout_ms: None,
            required: true,
            continue_on_failure: false,
            coordinate_fallback: None,
            audit: Default::default(),
        }]);

        let value = tools::macros::macro_run(
            &state,
            MacroRunRequest {
                manifest: Some(manifest),
                memory_id: None,
                max_steps: None,
                video: None,
            },
        );

        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["status"], "succeeded");
        let step_output = &value["result"]["step_results"][0]["output"];
        assert_eq!(step_output["assertion"], "image_checkpoint");
        assert_eq!(step_output["comparison"]["different_pixels"], 0);
        assert_ne!(
            step_output["error"]["code"],
            "macro_assertion_not_implemented"
        );
    }

    #[test]
    fn macro_text_checkpoint_fails_macro_when_text_is_missing() {
        let state = AppState::with_capture_dir_and_memory(
            std::env::temp_dir().join("winctl-mcp-test-captures"),
            winctl_memory::MemoryStore::open_in_memory().unwrap(),
        );
        let manifest = checkpoint_manifest(vec![winctl_macro::MacroStep {
            id: "text-checkpoint".into(),
            tool: "macro.assert_text_checkpoint".into(),
            args: serde_json::json!({
                "actual_text": "Settings pane is ready",
                "contains": "missing text"
            }),
            target: Some(winctl_macro::MacroTarget::TextCheckpoint {
                text: "missing text".into(),
            }),
            timeout_ms: None,
            required: true,
            continue_on_failure: false,
            coordinate_fallback: None,
            audit: Default::default(),
        }]);

        let value = tools::macros::macro_run(
            &state,
            MacroRunRequest {
                manifest: Some(manifest),
                memory_id: None,
                max_steps: None,
                video: None,
            },
        );

        assert_eq!(value["ok"], false);
        assert_eq!(value["result"]["status"], "failed");
        let step = &value["result"]["step_results"][0];
        assert_eq!(step["status"], "failed");
        assert_eq!(step["error"]["code"], "macro_assertion_failed");
        assert_eq!(step["output"]["assertion"], "text_checkpoint");
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
    fn http_auth_accepts_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret".parse().unwrap(),
        );
        let uri: Uri = "/mcp".parse().unwrap();

        assert!(http_request_authorized(&headers, &uri, "secret", false));
    }

    #[test]
    fn http_auth_accepts_query_token_only_when_enabled() {
        let headers = HeaderMap::new();
        let uri: Uri = "/dashboard?token=s%20e%2Bcret".parse().unwrap();

        assert!(http_request_authorized(&headers, &uri, "s e+cret", true));
        assert!(!http_request_authorized(&headers, &uri, "s e+cret", false));
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

    #[test]
    fn cli_loads_toml_config_policy() {
        let path =
            std::env::temp_dir().join(format!("winctl-config-test-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            r#"
[transport]
mode = "http"
listen = "127.0.0.1:8765"

[auth]
token = "test-token"

[paths]
capture_dir = "D:/captures"
filesystem_roots = ["D:/captures", "D:/artifacts"]
memory_db = "D:/memory.sqlite"
artifact_dir = "D:/artifacts"

[policy]
enable_filesystem_mutation = true
enable_clipboard_write = true
enable_registry_mutation = true
allow_private_network = true
memory_mutation_enabled = false
macro_execution_enabled = false
macro_destructive_tools_allowed = false
max_macro_steps = 12
tool_denylist = ["registry.delete"]

[embedding]
model_path = "D:/models/minilm.onnx"
dimension = 384
"#,
        )
        .unwrap();
        let cli = Cli::parse([
            OsString::from("serve"),
            OsString::from("--config"),
            OsString::from(path.as_os_str()),
        ])
        .unwrap();
        let _ = std::fs::remove_file(&path);
        match cli.command {
            Command::Serve(config) => {
                assert_eq!(config.transport, TransportMode::Http);
                assert_eq!(config.auth_token.as_deref(), Some("test-token"));
                assert_eq!(config.capture_dir, Some(PathBuf::from("D:/captures")));
                assert_eq!(
                    config.memory_db_path,
                    Some(PathBuf::from("D:/memory.sqlite"))
                );
                assert!(config.policy.enable_filesystem_mutation);
                assert!(config.policy.allow_private_network);
                assert!(!config.policy.memory_mutation_enabled);
                assert!(!config.policy.macro_execution_enabled);
                assert_eq!(config.policy.max_macro_steps, Some(12));
                assert_eq!(config.policy.tool_denylist, vec!["registry.delete"]);
            }
            _ => panic!("expected serve command"),
        }
    }

    fn delay_manifest() -> winctl_macro::MacroManifest {
        winctl_macro::MacroManifest {
            version: winctl_macro::MACRO_MANIFEST_VERSION.into(),
            kind: "test_procedure".into(),
            title: "Delay-only macro".into(),
            description: "Exercise the macro runner with a non-mutating timing step.".into(),
            tags: vec!["test".into()],
            app_identity: None,
            launch: None,
            bind: None,
            preconditions: vec![],
            steps: vec![winctl_macro::MacroStep {
                id: "delay".into(),
                tool: "input.delay".into(),
                args: serde_json::json!({"duration_ms": 1}),
                target: None,
                timeout_ms: None,
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: Default::default(),
            }],
            waits: vec![],
            assertions: vec![],
            cleanup: vec![],
            artifacts: Default::default(),
            replay: Default::default(),
        }
    }

    fn checkpoint_manifest(
        assertions: Vec<winctl_macro::MacroStep>,
    ) -> winctl_macro::MacroManifest {
        winctl_macro::MacroManifest {
            version: winctl_macro::MACRO_MANIFEST_VERSION.into(),
            kind: "test_procedure".into(),
            title: "Checkpoint macro".into(),
            description: "Exercise macro checkpoint assertions.".into(),
            tags: vec!["test".into()],
            app_identity: Some(winctl_macro::AppIdentity {
                executable_name: Some("Fixture.exe".into()),
                executable_path: None,
                product_name: None,
                required: true,
                extra: serde_json::Value::Null,
            }),
            launch: None,
            bind: Some(winctl_macro::BindingStrategy {
                strategy: "bound_window".into(),
                required_executable: Some("Fixture.exe".into()),
                expected_identity_json: None,
                allow_child_process_windows: false,
            }),
            preconditions: vec![],
            steps: vec![],
            waits: vec![],
            assertions,
            cleanup: vec![],
            artifacts: Default::default(),
            replay: Default::default(),
        }
    }
}
