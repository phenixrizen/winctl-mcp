use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;
use winctl_macro::{
    tool_descriptor, validate_manifest, AppIdentity, BindingStrategy, CoordinateFallbackMetadata,
    MacroManifest, MacroStep, MacroTarget, Rect, ReplayMetadata, Size, StepAudit, ToolCall,
    UiElementTarget, MACRO_MANIFEST_VERSION,
};
use winctl_memory::RememberRequest;

use crate::{
    AppState, RecorderExportRequest, RecorderPauseRequest, RecorderRecordStepRequest,
    RecorderStartRequest, RecorderStopRequest,
};

const RECORDER_TEXT_MERGE_MS: u64 = 750;
const RECORDER_DOUBLE_CLICK_MS: u64 = 500;
const RECORDER_DRAG_THRESHOLD_PX: f64 = 6.0;
static NATIVE_CAPTURE_STARTED: AtomicBool = AtomicBool::new(false);
static RECORDER_HOTKEY_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecorderRuntimeState {
    next_session_id: u64,
    active: Option<RecordingSession>,
    completed: HashMap<String, RecordingSession>,
    #[serde(default)]
    status: RecorderStatus,
    #[serde(default)]
    native_capture: RecorderNativeCaptureState,
    #[serde(skip)]
    coalescer: Option<RecordingCoalescer>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecorderStatus {
    Idle,
    Recording,
    Paused,
}

impl Default for RecorderStatus {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderNativeCaptureState {
    pub provider_enabled: bool,
    pub provider: String,
    pub hooks_started: bool,
    pub hotkeys_started: bool,
    pub capture_enabled: bool,
    pub ignored_process_names: Vec<String>,
    pub captured_event_count: u64,
    pub emitted_step_count: u64,
    pub ignored_event_count: u64,
    pub last_event_unix_ms: Option<u64>,
    pub last_error: Option<String>,
    pub notes: Vec<String>,
}

impl Default for RecorderNativeCaptureState {
    fn default() -> Self {
        Self {
            provider_enabled: cfg!(windows),
            provider: if cfg!(windows) {
                "windows_low_level_hooks".into()
            } else {
                "unsupported_platform".into()
            },
            hooks_started: false,
            hotkeys_started: false,
            capture_enabled: false,
            ignored_process_names: vec![
                "winctl-mcp-server.exe".into(),
                "winctl-tray.exe".into(),
            ],
            captured_event_count: 0,
            emitted_step_count: 0,
            ignored_event_count: 0,
            last_event_unix_ms: None,
            last_error: None,
            notes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub app_identity: Option<AppIdentity>,
    pub status: RecorderStatus,
    pub capture_input: bool,
    pub created_at: String,
    pub updated_at: String,
    pub stopped_at: Option<String>,
    pub steps: Vec<MacroStep>,
    pub notes: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecorderMouseButton {
    Left,
    Right,
    Middle,
}

#[allow(dead_code)]
impl RecorderMouseButton {
    fn as_tool_button(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecorderWindowIdentity {
    pub hwnd_hex: Option<String>,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub exe_path: Option<String>,
    pub title: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecorderSemanticTarget {
    pub automation_id: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub control_type: Option<String>,
    pub class_name: Option<String>,
    pub element_ref: Option<String>,
    pub window: Option<RecorderWindowIdentity>,
    pub screenshot_path: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecorderPointer {
    pub x: i32,
    pub y: i32,
    pub timestamp_ms: u64,
    pub target: Option<RecorderSemanticTarget>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecorderInputEvent {
    MouseDown {
        pointer: RecorderPointer,
        button: RecorderMouseButton,
    },
    MouseMove {
        pointer: RecorderPointer,
    },
    MouseUp {
        pointer: RecorderPointer,
        button: RecorderMouseButton,
    },
    Wheel {
        pointer: RecorderPointer,
        delta_x: i32,
        delta_y: i32,
    },
    Text {
        text: String,
        timestamp_ms: u64,
        target: Option<RecorderSemanticTarget>,
        #[serde(default)]
        is_password: bool,
    },
    Shortcut {
        keys: Vec<String>,
        timestamp_ms: u64,
        target: Option<RecorderSemanticTarget>,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedTextPolicy {
    Redact,
    IncludePlaintextForTests,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RecordingCoalescer {
    text_policy: RecordedTextPolicy,
    next_step_index: usize,
    pending_down: Option<PendingMouseDown>,
    pending_click: Option<PendingClick>,
    pending_text: Option<PendingText>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct PendingMouseDown {
    pointer: RecorderPointer,
    button: RecorderMouseButton,
    moved: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct PendingClick {
    pointer: RecorderPointer,
    button: RecorderMouseButton,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct PendingText {
    text: String,
    timestamp_ms: u64,
    target: Option<RecorderSemanticTarget>,
}

impl RecorderRuntimeState {
    fn next_id(&mut self) -> String {
        self.next_session_id += 1;
        format!("recording-{}", self.next_session_id)
    }

    fn start_session(&mut self, request: RecorderStartRequest) -> RecordingSession {
        let id = self.next_id();
        let now = Utc::now().to_rfc3339();
        let capture_input = request.capture_input.unwrap_or(true);
        let session = RecordingSession {
            id,
            title: request.title,
            description: request.description.unwrap_or_default(),
            tags: request.tags,
            app_identity: request.app_identity,
            status: if capture_input {
                RecorderStatus::Recording
            } else {
                RecorderStatus::Paused
            },
            capture_input,
            created_at: now.clone(),
            updated_at: now,
            stopped_at: None,
            steps: Vec::new(),
            notes: Vec::new(),
        };
        self.status = session.status;
        self.native_capture.capture_enabled = capture_input;
        self.native_capture.last_error = None;
        self.coalescer = Some(RecordingCoalescer::default());
        self.active = Some(session.clone());
        session
    }

    fn pause_session(
        &mut self,
        paused: bool,
        reason: Option<String>,
    ) -> Result<RecordingSession, String> {
        let Some(session) = self.active.as_mut() else {
            return Err("no active recording session".into());
        };
        session.status = if paused {
            RecorderStatus::Paused
        } else {
            RecorderStatus::Recording
        };
        session.updated_at = Utc::now().to_rfc3339();
        if let Some(reason) = reason {
            if !reason.trim().is_empty() {
                session.notes.push(reason);
            }
        }
        self.status = session.status;
        self.native_capture.capture_enabled = !paused && session.capture_input;
        Ok(session.clone())
    }

    fn stop_active_session(&mut self) -> Option<RecordingSession> {
        let mut session = self.active.take()?;
        let flushed = self
            .coalescer
            .as_mut()
            .map(RecordingCoalescer::finish)
            .unwrap_or_default();
        let emitted_count = flushed.len();
        if emitted_count > 0 {
            session.steps.extend(flushed);
            self.native_capture.emitted_step_count = self
                .native_capture
                .emitted_step_count
                .saturating_add(emitted_count as u64);
        }
        let now = Utc::now().to_rfc3339();
        session.updated_at = now.clone();
        session.stopped_at = Some(now);
        session.status = RecorderStatus::Idle;
        self.status = RecorderStatus::Idle;
        self.native_capture.capture_enabled = false;
        self.coalescer = None;
        self.completed.insert(session.id.clone(), session.clone());
        Some(session)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn ingest_event(&mut self, event: RecorderInputEvent) -> usize {
        if !matches!(self.status, RecorderStatus::Recording)
            || self
                .active
                .as_ref()
                .map(|session| !session.capture_input)
                .unwrap_or(true)
        {
            self.native_capture.ignored_event_count =
                self.native_capture.ignored_event_count.saturating_add(1);
            return 0;
        }
        self.native_capture.captured_event_count =
            self.native_capture.captured_event_count.saturating_add(1);
        self.native_capture.last_event_unix_ms = Some(now_unix_ms());
        let steps = self
            .coalescer
            .get_or_insert_with(RecordingCoalescer::default)
            .push_event(event);
        if steps.is_empty() {
            return 0;
        }
        let count = steps.len();
        if let Some(session) = self.active.as_mut() {
            session.updated_at = Utc::now().to_rfc3339();
            session.steps.extend(steps);
            self.native_capture.emitted_step_count = self
                .native_capture
                .emitted_step_count
                .saturating_add(count as u64);
            count
        } else {
            0
        }
    }
}

impl Default for RecordingCoalescer {
    fn default() -> Self {
        Self::new(RecordedTextPolicy::Redact)
    }
}

#[allow(dead_code)]
impl RecordingCoalescer {
    pub fn new(text_policy: RecordedTextPolicy) -> Self {
        Self {
            text_policy,
            next_step_index: 1,
            pending_down: None,
            pending_click: None,
            pending_text: None,
        }
    }

    pub fn push_event(&mut self, event: RecorderInputEvent) -> Vec<MacroStep> {
        match event {
            RecorderInputEvent::MouseDown { pointer, button } => {
                let mut steps = self.flush_text();
                steps.extend(self.flush_stale_click(pointer.timestamp_ms));
                self.pending_down = Some(PendingMouseDown {
                    pointer,
                    button,
                    moved: false,
                });
                steps
            }
            RecorderInputEvent::MouseMove { pointer } => {
                if let Some(down) = self.pending_down.as_mut() {
                    if pointer_distance(&down.pointer, &pointer) >= RECORDER_DRAG_THRESHOLD_PX {
                        down.moved = true;
                    }
                }
                self.flush_stale_click(pointer.timestamp_ms)
            }
            RecorderInputEvent::MouseUp { pointer, button } => {
                let mut steps = self.flush_text();
                let Some(down) = self.pending_down.take() else {
                    steps.extend(self.flush_stale_click(pointer.timestamp_ms));
                    return steps;
                };
                if down.button != button {
                    steps.extend(self.flush_stale_click(pointer.timestamp_ms));
                    return steps;
                }
                let distance = pointer_distance(&down.pointer, &pointer);
                if down.moved || distance >= RECORDER_DRAG_THRESHOLD_PX {
                    steps.extend(self.flush_click());
                    steps.push(self.drag_step(down.pointer, pointer, button));
                    return steps;
                }
                if let Some(click) = self.pending_click.take() {
                    if click.button == button
                        && pointer
                            .timestamp_ms
                            .saturating_sub(click.pointer.timestamp_ms)
                            <= RECORDER_DOUBLE_CLICK_MS
                        && pointer_distance(&click.pointer, &pointer) <= RECORDER_DRAG_THRESHOLD_PX
                    {
                        steps.push(self.double_click_step(pointer, button));
                        return steps;
                    }
                    steps.push(self.click_step(click.pointer, click.button));
                }
                self.pending_click = Some(PendingClick { pointer, button });
                steps
            }
            RecorderInputEvent::Wheel {
                pointer,
                delta_x,
                delta_y,
            } => {
                let mut steps = self.flush_text();
                steps.extend(self.flush_click());
                steps.push(self.scroll_step(pointer, delta_x, delta_y));
                steps
            }
            RecorderInputEvent::Text {
                text,
                timestamp_ms,
                target,
                is_password,
            } => {
                let mut steps = self.flush_click();
                if is_password {
                    steps.extend(self.flush_text());
                    let _zeroized = Zeroizing::new(text);
                    steps.push(self.type_secret_step(target));
                    return steps;
                }
                if let Some(pending) = self.pending_text.as_mut() {
                    if timestamp_ms.saturating_sub(pending.timestamp_ms) <= RECORDER_TEXT_MERGE_MS
                        && pending.target == target
                    {
                        pending.text.push_str(&text);
                        pending.timestamp_ms = timestamp_ms;
                        return steps;
                    }
                }
                steps.extend(self.flush_text());
                self.pending_text = Some(PendingText {
                    text,
                    timestamp_ms,
                    target,
                });
                steps
            }
            RecorderInputEvent::Shortcut { keys, target, .. } => {
                let mut steps = self.flush_text();
                steps.extend(self.flush_click());
                steps.push(self.shortcut_step(keys, target));
                steps
            }
        }
    }

    pub fn finish(&mut self) -> Vec<MacroStep> {
        let mut steps = self.flush_text();
        steps.extend(self.flush_click());
        self.pending_down = None;
        steps
    }

    fn click_step(&mut self, pointer: RecorderPointer, button: RecorderMouseButton) -> MacroStep {
        self.pointer_step(
            "click",
            "input.click",
            serde_json::json!({
                "x": pointer.x,
                "y": pointer.y,
                "coordinate_space": "virtual_desktop",
                "button": button.as_tool_button()
            }),
            pointer,
        )
    }

    fn double_click_step(
        &mut self,
        pointer: RecorderPointer,
        button: RecorderMouseButton,
    ) -> MacroStep {
        self.pointer_step(
            "double-click",
            "input.double_click",
            serde_json::json!({
                "x": pointer.x,
                "y": pointer.y,
                "coordinate_space": "virtual_desktop",
                "button": button.as_tool_button()
            }),
            pointer,
        )
    }

    fn drag_step(
        &mut self,
        start: RecorderPointer,
        end: RecorderPointer,
        button: RecorderMouseButton,
    ) -> MacroStep {
        let target = macro_target_from_semantic(start.target.as_ref());
        let fallback = coordinate_fallback(&start, start.target.as_ref());
        self.next_step(
            "drag",
            "input.drag",
            serde_json::json!({
                "start_x": start.x,
                "start_y": start.y,
                "end_x": end.x,
                "end_y": end.y,
                "coordinate_space": "virtual_desktop",
                "button": button.as_tool_button()
            }),
            target,
            fallback,
            StepAudit::default(),
        )
    }

    fn scroll_step(&mut self, pointer: RecorderPointer, delta_x: i32, delta_y: i32) -> MacroStep {
        self.pointer_step(
            "scroll",
            "input.scroll",
            serde_json::json!({
                "x": pointer.x,
                "y": pointer.y,
                "coordinate_space": "virtual_desktop",
                "delta_x": delta_x,
                "delta_y": delta_y
            }),
            pointer,
        )
    }

    fn shortcut_step(
        &mut self,
        keys: Vec<String>,
        target: Option<RecorderSemanticTarget>,
    ) -> MacroStep {
        self.next_step(
            "shortcut",
            "input.shortcut",
            serde_json::json!({ "keys": keys }),
            macro_target_from_semantic(target.as_ref()),
            None,
            StepAudit::default(),
        )
    }

    fn type_text_step(&mut self, pending: PendingText) -> MacroStep {
        let (text, redacted) = match self.text_policy {
            RecordedTextPolicy::IncludePlaintextForTests => (pending.text, false),
            RecordedTextPolicy::Redact => {
                let _zeroized = Zeroizing::new(pending.text);
                (String::new(), true)
            }
        };
        self.next_step(
            "type-text",
            "input.type_text",
            serde_json::json!({ "text": text }),
            macro_target_from_semantic(pending.target.as_ref()),
            None,
            StepAudit {
                notes: if redacted {
                    vec![
                        "recorded text was redacted; review must supply safe text before replay"
                            .into(),
                    ]
                } else {
                    Vec::new()
                },
                ..Default::default()
            },
        )
    }

    fn type_secret_step(&mut self, target: Option<RecorderSemanticTarget>) -> MacroStep {
        self.next_step(
            "type-secret",
            "macro.type_secret",
            serde_json::json!({ "secret_ref": "" }),
            macro_target_from_semantic(target.as_ref()),
            None,
            StepAudit {
                notes: vec![
                    "password input was buffered in memory only; bind secret_ref during review"
                        .into(),
                ],
                ..Default::default()
            },
        )
    }

    fn pointer_step(
        &mut self,
        id_prefix: &str,
        tool: &str,
        args: Value,
        pointer: RecorderPointer,
    ) -> MacroStep {
        let target = macro_target_from_semantic(pointer.target.as_ref());
        let fallback = coordinate_fallback(&pointer, pointer.target.as_ref());
        self.next_step(
            id_prefix,
            tool,
            args,
            target,
            fallback,
            StepAudit::default(),
        )
    }

    fn next_step(
        &mut self,
        id_prefix: &str,
        tool: &str,
        args: Value,
        target: Option<MacroTarget>,
        coordinate_fallback: Option<CoordinateFallbackMetadata>,
        audit: StepAudit,
    ) -> MacroStep {
        let id = format!("{id_prefix}-{}", self.next_step_index);
        self.next_step_index += 1;
        MacroStep {
            id,
            tool: tool.into(),
            args,
            target: target.or(Some(MacroTarget::Current)),
            timeout_ms: None,
            required: true,
            continue_on_failure: false,
            coordinate_fallback,
            audit,
        }
    }

    fn flush_text(&mut self) -> Vec<MacroStep> {
        self.pending_text
            .take()
            .map(|pending| vec![self.type_text_step(pending)])
            .unwrap_or_default()
    }

    fn flush_click(&mut self) -> Vec<MacroStep> {
        self.pending_click
            .take()
            .map(|click| vec![self.click_step(click.pointer, click.button)])
            .unwrap_or_default()
    }

    fn flush_stale_click(&mut self, timestamp_ms: u64) -> Vec<MacroStep> {
        let stale = self
            .pending_click
            .as_ref()
            .map(|click| {
                timestamp_ms.saturating_sub(click.pointer.timestamp_ms) > RECORDER_DOUBLE_CLICK_MS
            })
            .unwrap_or(false);
        if stale {
            self.flush_click()
        } else {
            Vec::new()
        }
    }
}

pub fn recorder_start(state: &AppState, request: RecorderStartRequest) -> serde_json::Value {
    tracing::info!(title = %request.title, "recorder.start requested");
    start_native_recorder_capture(state.recorder_runtime.clone());
    start_recorder_hotkeys(state.recorder_runtime.clone(), state.control_runtime.clone());
    let session = {
        let mut runtime = state
            .recorder_runtime
            .lock()
            .expect("recorder mutex poisoned");
        runtime.start_session(request)
    };
    record_recorder_audit(
        state,
        "recorder_started",
        Some(session.id.clone()),
        format!(
            "recording session started; native input capture enabled={}",
            session.capture_input
        ),
    );
    serde_json::json!({"ok": true, "session": session})
}

pub fn recorder_pause(state: &AppState, request: RecorderPauseRequest) -> serde_json::Value {
    tracing::info!(
        paused = request.paused,
        reason = ?request.reason,
        "recorder.pause requested"
    );
    let session = {
        let mut runtime = state
            .recorder_runtime
            .lock()
            .expect("recorder mutex poisoned");
        match runtime.pause_session(request.paused, request.reason.clone()) {
            Ok(session) => session,
            Err(message) => {
                return recorder_error("recording_not_active", &message);
            }
        }
    };
    record_recorder_audit(
        state,
        if request.paused {
            "recorder_paused"
        } else {
            "recorder_resumed"
        },
        Some(session.id.clone()),
        request.reason.unwrap_or_else(|| {
            if session.status == RecorderStatus::Paused {
                "recording session paused".into()
            } else {
                "recording session resumed".into()
            }
        }),
    );
    serde_json::json!({"ok": true, "session": session})
}

pub fn recorder_record_step(
    state: &AppState,
    request: RecorderRecordStepRequest,
) -> serde_json::Value {
    tracing::info!(tool = %request.tool, "recorder.record_step requested");
    let mut runtime = state
        .recorder_runtime
        .lock()
        .expect("recorder mutex poisoned");
    let Some(session) = runtime.active.as_mut() else {
        return recorder_error("recording_not_active", "no active recording session");
    };
    let step_index = session.steps.len() + 1;
    let args = request.args.unwrap_or(Value::Null);
    let target = request
        .target
        .or_else(|| default_recorded_target(&request.tool, &args));
    let step = MacroStep {
        id: request.id.unwrap_or_else(|| format!("step-{step_index}")),
        tool: request.tool,
        args,
        target,
        timeout_ms: request.timeout_ms,
        required: request.required,
        continue_on_failure: request.continue_on_failure,
        coordinate_fallback: request.coordinate_fallback,
        audit: request.audit.unwrap_or_default(),
    };
    session.updated_at = Utc::now().to_rfc3339();
    if let Some(note) = request.note {
        session.notes.push(note);
    }
    session.steps.push(step.clone());
    serde_json::json!({"ok": true, "session_id": session.id, "step": step, "step_count": session.steps.len()})
}

pub fn recorder_stop(state: &AppState, request: RecorderStopRequest) -> serde_json::Value {
    tracing::info!(
        save_to_memory = request.save_to_memory,
        "recorder.stop requested"
    );
    let session = {
        let mut runtime = state
            .recorder_runtime
            .lock()
            .expect("recorder mutex poisoned");
        let Some(session) = runtime.stop_active_session() else {
            return recorder_error("recording_not_active", "no active recording session");
        };
        session
    };
    record_recorder_audit(
        state,
        "recorder_stopped",
        Some(session.id.clone()),
        format!("recording session stopped; steps={}", session.steps.len()),
    );
    let manifest = manifest_from_session(&session);
    let report = validate_manifest(&manifest);
    let memory_id = if request.save_to_memory {
        if !state.policy.memory_mutation_enabled {
            None
        } else {
            let mut memory = state.memory.lock().expect("memory store mutex poisoned");
            memory
                .remember(RememberRequest {
                    kind: "macro".into(),
                    title: manifest.title.clone(),
                    text: format!("{}\n{}", manifest.title, manifest.description),
                    manifest_json: Some(serde_json::to_value(&manifest).unwrap_or(Value::Null)),
                    tags: manifest.tags.clone(),
                    app_identity_json: manifest
                        .app_identity
                        .as_ref()
                        .and_then(|identity| serde_json::to_value(identity).ok()),
                    target_identity_json: None,
                })
                .ok()
                .map(|item| item.id)
        }
    } else {
        None
    };
    serde_json::json!({
        "ok": true,
        "session": session,
        "manifest": manifest,
        "validation": report,
        "memory_id": memory_id,
        "warnings": if request.save_to_memory && memory_id.is_none() {
            vec!["recording was not saved to memory; memory mutation may be disabled"]
        } else {
            Vec::<&str>::new()
        }
    })
}

pub fn recorder_export(state: &AppState, request: RecorderExportRequest) -> serde_json::Value {
    tracing::info!(session_id = ?request.session_id, "recorder.export_manifest requested");
    let runtime = state
        .recorder_runtime
        .lock()
        .expect("recorder mutex poisoned");
    let session = if let Some(session_id) = request.session_id {
        runtime.completed.get(&session_id).or_else(|| {
            runtime
                .active
                .as_ref()
                .filter(|session| session.id == session_id)
        })
    } else {
        runtime.active.as_ref()
    };
    let Some(session) = session else {
        return recorder_error("recording_not_found", "recording session was not found");
    };
    let manifest = manifest_from_session(session);
    serde_json::json!({
        "ok": true,
        "session": session,
        "manifest": manifest,
        "validation": validate_manifest(&manifest),
    })
}

pub fn recorder_state(state: &AppState) -> serde_json::Value {
    let runtime = state
        .recorder_runtime
        .lock()
        .expect("recorder mutex poisoned");
    serde_json::json!({
        "ok": true,
        "status": runtime.status,
        "native_capture": runtime.native_capture,
        "active": runtime.active,
        "completed": runtime.completed.values().collect::<Vec<_>>(),
    })
}

fn record_recorder_audit(
    state: &AppState,
    kind: &str,
    session_id: Option<String>,
    message: String,
) {
    let _ = crate::tools::control::record_passive_audit_event(
        &state.control_runtime,
        kind,
        Some("recorder".into()),
        Some("passive_recording".into()),
        None,
        session_id,
        message,
    );
}

fn start_native_recorder_capture(runtime: Arc<Mutex<RecorderRuntimeState>>) {
    if NATIVE_CAPTURE_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        mark_hooks_started(&runtime);
        return;
    }

    #[cfg(windows)]
    {
        windows_recorder::start_capture(runtime);
    }

    #[cfg(not(windows))]
    {
        let mut runtime = runtime.lock().expect("recorder mutex poisoned");
        runtime.native_capture.provider_enabled = false;
        runtime.native_capture.hooks_started = false;
        runtime.native_capture.last_error = Some(
            "native recorder hooks require Windows runtime; explicit recorder.record_step still works"
                .into(),
        );
        NATIVE_CAPTURE_STARTED.store(false, Ordering::SeqCst);
    }
}

fn start_recorder_hotkeys(
    runtime: Arc<Mutex<RecorderRuntimeState>>,
    control_runtime: Arc<Mutex<crate::tools::control::ControlRuntimeState>>,
) {
    if RECORDER_HOTKEY_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        mark_hotkeys_started(&runtime);
        return;
    }

    #[cfg(windows)]
    {
        windows_recorder::start_hotkeys(runtime, control_runtime);
    }

    #[cfg(not(windows))]
    {
        let _ = control_runtime;
        let mut runtime = runtime.lock().expect("recorder mutex poisoned");
        runtime.native_capture.hotkeys_started = false;
        runtime.native_capture.notes.push(
            "recorder session hotkeys require Windows runtime; use recorder.start/pause/stop tools"
                .into(),
        );
        RECORDER_HOTKEY_STARTED.store(false, Ordering::SeqCst);
    }
}

fn mark_hooks_started(runtime: &Arc<Mutex<RecorderRuntimeState>>) {
    if let Ok(mut runtime) = runtime.lock() {
        runtime.native_capture.hooks_started = cfg!(windows);
    }
}

fn mark_hotkeys_started(runtime: &Arc<Mutex<RecorderRuntimeState>>) {
    if let Ok(mut runtime) = runtime.lock() {
        runtime.native_capture.hotkeys_started = cfg!(windows);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn record_native_error(runtime: &Arc<Mutex<RecorderRuntimeState>>, error: impl Into<String>) {
    let error = error.into();
    tracing::warn!(error = %error, "native recorder capture error");
    if let Ok(mut runtime) = runtime.lock() {
        runtime.native_capture.last_error = Some(error);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn ingest_native_event(runtime: &Arc<Mutex<RecorderRuntimeState>>, event: RecorderInputEvent) {
    let emitted = {
        let mut runtime = runtime.lock().expect("recorder mutex poisoned");
        runtime.ingest_event(event)
    };
    if emitted > 0 {
        tracing::info!(emitted_steps = emitted, "native recorder emitted macro steps");
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn start_hotkey_recording(runtime: &Arc<Mutex<RecorderRuntimeState>>) -> Option<RecordingSession> {
    let mut runtime = runtime.lock().expect("recorder mutex poisoned");
    if runtime.active.is_some() {
        return None;
    }
    Some(runtime.start_session(RecorderStartRequest {
        title: "Hotkey recording".into(),
        description: Some("Started with Ctrl+Alt+F9".into()),
        tags: vec!["hotkey".into(), "recorded".into()],
        app_identity: None,
        capture_input: Some(true),
    }))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn stop_hotkey_recording(runtime: &Arc<Mutex<RecorderRuntimeState>>) -> Option<RecordingSession> {
    runtime
        .lock()
        .expect("recorder mutex poisoned")
        .stop_active_session()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn toggle_hotkey_pause(runtime: &Arc<Mutex<RecorderRuntimeState>>) -> Option<RecordingSession> {
    let mut runtime = runtime.lock().expect("recorder mutex poisoned");
    let paused = !matches!(runtime.status, RecorderStatus::Paused);
    runtime
        .pause_session(paused, Some("toggled by Ctrl+Alt+F10".into()))
        .ok()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn audit_hotkey_event(
    control_runtime: &Arc<Mutex<crate::tools::control::ControlRuntimeState>>,
    kind: &str,
    session_id: Option<String>,
    message: impl Into<String>,
) {
    let _ = crate::tools::control::record_passive_audit_event(
        control_runtime,
        kind,
        Some("recorder".into()),
        Some("passive_recording_hotkey".into()),
        None,
        session_id,
        message.into(),
    );
}

#[cfg_attr(not(windows), allow(dead_code))]
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(windows)]
mod windows_recorder {
    use std::sync::{mpsc, Mutex, OnceLock};

    use windows::core::IUnknown;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardState, GetKeyState, RegisterHotKey, ToUnicode, UnregisterHotKey, MOD_ALT,
        MOD_CONTROL, MOD_NOREPEAT, VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, MSG, WH_KEYBOARD_LL,
        WH_MOUSE_LL, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
        WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
    };

    use super::*;

    const HOTKEY_TOGGLE_RECORDING: i32 = 0x5752;
    const HOTKEY_PAUSE_RECORDING: i32 = 0x5753;
    const VK_F9_CODE: u32 = 0x78;
    const VK_F10_CODE: u32 = 0x79;

    static HOOK_SENDER: OnceLock<Mutex<Option<mpsc::SyncSender<NativeRecorderEvent>>>> =
        OnceLock::new();

    #[derive(Debug, Clone)]
    enum NativeRecorderEvent {
        Mouse {
            kind: NativeMouseKind,
            x: i32,
            y: i32,
            mouse_data: u32,
            timestamp_ms: u64,
        },
        Key {
            vk_code: u32,
            scan_code: u32,
            timestamp_ms: u64,
        },
    }

    #[derive(Debug, Clone, Copy)]
    enum NativeMouseKind {
        Move,
        LeftDown,
        LeftUp,
        RightDown,
        RightUp,
        MiddleDown,
        MiddleUp,
        Wheel,
    }

    pub(super) fn start_capture(runtime: Arc<Mutex<RecorderRuntimeState>>) {
        let (sender, receiver) = mpsc::sync_channel(4096);
        *HOOK_SENDER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("recorder hook sender mutex poisoned") = Some(sender);

        let processor_runtime = runtime.clone();
        let _ = thread::Builder::new()
            .name("winctl-recorder-processor".into())
            .spawn(move || process_hook_events(processor_runtime, receiver))
            .map_err(|error| {
                super::record_native_error(
                    &runtime,
                    format!("failed to start recorder processor thread: {error}"),
                );
            });

        let hook_runtime = runtime.clone();
        match thread::Builder::new()
            .name("winctl-recorder-hooks".into())
            .spawn(move || hook_thread(hook_runtime))
        {
            Ok(_) => {
                let mut runtime = runtime.lock().expect("recorder mutex poisoned");
                runtime.native_capture.hooks_started = true;
                runtime.native_capture.provider_enabled = true;
                runtime.native_capture.notes.push(
                    "native low-level mouse/keyboard hooks started; recorder remains passive"
                        .into(),
                );
            }
            Err(error) => {
                super::NATIVE_CAPTURE_STARTED.store(false, Ordering::SeqCst);
                super::record_native_error(
                    &runtime,
                    format!("failed to start recorder hook thread: {error}"),
                );
            }
        }
    }

    pub(super) fn start_hotkeys(
        runtime: Arc<Mutex<RecorderRuntimeState>>,
        control_runtime: Arc<Mutex<crate::tools::control::ControlRuntimeState>>,
    ) {
        let hotkey_runtime = runtime.clone();
        match thread::Builder::new()
            .name("winctl-recorder-hotkeys".into())
            .spawn(move || hotkey_thread(hotkey_runtime, control_runtime))
        {
            Ok(_) => {
                let mut runtime = runtime.lock().expect("recorder mutex poisoned");
                runtime.native_capture.hotkeys_started = true;
                runtime.native_capture.notes.push(
                    "registered Ctrl+Alt+F9 recording toggle and Ctrl+Alt+F10 pause/resume"
                        .into(),
                );
            }
            Err(error) => {
                super::RECORDER_HOTKEY_STARTED.store(false, Ordering::SeqCst);
                super::record_native_error(
                    &runtime,
                    format!("failed to start recorder hotkey thread: {error}"),
                );
            }
        }
    }

    unsafe extern "system" fn mouse_hook(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            let kind = match message {
                WM_MOUSEMOVE => Some(NativeMouseKind::Move),
                WM_LBUTTONDOWN => Some(NativeMouseKind::LeftDown),
                WM_LBUTTONUP => Some(NativeMouseKind::LeftUp),
                WM_RBUTTONDOWN => Some(NativeMouseKind::RightDown),
                WM_RBUTTONUP => Some(NativeMouseKind::RightUp),
                WM_MBUTTONDOWN => Some(NativeMouseKind::MiddleDown),
                WM_MBUTTONUP => Some(NativeMouseKind::MiddleUp),
                WM_MOUSEWHEEL => Some(NativeMouseKind::Wheel),
                _ => None,
            };
            if let Some(kind) = kind {
                let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
                send_hook_event(NativeRecorderEvent::Mouse {
                    kind,
                    x: info.pt.x,
                    y: info.pt.y,
                    mouse_data: info.mouseData,
                    timestamp_ms: super::now_unix_ms(),
                });
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    unsafe extern "system" fn keyboard_hook(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
                let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                send_hook_event(NativeRecorderEvent::Key {
                    vk_code: info.vkCode,
                    scan_code: info.scanCode,
                    timestamp_ms: super::now_unix_ms(),
                });
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    fn send_hook_event(event: NativeRecorderEvent) {
        let Some(sender) = HOOK_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|guard| guard.clone()))
        else {
            return;
        };
        let _ = sender.try_send(event);
    }

    fn hook_thread(runtime: Arc<Mutex<RecorderRuntimeState>>) {
        let module = unsafe { GetModuleHandleW(None) }
            .map(HINSTANCE::from)
            .ok();
        let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), module, 0) };
        let mouse_hook = match mouse_hook {
            Ok(hook) => HookGuard(hook),
            Err(error) => {
                super::NATIVE_CAPTURE_STARTED.store(false, Ordering::SeqCst);
                super::record_native_error(&runtime, format!("failed to install mouse hook: {error}"));
                return;
            }
        };
        let keyboard_hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0) };
        let keyboard_hook = match keyboard_hook {
            Ok(hook) => HookGuard(hook),
            Err(error) => {
                super::NATIVE_CAPTURE_STARTED.store(false, Ordering::SeqCst);
                super::record_native_error(
                    &runtime,
                    format!("failed to install keyboard hook: {error}"),
                );
                drop(mouse_hook);
                return;
            }
        };

        tracing::info!("native recorder low-level hooks installed");
        let mut message = MSG::default();
        loop {
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 <= 0 {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        drop(keyboard_hook);
        drop(mouse_hook);
        super::NATIVE_CAPTURE_STARTED.store(false, Ordering::SeqCst);
    }

    fn hotkey_thread(
        runtime: Arc<Mutex<RecorderRuntimeState>>,
        control_runtime: Arc<Mutex<crate::tools::control::ControlRuntimeState>>,
    ) {
        let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
        if let Err(error) =
            unsafe { RegisterHotKey(None, HOTKEY_TOGGLE_RECORDING, modifiers, VK_F9_CODE) }
        {
            super::RECORDER_HOTKEY_STARTED.store(false, Ordering::SeqCst);
            super::record_native_error(
                &runtime,
                format!("failed to register Ctrl+Alt+F9 recorder hotkey: {error}"),
            );
            return;
        }
        if let Err(error) =
            unsafe { RegisterHotKey(None, HOTKEY_PAUSE_RECORDING, modifiers, VK_F10_CODE) }
        {
            let _ = unsafe { UnregisterHotKey(None, HOTKEY_TOGGLE_RECORDING) };
            super::RECORDER_HOTKEY_STARTED.store(false, Ordering::SeqCst);
            super::record_native_error(
                &runtime,
                format!("failed to register Ctrl+Alt+F10 recorder hotkey: {error}"),
            );
            return;
        }

        tracing::info!("registered recorder Ctrl+Alt+F9 and Ctrl+Alt+F10 hotkeys");
        let mut message = MSG::default();
        loop {
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 <= 0 {
                break;
            }
            if message.message != WM_HOTKEY {
                continue;
            }
            match message.wParam.0 as i32 {
                HOTKEY_TOGGLE_RECORDING => {
                    if let Some(session) = super::stop_hotkey_recording(&runtime) {
                        super::audit_hotkey_event(
                            &control_runtime,
                            "recorder_stopped",
                            Some(session.id),
                            "recording session stopped by Ctrl+Alt+F9",
                        );
                    } else if let Some(session) = super::start_hotkey_recording(&runtime) {
                        super::audit_hotkey_event(
                            &control_runtime,
                            "recorder_started",
                            Some(session.id),
                            "recording session started by Ctrl+Alt+F9",
                        );
                    }
                }
                HOTKEY_PAUSE_RECORDING => {
                    if let Some(session) = super::toggle_hotkey_pause(&runtime) {
                        let paused = session.status == RecorderStatus::Paused;
                        super::audit_hotkey_event(
                            &control_runtime,
                            if paused {
                                "recorder_paused"
                            } else {
                                "recorder_resumed"
                            },
                            Some(session.id),
                            if paused {
                                "recording session paused by Ctrl+Alt+F10"
                            } else {
                                "recording session resumed by Ctrl+Alt+F10"
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        let _ = unsafe { UnregisterHotKey(None, HOTKEY_TOGGLE_RECORDING) };
        let _ = unsafe { UnregisterHotKey(None, HOTKEY_PAUSE_RECORDING) };
        super::RECORDER_HOTKEY_STARTED.store(false, Ordering::SeqCst);
    }

    fn process_hook_events(
        runtime: Arc<Mutex<RecorderRuntimeState>>,
        receiver: mpsc::Receiver<NativeRecorderEvent>,
    ) {
        while let Ok(event) = receiver.recv() {
            match event {
                NativeRecorderEvent::Mouse {
                    kind,
                    x,
                    y,
                    mouse_data,
                    timestamp_ms,
                } => {
                    if !native_capture_enabled(&runtime) {
                        continue;
                    }
                    if point_belongs_to_own_surface(&runtime, x, y) {
                        mark_ignored(&runtime);
                        continue;
                    }
                    if let Some(event) = mouse_event(kind, x, y, mouse_data, timestamp_ms) {
                        super::ingest_native_event(&runtime, event);
                    }
                }
                NativeRecorderEvent::Key {
                    vk_code,
                    scan_code,
                    timestamp_ms,
                } => {
                    if !native_capture_enabled(&runtime) {
                        continue;
                    }
                    if let Some(event) = keyboard_event(vk_code, scan_code, timestamp_ms) {
                        super::ingest_native_event(&runtime, event);
                    }
                }
            }
        }
    }

    fn mouse_event(
        kind: NativeMouseKind,
        x: i32,
        y: i32,
        mouse_data: u32,
        timestamp_ms: u64,
    ) -> Option<RecorderInputEvent> {
        Some(match kind {
            NativeMouseKind::Move => RecorderInputEvent::MouseMove {
                pointer: pointer_at(x, y, timestamp_ms, false),
            },
            NativeMouseKind::LeftDown => RecorderInputEvent::MouseDown {
                pointer: pointer_at(x, y, timestamp_ms, true),
                button: RecorderMouseButton::Left,
            },
            NativeMouseKind::LeftUp => RecorderInputEvent::MouseUp {
                pointer: pointer_at(x, y, timestamp_ms, true),
                button: RecorderMouseButton::Left,
            },
            NativeMouseKind::RightDown => RecorderInputEvent::MouseDown {
                pointer: pointer_at(x, y, timestamp_ms, true),
                button: RecorderMouseButton::Right,
            },
            NativeMouseKind::RightUp => RecorderInputEvent::MouseUp {
                pointer: pointer_at(x, y, timestamp_ms, true),
                button: RecorderMouseButton::Right,
            },
            NativeMouseKind::MiddleDown => RecorderInputEvent::MouseDown {
                pointer: pointer_at(x, y, timestamp_ms, true),
                button: RecorderMouseButton::Middle,
            },
            NativeMouseKind::MiddleUp => RecorderInputEvent::MouseUp {
                pointer: pointer_at(x, y, timestamp_ms, true),
                button: RecorderMouseButton::Middle,
            },
            NativeMouseKind::Wheel => RecorderInputEvent::Wheel {
                pointer: pointer_at(x, y, timestamp_ms, true),
                delta_x: 0,
                delta_y: wheel_delta(mouse_data),
            },
        })
    }

    fn keyboard_event(
        vk_code: u32,
        scan_code: u32,
        timestamp_ms: u64,
    ) -> Option<RecorderInputEvent> {
        if is_modifier_key(vk_code) {
            return None;
        }
        let target = focused_target();
        if shortcut_modifiers_active() || is_non_text_key(vk_code) {
            return Some(RecorderInputEvent::Shortcut {
                keys: shortcut_keys(vk_code),
                timestamp_ms,
                target,
            });
        }
        let text = key_to_text(vk_code, scan_code)?;
        let is_password = focused_element_is_password();
        Some(RecorderInputEvent::Text {
            text,
            timestamp_ms,
            target,
            is_password,
        })
    }

    fn pointer_at(x: i32, y: i32, timestamp_ms: u64, resolve_target: bool) -> RecorderPointer {
        RecorderPointer {
            x,
            y,
            timestamp_ms,
            target: if resolve_target {
                semantic_target_from_point(x, y)
            } else {
                None
            },
        }
    }

    fn focused_target() -> Option<RecorderSemanticTarget> {
        let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }
        let window = winctl::window_info_from_hwnd(hwnd.0 as isize)?;
        Some(RecorderSemanticTarget {
            automation_id: None,
            name: None,
            role: None,
            control_type: None,
            class_name: None,
            element_ref: None,
            window: Some(window_identity(&window)),
            screenshot_path: None,
        })
    }

    fn semantic_target_from_point(x: i32, y: i32) -> Option<RecorderSemanticTarget> {
        let hit = winctl::window_from_point(x, y, None);
        let window = hit.top_level.as_ref().or(hit.child.as_ref()).cloned();
        let mut target = RecorderSemanticTarget {
            automation_id: None,
            name: None,
            role: None,
            control_type: None,
            class_name: None,
            element_ref: None,
            window: window.as_ref().map(window_identity),
            screenshot_path: None,
        };
        if let Some(uia_target) = uia_target_from_point(x, y, window.as_ref()) {
            target.automation_id = uia_target.automation_id;
            target.name = uia_target.name;
            target.role = uia_target.role;
            target.control_type = uia_target.control_type;
            target.class_name = uia_target.class_name;
            target.element_ref = uia_target.element_ref;
        }
        Some(target)
    }

    fn uia_target_from_point(
        x: i32,
        y: i32,
        window: Option<&winctl::WindowInfo>,
    ) -> Option<RecorderSemanticTarget> {
        let _apartment = ComApartment::initialize().ok()?;
        let automation: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
        }
        .ok()?;
        let element = unsafe { automation.ElementFromPoint(POINT { x, y }) }.ok()?;
        let name = read_bstr(|| unsafe { element.CurrentName() });
        let automation_id = read_bstr(|| unsafe { element.CurrentAutomationId() });
        let class_name = read_bstr(|| unsafe { element.CurrentClassName() });
        let control_type_id = unsafe { element.CurrentControlType() }.ok().map(|value| value.0);
        let role = control_type_id
            .and_then(winctl::control_type_name)
            .map(str::to_owned);
        let control_type = role.clone();
        let element_ref = window.map(|window| {
            format!(
                "uia:point:{}:{}:{}:{}:{}",
                window.hwnd_hex,
                window.pid,
                automation_id.as_deref().unwrap_or(""),
                control_type_id.unwrap_or_default(),
                class_name.as_deref().unwrap_or("")
            )
        });
        Some(RecorderSemanticTarget {
            automation_id,
            name,
            role,
            control_type,
            class_name,
            element_ref,
            window: window.map(window_identity),
            screenshot_path: None,
        })
    }

    fn focused_element_is_password() -> bool {
        let Ok(_apartment) = ComApartment::initialize() else {
            return false;
        };
        let Ok(automation) = (unsafe {
            CoCreateInstance::<_, IUIAutomation>(
                &CUIAutomation,
                None::<&IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
        }) else {
            return false;
        };
        let Ok(element) = (unsafe { automation.GetFocusedElement() }) else {
            return false;
        };
        unsafe { element.CurrentIsPassword() }
            .map(|value| value.as_bool())
            .unwrap_or(false)
    }

    fn window_identity(window: &winctl::WindowInfo) -> RecorderWindowIdentity {
        RecorderWindowIdentity {
            hwnd_hex: Some(window.hwnd_hex.clone()),
            pid: Some(window.pid),
            process_name: window.process_name.clone(),
            exe_path: window.exe_path.clone(),
            title: Some(window.title.clone()),
        }
    }

    fn point_belongs_to_own_surface(
        runtime: &Arc<Mutex<RecorderRuntimeState>>,
        x: i32,
        y: i32,
    ) -> bool {
        let hit = winctl::window_from_point(x, y, None);
        let window = hit.child.as_ref().or(hit.top_level.as_ref());
        let Some(window) = window else {
            return false;
        };
        let own_pid = std::process::id();
        if window.pid == own_pid {
            return true;
        }
        let ignored_names = runtime
            .lock()
            .ok()
            .map(|runtime| runtime.native_capture.ignored_process_names.clone())
            .unwrap_or_default();
        window
            .process_name
            .as_ref()
            .map(|name| ignored_names.iter().any(|ignored| name.eq_ignore_ascii_case(ignored)))
            .unwrap_or(false)
            || window.title.to_ascii_lowercase().contains("winctl-mcp")
            || window
                .class_name
                .to_ascii_lowercase()
                .contains("winctl")
    }

    fn native_capture_enabled(runtime: &Arc<Mutex<RecorderRuntimeState>>) -> bool {
        runtime
            .lock()
            .ok()
            .map(|runtime| {
                matches!(runtime.status, RecorderStatus::Recording)
                    && runtime
                        .active
                        .as_ref()
                        .map(|session| session.capture_input)
                        .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    fn mark_ignored(runtime: &Arc<Mutex<RecorderRuntimeState>>) {
        if let Ok(mut runtime) = runtime.lock() {
            runtime.native_capture.ignored_event_count =
                runtime.native_capture.ignored_event_count.saturating_add(1);
        }
    }

    fn wheel_delta(mouse_data: u32) -> i32 {
        ((mouse_data >> 16) as i16) as i32
    }

    fn shortcut_modifiers_active() -> bool {
        key_down(VK_CONTROL.0 as i32) || key_down(VK_MENU.0 as i32)
    }

    fn shortcut_keys(vk_code: u32) -> Vec<String> {
        let mut keys = Vec::new();
        if key_down(VK_CONTROL.0 as i32) {
            keys.push("Ctrl".into());
        }
        if key_down(VK_MENU.0 as i32) {
            keys.push("Alt".into());
        }
        if key_down(VK_SHIFT.0 as i32) {
            keys.push("Shift".into());
        }
        keys.push(vk_name(vk_code));
        keys
    }

    fn key_down(vk_code: i32) -> bool {
        (unsafe { GetKeyState(vk_code) } as u16 & 0x8000) != 0
    }

    fn is_modifier_key(vk_code: u32) -> bool {
        matches!(vk_code, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C)
    }

    fn is_non_text_key(vk_code: u32) -> bool {
        matches!(
            vk_code,
            0x08 | 0x09 | 0x0D | 0x1B | 0x21..=0x28 | 0x2D..=0x2E | 0x70..=0x87
        )
    }

    fn key_to_text(vk_code: u32, scan_code: u32) -> Option<String> {
        let mut keyboard_state = [0u8; 256];
        if unsafe { GetKeyboardState(&mut keyboard_state) }.is_err() {
            return None;
        }
        let mut buffer = [0u16; 8];
        let count = unsafe {
            ToUnicode(
                vk_code,
                scan_code,
                Some(&keyboard_state),
                &mut buffer,
                0,
            )
        };
        if count <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..count as usize]))
    }

    fn vk_name(vk_code: u32) -> String {
        match vk_code {
            0x08 => "Backspace".into(),
            0x09 => "Tab".into(),
            0x0D => "Enter".into(),
            0x1B => "Esc".into(),
            0x20 => "Space".into(),
            0x21 => "PageUp".into(),
            0x22 => "PageDown".into(),
            0x23 => "End".into(),
            0x24 => "Home".into(),
            0x25 => "Left".into(),
            0x26 => "Up".into(),
            0x27 => "Right".into(),
            0x28 => "Down".into(),
            0x2E => "Delete".into(),
            0x70..=0x87 => format!("F{}", vk_code - 0x6F),
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk_code)
                .map(|value| value.to_string())
                .unwrap_or_else(|| format!("VK_{vk_code:X}")),
            _ => format!("VK_{vk_code:X}"),
        }
    }

    fn read_bstr<F>(operation: F) -> Option<String>
    where
        F: FnOnce() -> windows::core::Result<windows::core::BSTR>,
    {
        operation().ok().and_then(|value| {
            let value = value.to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
    }

    struct ComApartment;

    impl ComApartment {
        fn initialize() -> windows::core::Result<Self> {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
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

    struct HookGuard(HHOOK);

    impl Drop for HookGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }
}

fn manifest_from_session(session: &RecordingSession) -> MacroManifest {
    let inferred = infer_recording_setup(session);
    MacroManifest {
        version: MACRO_MANIFEST_VERSION.into(),
        kind: "test_procedure".into(),
        title: session.title.clone(),
        description: session.description.clone(),
        tags: session.tags.clone(),
        app_identity: session.app_identity.clone().or(inferred.app_identity),
        launch: inferred.launch,
        bind: inferred.bind,
        preconditions: Vec::new(),
        steps: session.steps.clone(),
        waits: Vec::new(),
        assertions: Vec::new(),
        cleanup: Vec::new(),
        artifacts: Default::default(),
        replay: inferred.replay,
    }
}

#[derive(Debug, Clone, Default)]
struct InferredRecordingSetup {
    app_identity: Option<AppIdentity>,
    launch: Option<ToolCall>,
    bind: Option<BindingStrategy>,
    replay: ReplayMetadata,
}

fn infer_recording_setup(session: &RecordingSession) -> InferredRecordingSetup {
    let mut replay = ReplayMetadata {
        created_at: Some(session.created_at.clone()),
        version_note: Some("generated by human recorder; launch/bind inference requires review".into()),
        ..Default::default()
    };
    let Some(window) = first_recorded_window(session) else {
        replay.extra = serde_json::json!({
            "recorder": {
                "launch_inference": {
                    "status": "not_available",
                    "reason": "no recorded step carried window identity metadata",
                    "requires_confirmation": true
                }
            }
        });
        return InferredRecordingSetup {
            replay,
            ..Default::default()
        };
    };

    let process_name = normalized_recorded_string(window.process_name.as_deref());
    let exe_path = normalized_recorded_string(window.exe_path.as_deref());
    let app_identity = Some(AppIdentity {
        executable_name: process_name.clone(),
        executable_path: exe_path.clone(),
        product_name: None,
        required: true,
        extra: serde_json::json!({
            "source": "recorder_inference",
            "first_window": window.clone(),
        }),
    });
    let launch = exe_path.as_ref().map(|exe| ToolCall {
        tool: "process.launch".into(),
        args: serde_json::json!({
            "exe": exe,
            "args": [],
            "wait_for_window": true,
            "timeout_ms": 10_000,
            "allow_child_process_windows": false,
        }),
    });
    let bind = Some(BindingStrategy {
        strategy: if launch.is_some() {
            "recorder_inferred_fresh_launch_main_window".into()
        } else {
            "recorder_inferred_bind_existing_window".into()
        },
        required_executable: process_name.clone(),
        expected_identity_json: serde_json::to_value(&window).ok(),
        allow_child_process_windows: false,
    });
    replay.extra = serde_json::json!({
        "recorder": {
            "launch_inference": {
                "status": "inferred",
                "mode": if launch.is_some() { "fresh_launch" } else { "bind_existing" },
                "requires_confirmation": true,
                "target_alias": "app_main",
                "first_window": window.clone(),
                "review_options": [
                    "fresh_launch",
                    "bind_existing",
                    "explicit_app_launch",
                    "multi_window"
                ]
            }
        }
    });
    InferredRecordingSetup {
        app_identity,
        launch,
        bind,
        replay,
    }
}

fn first_recorded_window(session: &RecordingSession) -> Option<RecorderWindowIdentity> {
    session.steps.iter().find_map(|step| {
        step.coordinate_fallback
            .as_ref()
            .and_then(|fallback| {
                serde_json::from_value::<RecorderSemanticTarget>(
                    fallback.original_resolved_target.clone(),
                )
                .ok()
            })
            .and_then(|target| target.window)
            .filter(recorded_window_has_identity)
    })
}

fn recorded_window_has_identity(window: &RecorderWindowIdentity) -> bool {
    normalized_recorded_string(window.exe_path.as_deref()).is_some()
        || normalized_recorded_string(window.process_name.as_deref()).is_some()
}

fn normalized_recorded_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn default_recorded_target(tool: &str, args: &Value) -> Option<MacroTarget> {
    let descriptor = tool_descriptor(tool)?;
    if descriptor.requires_bound_window
        && recorded_tool_needs_bound(tool, args)
        && !args_has_bound_id(args)
    {
        Some(MacroTarget::Current)
    } else {
        None
    }
}

#[allow(dead_code)]
fn macro_target_from_semantic(target: Option<&RecorderSemanticTarget>) -> Option<MacroTarget> {
    let target = target?;
    if target.element_ref.is_none()
        && target.name.is_none()
        && target.role.is_none()
        && target.control_type.is_none()
        && target.automation_id.is_none()
        && target.class_name.is_none()
    {
        return None;
    }
    Some(MacroTarget::UiElement(UiElementTarget {
        element_ref: target.element_ref.clone(),
        name: target.name.clone(),
        role: target.role.clone(),
        control_type: target.control_type.clone(),
        automation_id: target.automation_id.clone(),
        class_name: target.class_name.clone(),
        hierarchy_path: None,
        visible_text: target.name.clone(),
        required: true,
    }))
}

#[allow(dead_code)]
fn coordinate_fallback(
    pointer: &RecorderPointer,
    target: Option<&RecorderSemanticTarget>,
) -> Option<CoordinateFallbackMetadata> {
    Some(CoordinateFallbackMetadata {
        monitor_id: None,
        virtual_desktop_x: pointer.x,
        virtual_desktop_y: pointer.y,
        window_rect: Rect {
            x: pointer.x,
            y: pointer.y,
            width: 1,
            height: 1,
        },
        client_rect: None,
        dpi: None,
        scale_factor: None,
        screenshot_size: target
            .and_then(|target| target.screenshot_path.as_ref())
            .map(|_| Size {
                width: 0,
                height: 0,
            }),
        original_resolved_target: target
            .and_then(|target| serde_json::to_value(target).ok())
            .unwrap_or(Value::Null),
        preflight_validation: serde_json::json!({
            "source": "human_recorder",
            "coordinate_space": "virtual_desktop"
        }),
    })
}

#[allow(dead_code)]
fn pointer_distance(left: &RecorderPointer, right: &RecorderPointer) -> f64 {
    let dx = f64::from(left.x - right.x);
    let dy = f64::from(left.y - right.y);
    (dx * dx + dy * dy).sqrt()
}

fn recorded_tool_needs_bound(tool: &str, args: &Value) -> bool {
    match tool {
        "capture.ocr_region" => !args_has_string(args, "image_path"),
        "macro.assert_image_checkpoint" => !args_has_string(args, "actual_path"),
        "macro.assert_text_checkpoint" => {
            !args_has_string(args, "actual_text") && !args_has_string(args, "image_path")
        }
        _ => true,
    }
}

fn args_has_bound_id(args: &Value) -> bool {
    args.get("bound_id")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn args_has_string(args: &Value, key: &str) -> bool {
    args.get(key)
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn recorder_error(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalescer_turns_down_up_into_click() {
        let mut coalescer = RecordingCoalescer::default();
        assert!(coalescer
            .push_event(RecorderInputEvent::MouseDown {
                pointer: pointer(100, 200, 10),
                button: RecorderMouseButton::Left,
            })
            .is_empty());
        assert!(coalescer
            .push_event(RecorderInputEvent::MouseUp {
                pointer: pointer(102, 201, 30),
                button: RecorderMouseButton::Left,
            })
            .is_empty());
        let steps = coalescer.finish();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].tool, "input.click");
        assert_eq!(steps[0].args["button"], "left");
        assert_eq!(steps[0].args["coordinate_space"], "virtual_desktop");
    }

    #[test]
    fn coalescer_turns_two_clicks_into_double_click() {
        let mut coalescer = RecordingCoalescer::default();
        coalescer.push_event(RecorderInputEvent::MouseDown {
            pointer: pointer(100, 200, 10),
            button: RecorderMouseButton::Left,
        });
        coalescer.push_event(RecorderInputEvent::MouseUp {
            pointer: pointer(100, 200, 30),
            button: RecorderMouseButton::Left,
        });
        coalescer.push_event(RecorderInputEvent::MouseDown {
            pointer: pointer(101, 200, 120),
            button: RecorderMouseButton::Left,
        });
        let emitted = coalescer.push_event(RecorderInputEvent::MouseUp {
            pointer: pointer(101, 200, 150),
            button: RecorderMouseButton::Left,
        });
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].tool, "input.double_click");
        assert!(coalescer.finish().is_empty());
    }

    #[test]
    fn coalescer_turns_move_between_down_up_into_drag() {
        let mut coalescer = RecordingCoalescer::default();
        coalescer.push_event(RecorderInputEvent::MouseDown {
            pointer: pointer(10, 10, 10),
            button: RecorderMouseButton::Left,
        });
        coalescer.push_event(RecorderInputEvent::MouseMove {
            pointer: pointer(50, 50, 20),
        });
        let emitted = coalescer.push_event(RecorderInputEvent::MouseUp {
            pointer: pointer(80, 90, 40),
            button: RecorderMouseButton::Left,
        });
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].tool, "input.drag");
        assert_eq!(emitted[0].args["start_x"], 10);
        assert_eq!(emitted[0].args["end_y"], 90);
    }

    #[test]
    fn coalescer_merges_character_runs_without_leaking_by_default() {
        let mut coalescer = RecordingCoalescer::default();
        coalescer.push_event(RecorderInputEvent::Text {
            text: "he".into(),
            timestamp_ms: 10,
            target: Some(semantic_target()),
            is_password: false,
        });
        coalescer.push_event(RecorderInputEvent::Text {
            text: "llo".into(),
            timestamp_ms: 200,
            target: Some(semantic_target()),
            is_password: false,
        });
        let steps = coalescer.finish();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].tool, "input.type_text");
        assert_eq!(steps[0].args["text"], "");
        assert!(steps[0]
            .audit
            .notes
            .iter()
            .any(|note| note.contains("redacted")));
        assert!(matches!(steps[0].target, Some(MacroTarget::UiElement(_))));
    }

    #[test]
    fn coalescer_can_include_plaintext_for_explicit_tests_only() {
        let mut coalescer = RecordingCoalescer::new(RecordedTextPolicy::IncludePlaintextForTests);
        coalescer.push_event(RecorderInputEvent::Text {
            text: "hello".into(),
            timestamp_ms: 10,
            target: None,
            is_password: false,
        });
        let steps = coalescer.finish();
        assert_eq!(steps[0].tool, "input.type_text");
        assert_eq!(steps[0].args["text"], "hello");
    }

    #[test]
    fn coalescer_password_text_becomes_unbound_type_secret() {
        let mut coalescer = RecordingCoalescer::default();
        let emitted = coalescer.push_event(RecorderInputEvent::Text {
            text: "super-secret".into(),
            timestamp_ms: 10,
            target: Some(semantic_target()),
            is_password: true,
        });
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].tool, "macro.type_secret");
        assert_eq!(emitted[0].args["secret_ref"], "");
        let encoded = serde_json::to_string(&emitted).unwrap();
        assert!(!encoded.contains("super-secret"));
        assert!(emitted[0]
            .audit
            .notes
            .iter()
            .any(|note| note.contains("bind secret_ref")));
    }

    #[test]
    fn manifest_infers_launch_and_bind_from_first_recorded_window() {
        let mut coalescer = RecordingCoalescer::default();
        coalescer.push_event(RecorderInputEvent::MouseDown {
            pointer: pointer(100, 200, 10),
            button: RecorderMouseButton::Left,
        });
        coalescer.push_event(RecorderInputEvent::MouseUp {
            pointer: pointer(100, 200, 20),
            button: RecorderMouseButton::Left,
        });
        let session = RecordingSession {
            id: "recording-test".into(),
            title: "Recorded fixture".into(),
            description: "fixture recording".into(),
            tags: vec!["test".into()],
            app_identity: None,
            status: RecorderStatus::Idle,
            capture_input: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:01Z".into(),
            stopped_at: Some("2026-01-01T00:00:01Z".into()),
            steps: coalescer.finish(),
            notes: Vec::new(),
        };

        let manifest = manifest_from_session(&session);
        assert_eq!(manifest.launch.as_ref().unwrap().tool, "process.launch");
        assert_eq!(
            manifest.launch.as_ref().unwrap().args["exe"],
            "C:\\fixture.exe"
        );
        assert_eq!(
            manifest.bind.as_ref().unwrap().strategy,
            "recorder_inferred_fresh_launch_main_window"
        );
        assert_eq!(
            manifest.bind.as_ref().unwrap().required_executable.as_deref(),
            Some("fixture.exe")
        );
        assert_eq!(
            manifest.app_identity.as_ref().unwrap().executable_path.as_deref(),
            Some("C:\\fixture.exe")
        );
        assert_eq!(
            manifest.replay.extra["recorder"]["launch_inference"]["requires_confirmation"],
            true
        );
        assert_eq!(
            manifest.replay.extra["recorder"]["launch_inference"]["target_alias"],
            "app_main"
        );
    }

    fn pointer(x: i32, y: i32, timestamp_ms: u64) -> RecorderPointer {
        RecorderPointer {
            x,
            y,
            timestamp_ms,
            target: Some(semantic_target()),
        }
    }

    fn semantic_target() -> RecorderSemanticTarget {
        RecorderSemanticTarget {
            automation_id: Some("login".into()),
            name: Some("Login".into()),
            role: Some("Edit".into()),
            control_type: Some("Edit".into()),
            class_name: Some("Edit".into()),
            element_ref: Some("uia:fixture".into()),
            window: Some(RecorderWindowIdentity {
                hwnd_hex: Some("0x0000000000000123".into()),
                pid: Some(42),
                process_name: Some("fixture.exe".into()),
                exe_path: Some("C:\\fixture.exe".into()),
                title: Some("Fixture".into()),
            }),
            screenshot_path: None,
        }
    }
}
