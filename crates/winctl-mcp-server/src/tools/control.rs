use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    AppState, ControlArmRequest, ControlConsentRequest, ControlNotifyRequest, ControlRevokeRequest,
};

const MAX_EVENTS: usize = 200;
const DEFAULT_ARM_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlStatus {
    Idle,
    Armed,
    Warning,
    Controlling,
    Cooldown,
    Blocked,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlEvent {
    pub id: u64,
    pub timestamp_unix_ms: u64,
    pub kind: String,
    pub status: ControlStatus,
    pub tool_name: Option<String>,
    pub action_kind: Option<String>,
    pub bound_id: Option<String>,
    pub session_id: Option<String>,
    pub message: String,
    pub target_identity: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlRuntimeState {
    pub status: ControlStatus,
    pub session_id: Option<String>,
    pub bound_id: Option<String>,
    pub target_identity: Option<serde_json::Value>,
    pub armed_until_unix_ms: Option<u64>,
    pub last_decision: Option<String>,
    pub require_explicit_consent: bool,
    pub first_control_notification_sent: bool,
    pub emergency_stop_active: bool,
    pub active_tool: Option<String>,
    pub active_action_kind: Option<String>,
    pub next_event_id: u64,
    pub events: VecDeque<ControlEvent>,
}

impl Default for ControlRuntimeState {
    fn default() -> Self {
        Self {
            status: ControlStatus::Idle,
            session_id: None,
            bound_id: None,
            target_identity: None,
            armed_until_unix_ms: None,
            last_decision: None,
            require_explicit_consent: false,
            first_control_notification_sent: false,
            emergency_stop_active: false,
            active_tool: None,
            active_action_kind: None,
            next_event_id: 1,
            events: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPreflight {
    pub status_before: ControlStatus,
    pub status_after: ControlStatus,
    pub notification_sent: bool,
    pub armed_until_unix_ms: Option<u64>,
    pub session_id: Option<String>,
    pub event_id: u64,
}

pub fn control_state(state: &AppState) -> serde_json::Value {
    tracing::info!("control.state requested");
    let runtime = state.control_runtime.lock().expect("control mutex poisoned");
    serde_json::json!({
        "ok": true,
        "control": snapshot_locked(&runtime),
    })
}

pub fn control_arm(state: &AppState, request: ControlArmRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = ?request.bound_id,
        session_id = ?request.session_id,
        allow_for_ms = ?request.allow_for_ms,
        reason = ?request.reason,
        "control.arm requested"
    );
    let target_identity = request
        .bound_id
        .as_ref()
        .and_then(|bound_id| bound_identity(state, bound_id));
    let now = now_unix_ms();
    let allow_for_ms = request.allow_for_ms.unwrap_or(DEFAULT_ARM_MS).clamp(1_000, 24 * 60 * 60 * 1000);
    let mut runtime = state.control_runtime.lock().expect("control mutex poisoned");
    runtime.status = ControlStatus::Armed;
    runtime.session_id = request.session_id.clone();
    runtime.bound_id = request.bound_id.clone();
    runtime.target_identity = target_identity;
    runtime.armed_until_unix_ms = Some(now.saturating_add(allow_for_ms));
    runtime.last_decision = Some("allow_session".into());
    runtime.emergency_stop_active = false;
    let event = push_event(
        &mut runtime,
        "armed",
        None,
        None,
        request.bound_id,
        request.session_id,
        request.reason.unwrap_or_else(|| "control session armed".into()),
    );
    serde_json::json!({
        "ok": true,
        "event": event,
        "control": snapshot_locked(&runtime),
    })
}

pub fn control_consent(state: &AppState, request: ControlConsentRequest) -> serde_json::Value {
    tracing::info!(
        decision = %request.decision,
        bound_id = ?request.bound_id,
        session_id = ?request.session_id,
        allow_for_ms = ?request.allow_for_ms,
        "control.consent requested"
    );
    let decision = request.decision.to_ascii_lowercase();
    let target_identity = request
        .bound_id
        .as_ref()
        .and_then(|bound_id| bound_identity(state, bound_id));
    let now = now_unix_ms();
    let mut runtime = state.control_runtime.lock().expect("control mutex poisoned");
    let (status, armed_until) = match decision.as_str() {
        "allow_once" => (ControlStatus::Armed, Some(now.saturating_add(30_000))),
        "allow_session" => {
            let allow_for_ms = request
                .allow_for_ms
                .unwrap_or(DEFAULT_ARM_MS)
                .clamp(1_000, 24 * 60 * 60 * 1000);
            (ControlStatus::Armed, Some(now.saturating_add(allow_for_ms)))
        }
        "deny" => (ControlStatus::Blocked, None),
        "revoke_session" | "revoke" => (ControlStatus::Revoked, None),
        _ => {
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "invalid_control_decision",
                    "message": "decision must be allow_once, allow_session, deny, or revoke_session"
                }
            });
        }
    };
    runtime.status = status;
    runtime.session_id = request.session_id.clone();
    runtime.bound_id = request.bound_id.clone();
    runtime.target_identity = target_identity;
    runtime.armed_until_unix_ms = armed_until;
    runtime.last_decision = Some(decision.clone());
    runtime.emergency_stop_active = matches!(status, ControlStatus::Revoked);
    let event = push_event(
        &mut runtime,
        "consent",
        None,
        None,
        request.bound_id,
        request.session_id,
        format!("control consent decision: {decision}"),
    );
    serde_json::json!({
        "ok": true,
        "event": event,
        "control": snapshot_locked(&runtime),
    })
}

pub fn control_revoke(state: &AppState, request: ControlRevokeRequest) -> serde_json::Value {
    tracing::warn!(
        reason = ?request.reason,
        session_id = ?request.session_id,
        "control.revoke requested"
    );
    let mut runtime = state.control_runtime.lock().expect("control mutex poisoned");
    runtime.status = ControlStatus::Revoked;
    runtime.armed_until_unix_ms = None;
    runtime.last_decision = Some("revoke_session".into());
    runtime.emergency_stop_active = true;
    runtime.active_tool = None;
    runtime.active_action_kind = None;
    let event_bound_id = runtime.bound_id.clone();
    let event = push_event(
        &mut runtime,
        "revoked",
        None,
        None,
        event_bound_id,
        request.session_id,
        request.reason.unwrap_or_else(|| "control session revoked".into()),
    );
    serde_json::json!({
        "ok": true,
        "event": event,
        "control": snapshot_locked(&runtime),
    })
}

pub fn control_notify(state: &AppState, request: ControlNotifyRequest) -> serde_json::Value {
    tracing::info!(
        tool_name = %request.tool_name,
        bound_id = ?request.bound_id,
        action_kind = ?request.action_kind,
        countdown_ms = ?request.countdown_ms,
        "control.notify requested"
    );
    let target_identity = request
        .bound_id
        .as_ref()
        .and_then(|bound_id| bound_identity(state, bound_id));
    let mut runtime = state.control_runtime.lock().expect("control mutex poisoned");
    runtime.status = ControlStatus::Warning;
    runtime.session_id = request.session_id.clone();
    runtime.bound_id = request.bound_id.clone();
    runtime.target_identity = target_identity;
    runtime.first_control_notification_sent = true;
    let message = format!(
        "{} will control the desktop{}",
        request.tool_name,
        request
            .countdown_ms
            .map(|value| format!(" after {value} ms"))
            .unwrap_or_default()
    );
    let event = push_event(
        &mut runtime,
        "notification",
        Some(request.tool_name),
        request.action_kind,
        request.bound_id,
        request.session_id,
        message,
    );
    serde_json::json!({
        "ok": true,
        "event": event,
        "native_toast": {
            "attempted": false,
            "provider_enabled": false,
            "message": "native toast provider is not enabled in this build; dashboard/tray can display the control event"
        },
        "control": snapshot_locked(&runtime),
    })
}

pub fn preflight_control_action(
    state: &AppState,
    tool_name: &'static str,
    bound_id: Option<&str>,
    action_kind: &'static str,
    sensitive: bool,
) -> Result<ControlPreflight, serde_json::Value> {
    let target_identity = bound_id.and_then(|id| bound_identity(state, id));
    let now = now_unix_ms();
    let mut runtime = state.control_runtime.lock().expect("control mutex poisoned");
    let status_before = runtime.status;
    if runtime.emergency_stop_active || matches!(runtime.status, ControlStatus::Revoked) {
        let event_session_id = runtime.session_id.clone();
        let event = push_event(
            &mut runtime,
            "blocked",
            Some(tool_name.into()),
            Some(action_kind.into()),
            bound_id.map(str::to_owned),
            event_session_id,
            "control action rejected because emergency stop or revocation is active".into(),
        );
        tracing::warn!(
            tool_name = tool_name,
            action_kind = action_kind,
            bound_id = ?bound_id,
            event_id = event.id,
            "control action rejected by revoked state"
        );
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "control_revoked",
                "message": "control has been revoked; call control.arm or control.consent before sensitive actions"
            },
            "control": snapshot_locked(&runtime),
        }));
    }

    if runtime
        .armed_until_unix_ms
        .map(|until| until < now)
        .unwrap_or(false)
    {
        runtime.status = ControlStatus::Idle;
        runtime.armed_until_unix_ms = None;
        runtime.last_decision = Some("expired".into());
    }

    if sensitive
        && runtime.require_explicit_consent
        && !matches!(runtime.status, ControlStatus::Armed)
    {
        runtime.status = ControlStatus::Blocked;
        let event_session_id = runtime.session_id.clone();
        let event = push_event(
            &mut runtime,
            "blocked",
            Some(tool_name.into()),
            Some(action_kind.into()),
            bound_id.map(str::to_owned),
            event_session_id,
            "control action rejected because explicit consent is required".into(),
        );
        tracing::warn!(
            tool_name = tool_name,
            action_kind = action_kind,
            bound_id = ?bound_id,
            event_id = event.id,
            "control action rejected by consent gate"
        );
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "control_consent_required",
                "message": "explicit control consent is required before this sensitive action"
            },
            "control": snapshot_locked(&runtime),
        }));
    }

    let notification_sent = if sensitive && !runtime.first_control_notification_sent {
        runtime.first_control_notification_sent = true;
        true
    } else {
        false
    };
    runtime.status = if notification_sent {
        ControlStatus::Warning
    } else {
        ControlStatus::Controlling
    };
    runtime.bound_id = bound_id.map(str::to_owned).or_else(|| runtime.bound_id.clone());
    runtime.target_identity = target_identity.or_else(|| runtime.target_identity.clone());
    runtime.active_tool = Some(tool_name.into());
    runtime.active_action_kind = Some(action_kind.into());
    let message = if notification_sent {
        "first sensitive control action in this session; dashboard/tray should notify the user"
    } else {
        "control action entered active state"
    };
    let event_session_id = runtime.session_id.clone();
    let event = push_event(
        &mut runtime,
        if notification_sent {
            "notification"
        } else {
            "control_started"
        },
        Some(tool_name.into()),
        Some(action_kind.into()),
        bound_id.map(str::to_owned),
        event_session_id,
        message.into(),
    );
    runtime.status = ControlStatus::Controlling;
    Ok(ControlPreflight {
        status_before,
        status_after: runtime.status,
        notification_sent,
        armed_until_unix_ms: runtime.armed_until_unix_ms,
        session_id: runtime.session_id.clone(),
        event_id: event.id,
    })
}

pub fn finish_control_action(
    state: &AppState,
    tool_name: &'static str,
    action_kind: &'static str,
    ok: bool,
) {
    let mut runtime = state.control_runtime.lock().expect("control mutex poisoned");
    if matches!(runtime.status, ControlStatus::Revoked) {
        return;
    }
    runtime.status = ControlStatus::Cooldown;
    let event_bound_id = runtime.bound_id.clone();
    let event_session_id = runtime.session_id.clone();
    let event = push_event(
        &mut runtime,
        "control_finished",
        Some(tool_name.into()),
        Some(action_kind.into()),
        event_bound_id,
        event_session_id,
        if ok {
            "control action completed"
        } else {
            "control action failed"
        }
        .into(),
    );
    tracing::info!(
        tool_name = tool_name,
        action_kind = action_kind,
        ok = ok,
        event_id = event.id,
        "control action finished"
    );
    if runtime
        .armed_until_unix_ms
        .map(|until| until >= now_unix_ms())
        .unwrap_or(false)
    {
        runtime.status = ControlStatus::Armed;
    } else {
        runtime.status = ControlStatus::Idle;
        runtime.armed_until_unix_ms = None;
    }
    runtime.active_tool = None;
    runtime.active_action_kind = None;
}

pub fn with_control_gate<F>(
    state: &AppState,
    tool_name: &'static str,
    bound_id: Option<&str>,
    action_kind: &'static str,
    sensitive: bool,
    operation: F,
) -> serde_json::Value
where
    F: FnOnce() -> serde_json::Value,
{
    let preflight = match preflight_control_action(state, tool_name, bound_id, action_kind, sensitive)
    {
        Ok(preflight) => preflight,
        Err(error) => return error,
    };
    let mut output = operation();
    let ok = output
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    finish_control_action(state, tool_name, action_kind, ok);
    if let Some(object) = output.as_object_mut() {
        object.insert("control_preflight".into(), serde_json::json!(preflight));
    }
    output
}

fn snapshot_locked(runtime: &ControlRuntimeState) -> serde_json::Value {
    serde_json::json!({
        "status": runtime.status,
        "session_id": runtime.session_id,
        "bound_id": runtime.bound_id,
        "target_identity": runtime.target_identity,
        "armed_until_unix_ms": runtime.armed_until_unix_ms,
        "last_decision": runtime.last_decision,
        "require_explicit_consent": runtime.require_explicit_consent,
        "first_control_notification_sent": runtime.first_control_notification_sent,
        "emergency_stop_active": runtime.emergency_stop_active,
        "active_tool": runtime.active_tool,
        "active_action_kind": runtime.active_action_kind,
        "events": runtime.events,
    })
}

fn push_event(
    runtime: &mut ControlRuntimeState,
    kind: &str,
    tool_name: Option<String>,
    action_kind: Option<String>,
    bound_id: Option<String>,
    session_id: Option<String>,
    message: String,
) -> ControlEvent {
    let event = ControlEvent {
        id: runtime.next_event_id,
        timestamp_unix_ms: now_unix_ms(),
        kind: kind.into(),
        status: runtime.status,
        tool_name,
        action_kind,
        bound_id,
        session_id,
        message,
        target_identity: runtime.target_identity.clone(),
    };
    runtime.next_event_id = runtime.next_event_id.saturating_add(1);
    runtime.events.push_back(event.clone());
    while runtime.events.len() > MAX_EVENTS {
        runtime.events.pop_front();
    }
    event
}

fn bound_identity(state: &AppState, bound_id: &str) -> Option<serde_json::Value> {
    state
        .bound
        .lock()
        .ok()
        .and_then(|guard| guard.get(bound_id).cloned())
        .map(|bound| {
            serde_json::json!({
                "bound_id": bound.bound_id,
                "hwnd": bound.identity.hwnd,
                "hwnd_hex": bound.identity.hwnd_hex,
                "pid": bound.identity.pid,
                "process_name": bound.identity.process_name,
                "exe_path": bound.identity.exe_path,
                "title_at_bind": bound.title_at_bind,
            })
        })
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
