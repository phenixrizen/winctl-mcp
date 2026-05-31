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
use axum::extract::{Path as AxumPath, State};
use axum::http::{header::CONTENT_TYPE, HeaderMap, StatusCode, Uri};
use axum::middleware;
use axum::response::{Html, IntoResponse, Response};
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
    pub policy: Arc<SecurityPolicy>,
    pub launched: Arc<Mutex<HashMap<String, TrackedProcess>>>,
    pub memory: Arc<Mutex<winctl_memory::MemoryStore>>,
    pub macro_runtime: Arc<Mutex<tools::macros::MacroRuntimeState>>,
    pub recorder_runtime: Arc<Mutex<tools::recorder::RecorderRuntimeState>>,
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
            control_runtime: Arc::new(Mutex::new(tools::control::ControlRuntimeState::default())),
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

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct AppLaunchRequest {
    pub mode: winctl::AppLaunchMode,
    pub target: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WindowMoveRequest {
    pub bound_id: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct WindowResizeRequest {
    pub bound_id: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct WindowsForProcessRequest {
    pub pid: Option<u32>,
    pub launch_id: Option<String>,
    #[serde(default)]
    pub include_child_process_windows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserListRequest {
    pub browser: Option<winctl::BrowserKind>,
    pub pid: Option<u32>,
    #[serde(default)]
    pub include_windows: bool,
    #[serde(default)]
    pub only_mcp_launched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserDescribeRequest {
    pub bound_id: Option<String>,
    pub pid: Option<u32>,
    pub hwnd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserWaitForNavigationRequest {
    pub bound_id: String,
    pub title_contains: Option<String>,
    pub title_not_contains: Option<String>,
    pub timeout_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserAssertRequest {
    pub bound_id: String,
    pub browser: Option<winctl::BrowserKind>,
    pub title_contains: Option<String>,
    pub class_name_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct BrowserExtractContentRequest {
    pub bound_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ClipboardReadRequest {
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ClipboardWriteRequest {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemReadRequest {
    pub path: String,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemListRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    pub max_entries: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemSearchRequest {
    pub root: String,
    pub pattern: String,
    pub max_results: Option<usize>,
    pub max_file_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemCopyRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemMoveRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FilesystemDeleteRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ArtifactExportRequest {
    pub source_path: String,
    pub destination_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryListRequest {
    pub hive: winctl::RegistryHive,
    pub path: String,
    #[serde(default)]
    pub include_values: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryReadRequest {
    pub hive: winctl::RegistryHive,
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryWriteRequest {
    pub hive: winctl::RegistryHive,
    pub path: String,
    pub name: Option<String>,
    pub kind: winctl::RegistryValueKind,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RegistryDeleteRequest {
    pub hive: winctl::RegistryHive,
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct NotificationsListRequest {
    pub max_items: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ProcessDiagnosticsRequest {
    pub pid: u32,
    #[serde(default)]
    pub include_windows: bool,
    #[serde(default)]
    pub include_children: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct NetworkFetchRequest {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub follow_redirects: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct NetworkScrapeRequest {
    pub url: String,
    pub timeout_ms: Option<u64>,
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub follow_redirects: bool,
    #[serde(default = "default_true")]
    pub include_links: bool,
    #[serde(default = "default_true")]
    pub include_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderStartRequest {
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub app_identity: Option<winctl_macro::AppIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RecorderRecordStepRequest {
    pub id: Option<String>,
    pub tool: String,
    pub args: Option<serde_json::Value>,
    pub target: Option<winctl_macro::MacroTarget>,
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub continue_on_failure: bool,
    pub coordinate_fallback: Option<winctl_macro::CoordinateFallbackMetadata>,
    pub audit: Option<winctl_macro::StepAudit>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderStopRequest {
    #[serde(default)]
    pub save_to_memory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct RecorderExportRequest {
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TestManifestRequest {
    pub manifest: winctl_macro::TestManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TestRunRequest {
    pub manifest: winctl_macro::TestManifest,
    pub max_steps: Option<usize>,
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

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ControlArmRequest {
    pub session_id: Option<String>,
    pub bound_id: Option<String>,
    pub allow_for_ms: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ControlConsentRequest {
    pub decision: String,
    pub session_id: Option<String>,
    pub bound_id: Option<String>,
    pub allow_for_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct ControlRevokeRequest {
    pub session_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ControlNotifyRequest {
    pub tool_name: String,
    pub bound_id: Option<String>,
    pub action_kind: Option<String>,
    pub session_id: Option<String>,
    pub countdown_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroManifestRequest {
    pub manifest: winctl_macro::MacroManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroDryRunRequest {
    pub manifest: Option<winctl_macro::MacroManifest>,
    pub memory_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroRunRequest {
    pub manifest: Option<winctl_macro::MacroManifest>,
    pub memory_id: Option<String>,
    pub max_steps: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroRunStepRequest {
    pub manifest: Option<winctl_macro::MacroManifest>,
    pub memory_id: Option<String>,
    pub step_id: String,
    #[serde(default)]
    pub context_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroAbortRequest {
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroGetRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema, Default)]
pub struct MacroListRequest {
    pub kind: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroPromoteRequest {
    pub manifest: winctl_macro::MacroManifest,
    #[serde(default = "default_true")]
    pub remember: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MacroExportResultRequest {
    pub run_id: String,
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
        description = "Return a minimal health response without touching Win32 APIs."
    )]
    pub async fn server_ping(&self) -> Json<serde_json::Value> {
        tracing::info!("server.ping requested");
        Json(serde_json::json!({"ok": true, "pong": true}))
    }

    #[tool(
        name = "server.config",
        description = "Return effective runtime configuration and security policy diagnostics."
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
        description = "Return desktop-control gate state, active target identity, consent decision, and recent control events."
    )]
    pub async fn control_state(&self) -> Json<serde_json::Value> {
        let state = self.state.clone();
        run_blocking_tool("control.state", move || tools::control::control_state(&state)).await
    }

    #[tool(
        name = "control.arm",
        description = "Arm desktop control for a session or bound target before sensitive focus, input, close, kill, mutation, macro, test, or UIA actions."
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
        description = "Record an allow_once, allow_session, deny, or revoke_session decision for desktop-control actions."
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
        description = "Record a pending desktop-control notification event for tray or dashboard display."
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
        description = "Emergency-stop desktop control and reject future sensitive actions until rearmed."
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
        description = "Alias for control.revoke; immediately stops desktop control and requires a new arm/consent decision."
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
        description = "Explicitly store a structured memory item with searchable text, tags, app/target identity, and sqlite-vec embedding metadata."
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
        description = "Search remembered procedures, observations, macros, and recipes using hybrid sqlite-vec, FTS5, tag, identity, recency, and usefulness ranking."
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
        description = "Fetch one memory item by ID and update its explicit use metadata."
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
        description = "Explicitly update a remembered item and rebuild its FTS5 and sqlite-vec indexes."
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
        description = "Explicitly delete one remembered item by ID and remove it from memory indexes."
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
        description = "List remembered items with optional kind and tag filtering."
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
        description = "Rebuild memory FTS5 and sqlite-vec indexes for the local memory database."
    )]
    pub async fn memory_reindex(&self) -> Json<serde_json::Value> {
        let state = self.state.clone();
        run_blocking_tool("memory.reindex", move || {
            tools::memory::memory_reindex(&state)
        })
        .await
    }

    #[tool(
        name = "macro.validate",
        description = "Validate a winctl macro manifest version, tool names, target identity requirements, and coordinate fallback metadata."
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
        description = "Build a dry-run plan for a macro manifest without performing mutating UI actions."
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
        description = "Execute a macro manifest through the existing MCP tool implementations with target revalidation before control actions."
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
        description = "Execute one macro step by ID for stepwise debugging."
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
        name = "macro.abort",
        description = "Request a safe abort for an active macro run."
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
        description = "List session-promoted macros and memory-backed macro items."
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
        description = "Get a promoted macro manifest by session macro ID or memory item ID."
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
        description = "Promote an approved macro manifest into the session registry and optionally explicit memory storage."
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
        description = "Export a structured macro run result and artifact metadata by run ID."
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
        name = "windows.move",
        description = "Move a bound window after revalidating HWND, PID, and executable identity."
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
        description = "Resize a bound window after revalidating HWND, PID, and executable identity."
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
        description = "Minimize a bound window after revalidating stable identity."
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
        description = "Maximize a bound window after revalidating stable identity."
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
        description = "Restore a bound window after revalidating stable identity."
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
        description = "Post WM_CLOSE to a bound window after revalidating stable identity."
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
        description = "Return foreground and replay diagnostics for a bound window after identity revalidation."
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
        description = "List visible top-level windows for a PID or MCP launch ID, with explicit child-process policy."
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
        description = "Launch an executable, protocol handler, packaged app, or Start Menu app target without shell command concatenation."
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
        description = "List Chrome, Edge, and Firefox process/window state with explicit PID/HWND identity metadata."
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
        description = "Describe one browser target by bound window, PID, or HWND without tab-title selection."
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
        description = "Wait for a bound browser window title transition while revalidating browser PID/HWND/executable identity."
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
        description = "Assert browser kind and window title/class conditions against a revalidated bound browser window."
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
        description = "Return safe browser window content hints and identity metadata for a revalidated bound browser window."
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
        description = "Capture a screenshot checkpoint for a revalidated bound browser window."
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
        description = "Read Unicode clipboard text with optional truncation."
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
        description = "Write Unicode clipboard text only when clipboard mutation is explicitly enabled."
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
        description = "Read a UTF-8 file from an allowlisted filesystem root with bounded size."
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
        description = "List files and directories beneath an allowlisted filesystem root."
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
        description = "Search file names and bounded UTF-8 file content beneath an allowlisted filesystem root."
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
        description = "Copy a file within allowlisted roots only when filesystem mutation is explicitly enabled."
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
        description = "Move a file within allowlisted roots only when filesystem mutation is explicitly enabled."
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
        description = "Delete an allowlisted filesystem path only when filesystem mutation is explicitly enabled."
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
        description = "Export a captured artifact to the capture export directory or an allowlisted destination."
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
        description = "List Windows registry subkeys and optional values from a selected hive."
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
        description = "Read a Windows registry value from a selected hive."
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
        description = "Write a Windows registry value only when registry mutation is explicitly enabled."
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
        description = "Delete a Windows registry value only when registry mutation is explicitly enabled."
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
        description = "Return Windows notification inspection status and any available provider-backed notifications."
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
        description = "Return additional process diagnostics with optional windows and child-process metadata."
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
        description = "Fetch an HTTP/HTTPS URL with timeout, response-size, redirect, and private-network guards."
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
        description = "Fetch and extract basic title, link, and text content from an HTTP/HTTPS page under network policy."
    )]
    pub async fn network_scrape(
        &self,
        request: Parameters<NetworkScrapeRequest>,
    ) -> Json<serde_json::Value> {
        let request = request.0;
        Json(tools::network::network_scrape(request, self.state.policy.allow_private_network).await)
    }

    #[tool(
        name = "recorder.start",
        description = "Start a local recording session that can be exported as a macro manifest."
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
        description = "Append one recorded MCP tool step with optional target, note, and replay metadata."
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
        description = "Stop the active recording session and return its macro manifest."
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
        name = "recorder.export_manifest",
        description = "Export the active or completed recording session as a macro manifest."
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
        description = "Return active and completed local recording sessions."
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
        description = "Validate a winctl test manifest and its aligned macro manifest."
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
        description = "Build a dry-run plan for a winctl test manifest without mutating UI state."
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
        description = "Run a winctl test manifest through the macro execution engine."
    )]
    pub async fn test_run(&self, request: Parameters<TestRunRequest>) -> Json<serde_json::Value> {
        let state = self.state.clone();
        let request = request.0;
        run_blocking_tool("test.run", move || {
            tools::control::with_control_gate(
                &state,
                "test.run",
                None,
                "test_replay",
                true,
                || tools::tests::test_run(&state, request),
            )
        })
        .await
    }

    #[tool(
        name = "test.export_result",
        description = "Export a test run result by run ID."
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
        description = "Move the mouse to a bound window coordinate after identity revalidation and point preflight."
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
        description = "Double-click a bound window coordinate after identity revalidation and point preflight."
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
        description = "Drag between two bound window coordinates after identity revalidation and point preflight."
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
        description = "Scroll at a bound window coordinate after identity revalidation and point preflight."
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
        description = "Focus a bound window and dispatch a virtual key-down event after identity revalidation."
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
        description = "Focus a bound window and dispatch a virtual key-up event after identity revalidation."
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
        description = "Focus a bound window and dispatch a virtual-key shortcut after identity revalidation."
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
        .route("/dashboard/state", get(dashboard_state_json))
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

async fn dashboard_asset(AxumPath(path): AxumPath<String>) -> Response {
    match path.as_str() {
        "dashboard.css" => {
            ([(CONTENT_TYPE, "text/css; charset=utf-8")], DASHBOARD_CSS).into_response()
        }
        "dashboard.js" => (
            [(CONTENT_TYPE, "text/javascript; charset=utf-8")],
            DASHBOARD_JS,
        )
            .into_response(),
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
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
        "control": control,
        "connected_clients": serde_json::Value::Null,
        "recent_requests": [],
        "warnings": [
            "connected client and request-history tracking are not enabled yet"
        ]
    }))
}

async fn recorder_state_json(State(state): State<DashboardState>) -> impl IntoResponse {
    AxumJson(serde_json::json!({
        "ok": true,
        "windows": winctl::list_windows(),
        "recording": tools::recorder::recorder_state(&state.app_state),
    }))
}

const DASHBOARD_HTML: &str = include_str!("../dashboard/dist/index.html");
const DASHBOARD_CSS: &str = include_str!("../dashboard/dist/assets/dashboard.css");
const DASHBOARD_JS: &str = include_str!("../dashboard/dist/assets/dashboard.js");

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
                "browser.assert",
                "browser.describe",
                "browser.extract_content",
                "browser.list",
                "browser.screenshot_checkpoint",
                "browser.wait_for_navigation",
                "capture.screenshot_display",
                "capture.screenshot_window",
                "capture.wait_for_window_image_change",
                "clipboard.read",
                "clipboard.write",
                "control.arm",
                "control.consent",
                "control.emergency_stop",
                "control.notify",
                "control.revoke",
                "control.state",
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
                "process.wait_for_exit",
                "recorder.export_manifest",
                "recorder.record_step",
                "recorder.start",
                "recorder.state",
                "recorder.stop",
                "registry.delete",
                "registry.list",
                "registry.read",
                "registry.write",
                "server.config",
                "server.ping",
                "test.dry_run",
                "test.export_result",
                "test.run",
                "test.validate",
                "uia.find",
                "uia.resolve",
                "uia.snapshot",
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
}
