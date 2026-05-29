use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use winctl_macro::{
    validate_manifest, AppIdentity, MacroManifest, MacroStep, MACRO_MANIFEST_VERSION,
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

impl RecorderRuntimeState {
    fn next_id(&mut self) -> String {
        self.next_session_id += 1;
        format!("recording-{}", self.next_session_id)
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
    let step = MacroStep {
        id: request.id.unwrap_or_else(|| format!("step-{step_index}")),
        tool: request.tool,
        args: request.args.unwrap_or(Value::Null),
        target: request.target,
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

fn recorder_error(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}
