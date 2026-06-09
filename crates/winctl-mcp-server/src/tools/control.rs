use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::{mpsc, OnceLock};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    AppState, ControlArmRequest, ControlConsentRequest, ControlNotifyRequest, ControlRevokeRequest,
};

const MAX_EVENTS: usize = 200;
const CONTROL_AUDIT_SCHEMA_VERSION: u32 = 1;
const CONTROL_AUDIT_FILE_NAME: &str = "control-audit.jsonl";
const RECENT_AUDIT_ENTRIES: usize = 200;
const DEFAULT_ARM_MS: u64 = 5 * 60 * 1000;
const FIRST_CONTROL_COUNTDOWN_MS: u64 = 1_500;
static HOTKEY_STARTED: AtomicBool = AtomicBool::new(false);
static HOTKEY_REGISTERED: AtomicBool = AtomicBool::new(false);

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
pub struct ControlAuditRecord {
    pub schema_version: u32,
    pub event: ControlEvent,
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
    #[serde(skip)]
    pub audit_log_path: Option<PathBuf>,
    pub audit_last_write_error: Option<String>,
    pub audit_last_write_error_unix_ms: Option<u64>,
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
            audit_log_path: None,
            audit_last_write_error: None,
            audit_last_write_error_unix_ms: None,
        }
    }
}

pub fn configure_audit_log(runtime: &Arc<Mutex<ControlRuntimeState>>, capture_dir: &Path) {
    let path = capture_dir.join(CONTROL_AUDIT_FILE_NAME);
    let high_watermark = audit_high_watermark(&path);
    let mut runtime = runtime.lock().expect("control mutex poisoned");
    runtime.audit_log_path = Some(path.clone());
    match high_watermark {
        Ok(max_id) => {
            runtime.next_event_id = runtime.next_event_id.max(max_id.saturating_add(1));
            runtime.audit_last_write_error = None;
            runtime.audit_last_write_error_unix_ms = None;
            tracing::info!(
                path = %path.display(),
                next_event_id = runtime.next_event_id,
                "control audit log configured"
            );
        }
        Err(error) => {
            runtime.audit_last_write_error = Some(error.clone());
            runtime.audit_last_write_error_unix_ms = Some(now_unix_ms());
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to read existing control audit log high watermark"
            );
        }
    }
}

pub fn start_emergency_hotkey(runtime: Arc<Mutex<ControlRuntimeState>>) {
    if HOTKEY_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    HOTKEY_REGISTERED.store(false, Ordering::SeqCst);

    #[cfg(windows)]
    {
        thread::spawn(move || windows_hotkey_loop(runtime));
    }

    #[cfg(not(windows))]
    {
        let _ = runtime;
        HOTKEY_REGISTERED.store(false, Ordering::SeqCst);
        HOTKEY_STARTED.store(false, Ordering::SeqCst);
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
    pub native_toast: Option<serde_json::Value>,
    pub active_overlay: Option<serde_json::Value>,
}

pub fn control_state(state: &AppState) -> serde_json::Value {
    tracing::info!("control.state requested");
    let runtime = state
        .control_runtime
        .lock()
        .expect("control mutex poisoned");
    serde_json::json!({
        "ok": true,
        "control": snapshot_locked(&runtime),
    })
}

pub fn record_passive_audit_event(
    runtime: &Arc<Mutex<ControlRuntimeState>>,
    kind: &str,
    tool_name: Option<String>,
    action_kind: Option<String>,
    bound_id: Option<String>,
    session_id: Option<String>,
    message: String,
) -> Option<ControlEvent> {
    let mut runtime = match runtime.lock() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(
                kind = kind,
                error = %error,
                "failed to record passive control audit event because control runtime is poisoned"
            );
            return None;
        }
    };
    Some(push_event(
        &mut runtime,
        kind,
        tool_name,
        action_kind,
        bound_id,
        session_id,
        message,
    ))
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
    let allow_for_ms = request
        .allow_for_ms
        .unwrap_or(DEFAULT_ARM_MS)
        .clamp(1_000, 24 * 60 * 60 * 1000);
    let mut runtime = state
        .control_runtime
        .lock()
        .expect("control mutex poisoned");
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
        request
            .reason
            .unwrap_or_else(|| "control session armed".into()),
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
    let mut runtime = state
        .control_runtime
        .lock()
        .expect("control mutex poisoned");
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
    let mut runtime = state
        .control_runtime
        .lock()
        .expect("control mutex poisoned");
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
        request
            .reason
            .unwrap_or_else(|| "control session revoked".into()),
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
    let countdown_ms = request.countdown_ms.unwrap_or(FIRST_CONTROL_COUNTDOWN_MS);
    let (event, control_snapshot) = {
        let mut runtime = state
            .control_runtime
            .lock()
            .expect("control mutex poisoned");
        runtime.status = ControlStatus::Warning;
        runtime.session_id = request.session_id.clone();
        runtime.bound_id = request.bound_id.clone();
        runtime.target_identity = target_identity;
        runtime.first_control_notification_sent = true;
        let message = format!(
            "{} will control the desktop after {countdown_ms} ms",
            request.tool_name,
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
        (event, snapshot_locked(&runtime))
    };
    let native_toast = show_control_toast(&event, countdown_ms);
    serde_json::json!({
        "ok": true,
        "event": event,
        "native_toast": native_toast,
        "control": control_snapshot,
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
    let (
        status_before,
        notification_sent,
        notification_event,
        target_identity,
        armed_until,
        session_id,
    ) = {
        let mut runtime = state
            .control_runtime
            .lock()
            .expect("control mutex poisoned");
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
        runtime.bound_id = bound_id
            .map(str::to_owned)
            .or_else(|| runtime.bound_id.clone());
        runtime.target_identity = target_identity.or_else(|| runtime.target_identity.clone());
        runtime.active_tool = Some(tool_name.into());
        runtime.active_action_kind = Some(action_kind.into());
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
            if notification_sent {
                "first sensitive control action in this session; native toast countdown started"
            } else {
                "control action entered active state"
            }
            .into(),
        );
        (
            status_before,
            notification_sent,
            event,
            runtime.target_identity.clone(),
            runtime.armed_until_unix_ms,
            runtime.session_id.clone(),
        )
    };

    let native_toast = if notification_sent {
        let toast = show_control_toast(&notification_event, FIRST_CONTROL_COUNTDOWN_MS);
        record_control_event(
            state,
            "native_toast",
            Some(tool_name.into()),
            Some(action_kind.into()),
            bound_id.map(str::to_owned),
            session_id.clone(),
            format!(
                "native toast attempted; shown={}",
                toast
                    .get("shown")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
            ),
        );
        wait_for_control_countdown(state, FIRST_CONTROL_COUNTDOWN_MS)?;
        Some(toast)
    } else {
        None
    };

    let start_event_id = if notification_sent {
        let mut runtime = state
            .control_runtime
            .lock()
            .expect("control mutex poisoned");
        if runtime.emergency_stop_active || matches!(runtime.status, ControlStatus::Revoked) {
            return Err(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "control_revoked",
                    "message": "control was revoked during the notification countdown"
                },
                "control": snapshot_locked(&runtime),
            }));
        }
        runtime.status = ControlStatus::Controlling;
        let event_session_id = runtime.session_id.clone();
        let event_bound_id = runtime.bound_id.clone();
        let event = push_event(
            &mut runtime,
            "control_started",
            Some(tool_name.into()),
            Some(action_kind.into()),
            event_bound_id,
            event_session_id,
            "control action entered active state".into(),
        );
        event.id
    } else {
        notification_event.id
    };

    let overlay = show_active_control_overlay(target_identity.as_ref());
    record_control_event(
        state,
        "overlay_started",
        Some(tool_name.into()),
        Some(action_kind.into()),
        bound_id.map(str::to_owned),
        session_id.clone(),
        format!(
            "active-control overlay attempted; visible={}",
            overlay
                .get("visible")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        ),
    );

    Ok(ControlPreflight {
        status_before,
        status_after: ControlStatus::Controlling,
        notification_sent,
        armed_until_unix_ms: armed_until,
        session_id,
        event_id: if notification_sent {
            notification_event.id
        } else {
            start_event_id
        },
        native_toast,
        active_overlay: Some(overlay),
    })
}

pub fn finish_control_action(
    state: &AppState,
    tool_name: &'static str,
    action_kind: &'static str,
    ok: bool,
) {
    let mut runtime = state
        .control_runtime
        .lock()
        .expect("control mutex poisoned");
    if matches!(runtime.status, ControlStatus::Revoked) {
        let overlay = hide_active_control_overlay();
        let event_bound_id = runtime.bound_id.clone();
        let event_session_id = runtime.session_id.clone();
        push_event(
            &mut runtime,
            "overlay_cleared",
            Some(tool_name.into()),
            Some(action_kind.into()),
            event_bound_id,
            event_session_id,
            format!(
                "active-control overlay cleared after revocation; hidden={}",
                overlay
                    .get("hidden")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
            ),
        );
        return;
    }
    let overlay = hide_active_control_overlay();
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
    let event_bound_id = runtime.bound_id.clone();
    let event_session_id = runtime.session_id.clone();
    push_event(
        &mut runtime,
        "overlay_cleared",
        Some(tool_name.into()),
        Some(action_kind.into()),
        event_bound_id,
        event_session_id,
        format!(
            "active-control overlay cleared; hidden={}",
            overlay
                .get("hidden")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        ),
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

fn record_control_event(
    state: &AppState,
    kind: &str,
    tool_name: Option<String>,
    action_kind: Option<String>,
    bound_id: Option<String>,
    session_id: Option<String>,
    message: String,
) -> ControlEvent {
    let mut runtime = state
        .control_runtime
        .lock()
        .expect("control mutex poisoned");
    push_event(
        &mut runtime,
        kind,
        tool_name,
        action_kind,
        bound_id,
        session_id,
        message,
    )
}

fn wait_for_control_countdown(
    state: &AppState,
    countdown_ms: u64,
) -> Result<(), serde_json::Value> {
    let started = std::time::Instant::now();
    let timeout = Duration::from_millis(countdown_ms);
    while started.elapsed() < timeout {
        {
            let runtime = state
                .control_runtime
                .lock()
                .expect("control mutex poisoned");
            if runtime.emergency_stop_active || matches!(runtime.status, ControlStatus::Revoked) {
                return Err(serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "control_revoked",
                        "message": "control was revoked during the notification countdown"
                    },
                    "control": snapshot_locked(&runtime),
                }));
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn show_control_toast(event: &ControlEvent, countdown_ms: u64) -> serde_json::Value {
    tracing::info!(
        event_id = event.id,
        tool_name = ?event.tool_name,
        action_kind = ?event.action_kind,
        countdown_ms = countdown_ms,
        "native control toast requested"
    );
    show_control_toast_platform(event, countdown_ms)
}

#[cfg(not(windows))]
fn show_control_toast_platform(event: &ControlEvent, countdown_ms: u64) -> serde_json::Value {
    let _ = (event, countdown_ms);
    serde_json::json!({
        "attempted": true,
        "provider_enabled": false,
        "shown": false,
        "message": "native Windows toast provider is only available on Windows"
    })
}

#[cfg(windows)]
fn show_control_toast_platform(event: &ControlEvent, countdown_ms: u64) -> serde_json::Value {
    let target = toast_target_text(event.target_identity.as_ref());
    let title = "winctl-mcp desktop control";
    let tool = event.tool_name.as_deref().unwrap_or("unknown tool");
    let body = format!("{tool} is about to control {target}");
    let cancel = format!(
        "Press Ctrl+Alt+Esc within {:.1}s to cancel.",
        countdown_ms as f64 / 1000.0
    );
    match show_windows_toast(title, &body, &cancel) {
        Ok(()) => {
            tracing::info!(event_id = event.id, "native control toast shown");
            serde_json::json!({
                "attempted": true,
                "provider_enabled": true,
                "shown": true,
                "app_id": "winctl-mcp",
                "target": target,
                "countdown_ms": countdown_ms,
            })
        }
        Err(error) => {
            tracing::warn!(event_id = event.id, error = %error, "native control toast failed");
            serde_json::json!({
                "attempted": true,
                "provider_enabled": true,
                "shown": false,
                "app_id": "winctl-mcp",
                "target": target,
                "countdown_ms": countdown_ms,
                "error": {
                    "code": "native_toast_failed",
                    "message": error,
                    "hint": "desktop toast delivery can require notification permissions and a registered AppUserModelID"
                }
            })
        }
    }
}

#[cfg(windows)]
fn show_windows_toast(title: &str, body: &str, cancel: &str) -> Result<(), String> {
    use windows::core::w;
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    let ro_initialized = match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(error = %error, "RoInitialize failed before showing toast; trying WinRT activation anyway");
            false
        }
    };
    let xml = format!(
        r#"<toast duration="short"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text><text>{}</text></binding></visual></toast>"#,
        xml_escape(title),
        xml_escape(body),
        xml_escape(cancel)
    );
    let result = (|| {
        unsafe {
            SetCurrentProcessExplicitAppUserModelID(w!("winctl-mcp"))
                .map_err(|error| error.to_string())?;
        }
        let doc = XmlDocument::new().map_err(|error| error.to_string())?;
        doc.LoadXml(&HSTRING::from(xml))
            .map_err(|error| error.to_string())?;
        let toast =
            ToastNotification::CreateToastNotification(&doc).map_err(|error| error.to_string())?;
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("winctl-mcp"))
                .map_err(|error| error.to_string())?;
        notifier.Show(&toast).map_err(|error| error.to_string())
    })();
    if ro_initialized {
        unsafe { RoUninitialize() };
    }
    result
}

#[cfg(windows)]
fn toast_target_text(target_identity: Option<&serde_json::Value>) -> String {
    let Some(target) = target_identity else {
        return "the active desktop".into();
    };
    let process = target
        .get("process_name")
        .and_then(|value| value.as_str())
        .unwrap_or("target app");
    let pid = target
        .get("pid")
        .and_then(|value| value.as_u64())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let hwnd = target
        .get("hwnd_hex")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown HWND");
    format!("{process} (PID {pid}, {hwnd})")
}

#[cfg(windows)]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn show_active_control_overlay(target_identity: Option<&serde_json::Value>) -> serde_json::Value {
    show_active_control_overlay_platform(target_identity)
}

fn hide_active_control_overlay() -> serde_json::Value {
    hide_active_control_overlay_platform()
}

#[cfg(not(windows))]
fn show_active_control_overlay_platform(
    target_identity: Option<&serde_json::Value>,
) -> serde_json::Value {
    let _ = target_identity;
    serde_json::json!({
        "attempted": true,
        "provider_enabled": false,
        "visible": false,
        "message": "active-control overlay provider is only available on Windows"
    })
}

#[cfg(not(windows))]
fn hide_active_control_overlay_platform() -> serde_json::Value {
    serde_json::json!({
        "attempted": true,
        "provider_enabled": false,
        "hidden": false,
        "message": "active-control overlay provider is only available on Windows"
    })
}

#[cfg(windows)]
fn show_active_control_overlay_platform(
    target_identity: Option<&serde_json::Value>,
) -> serde_json::Value {
    let Some(hwnd) = target_identity.and_then(hwnd_from_target_identity) else {
        return serde_json::json!({
            "attempted": false,
            "provider_enabled": true,
            "visible": false,
            "error": {
                "code": "overlay_target_missing",
                "message": "target identity did not include an HWND"
            }
        });
    };
    match overlay_show(hwnd) {
        Ok(()) => {
            tracing::info!(hwnd = hwnd, "active-control overlay show requested");
            serde_json::json!({
                "attempted": true,
                "provider_enabled": true,
                "visible": true,
                "hwnd": hwnd,
            })
        }
        Err(error) => {
            tracing::warn!(hwnd = hwnd, error = %error, "active-control overlay show failed");
            serde_json::json!({
                "attempted": true,
                "provider_enabled": true,
                "visible": false,
                "hwnd": hwnd,
                "error": {
                    "code": "overlay_show_failed",
                    "message": error
                }
            })
        }
    }
}

#[cfg(windows)]
fn hide_active_control_overlay_platform() -> serde_json::Value {
    match overlay_hide() {
        Ok(()) => {
            tracing::info!("active-control overlay hide requested");
            serde_json::json!({
                "attempted": true,
                "provider_enabled": true,
                "hidden": true,
            })
        }
        Err(error) => {
            tracing::warn!(error = %error, "active-control overlay hide failed");
            serde_json::json!({
                "attempted": true,
                "provider_enabled": true,
                "hidden": false,
                "error": {
                    "code": "overlay_hide_failed",
                    "message": error
                }
            })
        }
    }
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
    let preflight =
        match preflight_control_action(state, tool_name, bound_id, action_kind, sensitive) {
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
        "emergency_hotkey_started": HOTKEY_STARTED.load(Ordering::SeqCst),
        "emergency_hotkey_registered": HOTKEY_REGISTERED.load(Ordering::SeqCst),
        "active_tool": runtime.active_tool,
        "active_action_kind": runtime.active_action_kind,
        "events": runtime.events,
        "audit_log": audit_snapshot(runtime),
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
    append_audit_event(runtime, &event);
    event
}

fn audit_snapshot(runtime: &ControlRuntimeState) -> serde_json::Value {
    let (recent_entries, recent_read_error) = match runtime.audit_log_path.as_deref() {
        Some(path) => match read_recent_audit_records(path, RECENT_AUDIT_ENTRIES) {
            Ok(entries) => (entries, None),
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "failed to read recent control audit entries"
                );
                (Vec::new(), Some(error))
            }
        },
        None => (Vec::new(), None),
    };
    serde_json::json!({
        "enabled": runtime.audit_log_path.is_some(),
        "format": "jsonl",
        "path": runtime.audit_log_path.as_ref().map(|path| path.display().to_string()),
        "last_write_error": runtime.audit_last_write_error,
        "last_write_error_unix_ms": runtime.audit_last_write_error_unix_ms,
        "recent_persisted_entries": recent_entries,
        "recent_persisted_error": recent_read_error,
    })
}

fn append_audit_event(runtime: &mut ControlRuntimeState, event: &ControlEvent) {
    let Some(path) = runtime.audit_log_path.clone() else {
        return;
    };
    let record = ControlAuditRecord {
        schema_version: CONTROL_AUDIT_SCHEMA_VERSION,
        event: event.clone(),
    };
    match append_audit_record(&path, &record) {
        Ok(()) => {
            runtime.audit_last_write_error = None;
            runtime.audit_last_write_error_unix_ms = None;
        }
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                event_id = event.id,
                error = %error,
                "failed to append control audit event"
            );
            runtime.audit_last_write_error = Some(error);
            runtime.audit_last_write_error_unix_ms = Some(now_unix_ms());
        }
    }
}

fn append_audit_record(path: &Path, record: &ControlAuditRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create control audit directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let line = serde_json::to_string(record)
        .map_err(|error| format!("failed to serialize control audit record: {error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to open control audit log {}: {error}",
                path.display()
            )
        })?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| {
            format!(
                "failed to write control audit log {}: {error}",
                path.display()
            )
        })
}

fn audit_high_watermark(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let records = read_recent_audit_records(path, usize::MAX)?;
    Ok(records
        .iter()
        .map(|record| record.event.id)
        .max()
        .unwrap_or(0))
}

fn read_recent_audit_records(path: &Path, limit: usize) -> Result<Vec<ControlAuditRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(path).map_err(|error| {
        format!(
            "failed to open control audit log {}: {error}",
            path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut records = VecDeque::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            format!(
                "failed to read control audit log {} at line {}: {error}",
                path.display(),
                index + 1
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str::<ControlAuditRecord>(&line).map_err(|error| {
            format!(
                "failed to parse control audit log {} at line {}: {error}",
                path.display(),
                index + 1
            )
        })?;
        records.push_back(record);
        while records.len() > limit {
            records.pop_front();
        }
    }
    Ok(records.into_iter().collect())
}

#[cfg(windows)]
#[derive(Debug)]
enum OverlayCommand {
    Show {
        hwnd: isize,
        ack: mpsc::Sender<Result<(), String>>,
    },
    Hide {
        ack: mpsc::Sender<Result<(), String>>,
    },
}

#[cfg(windows)]
static OVERLAY_SENDER: OnceLock<Option<Mutex<mpsc::Sender<OverlayCommand>>>> = OnceLock::new();

#[cfg(windows)]
fn overlay_show(hwnd: isize) -> Result<(), String> {
    overlay_send(|ack| OverlayCommand::Show { hwnd, ack })
}

#[cfg(windows)]
fn overlay_hide() -> Result<(), String> {
    overlay_send(|ack| OverlayCommand::Hide { ack })
}

#[cfg(windows)]
fn overlay_send<F>(build: F) -> Result<(), String>
where
    F: FnOnce(mpsc::Sender<Result<(), String>>) -> OverlayCommand,
{
    let sender = OVERLAY_SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("winctl-control-overlay".into())
            .spawn(move || overlay_thread(receiver))
        {
            Ok(_) => Some(Mutex::new(sender)),
            Err(error) => {
                tracing::warn!(error = %error, "failed to start active-control overlay thread");
                None
            }
        }
    });
    let Some(sender) = sender else {
        return Err("active-control overlay thread did not start".into());
    };
    let (ack_sender, ack_receiver) = mpsc::channel();
    sender
        .lock()
        .map_err(|_| "overlay sender mutex poisoned".to_owned())?
        .send(build(ack_sender))
        .map_err(|error| error.to_string())?;
    ack_receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| format!("timed out waiting for overlay thread acknowledgment: {error}"))?
}

#[cfg(windows)]
fn overlay_thread(receiver: mpsc::Receiver<OverlayCommand>) {
    use std::time::Duration as StdDuration;
    use windows::core::w;
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, FrameRect, HGDIOBJ,
        PAINTSTRUCT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DispatchMessageW, GetClientRect, GetWindowRect, PeekMessageW,
        RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage,
        CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, LWA_COLORKEY, MSG, PM_REMOVE, SWP_NOACTIVATE,
        SWP_NOOWNERZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA, WM_PAINT, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    unsafe extern "system" fn overlay_wnd_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_PAINT {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut rect = RECT::default();
            if unsafe { GetClientRect(hwnd, &mut rect) }.is_ok() {
                let black = unsafe { CreateSolidBrush(COLORREF(0)) };
                unsafe {
                    FillRect(hdc, &rect, black);
                    let _ = DeleteObject(HGDIOBJ(black.0));
                }
                let border = unsafe { CreateSolidBrush(COLORREF(0x00FFAA00)) };
                for inset in 0..4 {
                    let frame = RECT {
                        left: rect.left + inset,
                        top: rect.top + inset,
                        right: rect.right - inset,
                        bottom: rect.bottom - inset,
                    };
                    unsafe {
                        FrameRect(hdc, &frame, border);
                    }
                }
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(border.0));
                }
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn ensure_overlay_window() -> Result<HWND, String> {
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::UI::WindowsAndMessaging::CreateWindowExW;

        let module = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
        let instance = HINSTANCE(module.0);
        let class_name = w!("WinctlMcpControlOverlay");
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_wnd_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wnd_class);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                class_name,
                w!("winctl-mcp control overlay"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            )
        }
        .map_err(|error| error.to_string())?;
        unsafe {
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_COLORKEY)
                .map_err(|error| error.to_string())?;
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        Ok(hwnd)
    }

    fn show_for_target(overlay: HWND, target: isize) -> Result<(), String> {
        let target = HWND(target as *mut std::ffi::c_void);
        let mut rect = RECT::default();
        unsafe { GetWindowRect(target, &mut rect) }.map_err(|error| error.to_string())?;
        let margin = 6;
        let width = (rect.right - rect.left).saturating_add(margin * 2).max(1);
        let height = (rect.bottom - rect.top).saturating_add(margin * 2).max(1);
        unsafe {
            SetWindowPos(
                overlay,
                Some(HWND_TOPMOST),
                rect.left - margin,
                rect.top - margin,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
            )
            .map_err(|error| error.to_string())?;
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(overlay), None, true);
            let _ = ShowWindow(overlay, SW_SHOWNA);
        }
        Ok(())
    }

    let mut overlay = match ensure_overlay_window() {
        Ok(hwnd) => Some(hwnd),
        Err(error) => {
            tracing::warn!(error = %error, "failed to create active-control overlay window");
            None
        }
    };
    loop {
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        match receiver.recv_timeout(StdDuration::from_millis(50)) {
            Ok(OverlayCommand::Show { hwnd, ack }) => {
                if overlay.is_none() {
                    overlay = match ensure_overlay_window() {
                        Ok(hwnd) => Some(hwnd),
                        Err(error) => {
                            let _ = ack.send(Err(error));
                            continue;
                        }
                    };
                }
                let result = overlay
                    .map(|overlay_hwnd| show_for_target(overlay_hwnd, hwnd))
                    .unwrap_or_else(|| Err("active-control overlay window was not created".into()));
                if let Err(error) = &result {
                    tracing::warn!(hwnd = hwnd, error = %error, "failed to show active-control overlay");
                }
                let _ = ack.send(result);
            }
            Ok(OverlayCommand::Hide { ack }) => {
                if let Some(overlay_hwnd) = overlay {
                    unsafe {
                        let _ = ShowWindow(overlay_hwnd, SW_HIDE);
                    }
                }
                let _ = ack.send(Ok(()));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(windows)]
fn hwnd_from_target_identity(target: &serde_json::Value) -> Option<isize> {
    target
        .get("hwnd")
        .and_then(|value| value.as_i64())
        .and_then(|value| isize::try_from(value).ok())
        .or_else(|| {
            target
                .get("hwnd_hex")
                .and_then(|value| value.as_str())
                .and_then(|value| value.strip_prefix("0x").or(Some(value)))
                .and_then(|value| isize::from_str_radix(value, 16).ok())
        })
}

#[cfg(windows)]
fn apply_emergency_stop(runtime: &mut ControlRuntimeState, reason: &str) -> ControlEvent {
    let _ = hide_active_control_overlay();
    runtime.status = ControlStatus::Revoked;
    runtime.armed_until_unix_ms = None;
    runtime.last_decision = Some("emergency_stop".into());
    runtime.emergency_stop_active = true;
    runtime.active_tool = None;
    runtime.active_action_kind = None;
    let event_bound_id = runtime.bound_id.clone();
    let event_session_id = runtime.session_id.clone();
    push_event(
        runtime,
        "emergency_stop",
        Some("global_hotkey".into()),
        Some("control_revoke".into()),
        event_bound_id,
        event_session_id,
        reason.into(),
    )
}

#[cfg(windows)]
fn windows_hotkey_loop(runtime: Arc<Mutex<ControlRuntimeState>>) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_ESCAPE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    const HOTKEY_ID: i32 = 0x5743;
    let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    if let Err(error) = unsafe { RegisterHotKey(None, HOTKEY_ID, modifiers, VK_ESCAPE.0 as u32) } {
        HOTKEY_REGISTERED.store(false, Ordering::SeqCst);
        HOTKEY_STARTED.store(false, Ordering::SeqCst);
        tracing::warn!(
            error = %error,
            "failed to register Ctrl+Alt+Esc emergency stop hotkey"
        );
        return;
    }
    HOTKEY_REGISTERED.store(true, Ordering::SeqCst);
    tracing::info!("registered Ctrl+Alt+Esc emergency stop hotkey");

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
            let mut runtime = runtime.lock().expect("control mutex poisoned");
            let event = apply_emergency_stop(
                &mut runtime,
                "global Ctrl+Alt+Esc emergency stop hotkey pressed",
            );
            tracing::warn!(
                event_id = event.id,
                "desktop control revoked by global emergency stop hotkey"
            );
        }
    }
    let _ = unsafe { UnregisterHotKey(None, HOTKEY_ID) };
    HOTKEY_REGISTERED.store(false, Ordering::SeqCst);
    HOTKEY_STARTED.store(false, Ordering::SeqCst);
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
