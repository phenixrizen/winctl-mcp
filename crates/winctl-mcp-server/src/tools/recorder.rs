use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use winctl_macro::{
    tool_descriptor, validate_manifest, AppIdentity, CoordinateFallbackMetadata, MacroManifest,
    MacroStep, MacroTarget, Rect, Size, StepAudit, UiElementTarget, MACRO_MANIFEST_VERSION,
};
use winctl_memory::RememberRequest;

use crate::{
    AppState, RecorderExportRequest, RecorderRecordStepRequest, RecorderStartRequest,
    RecorderStopRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecorderRuntimeState {
    next_session_id: u64,
    active: Option<RecordingSession>,
    completed: HashMap<String, RecordingSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub app_identity: Option<AppIdentity>,
    pub created_at: String,
    pub updated_at: String,
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
                    if pointer_distance(&down.pointer, &pointer) >= 6.0 {
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
                if down.moved || distance >= 6.0 {
                    steps.extend(self.flush_click());
                    steps.push(self.drag_step(down.pointer, pointer, button));
                    return steps;
                }
                if let Some(click) = self.pending_click.take() {
                    if click.button == button
                        && pointer
                            .timestamp_ms
                            .saturating_sub(click.pointer.timestamp_ms)
                            <= 500
                        && pointer_distance(&click.pointer, &pointer) <= 6.0
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
                    steps.push(self.type_secret_step(target));
                    return steps;
                }
                if let Some(pending) = self.pending_text.as_mut() {
                    if timestamp_ms.saturating_sub(pending.timestamp_ms) <= 750
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
            RecordedTextPolicy::Redact => (String::new(), true),
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
            .map(|click| timestamp_ms.saturating_sub(click.pointer.timestamp_ms) > 500)
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
    let mut runtime = state
        .recorder_runtime
        .lock()
        .expect("recorder mutex poisoned");
    let id = runtime.next_id();
    let now = Utc::now().to_rfc3339();
    let session = RecordingSession {
        id: id.clone(),
        title: request.title,
        description: request.description.unwrap_or_default(),
        tags: request.tags,
        app_identity: request.app_identity,
        created_at: now.clone(),
        updated_at: now,
        steps: Vec::new(),
        notes: Vec::new(),
    };
    runtime.active = Some(session.clone());
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
        let Some(session) = runtime.active.take() else {
            return recorder_error("recording_not_active", "no active recording session");
        };
        runtime
            .completed
            .insert(session.id.clone(), session.clone());
        session
    };
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
        "active": runtime.active,
        "completed": runtime.completed.values().collect::<Vec<_>>(),
    })
}

fn manifest_from_session(session: &RecordingSession) -> MacroManifest {
    MacroManifest {
        version: MACRO_MANIFEST_VERSION.into(),
        kind: "test_procedure".into(),
        title: session.title.clone(),
        description: session.description.clone(),
        tags: session.tags.clone(),
        app_identity: session.app_identity.clone(),
        launch: None,
        bind: None,
        preconditions: Vec::new(),
        steps: session.steps.clone(),
        waits: Vec::new(),
        assertions: Vec::new(),
        cleanup: Vec::new(),
        artifacts: Default::default(),
        replay: Default::default(),
    }
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
