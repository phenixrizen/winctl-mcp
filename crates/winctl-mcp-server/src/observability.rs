use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const REQUEST_HISTORY_LIMIT: usize = 500;
const CLIENT_STALE_AFTER_MS: u64 = 15 * 60 * 1000;
const MAX_SUMMARY_STRING_LEN: usize = 160;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectedClient {
    pub connection_id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub transport: String,
    pub connected_at_unix_ms: u64,
    pub last_seen_unix_ms: u64,
    pub request_count: u64,
    pub last_tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestHistoryEntry {
    pub id: u64,
    pub connection_id: String,
    pub tool_name: String,
    pub started_at_unix_ms: u64,
    pub duration_ms: u64,
    pub ok: bool,
    pub error_code: Option<String>,
    pub summary: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ObservabilitySnapshot {
    pub connected_clients: Vec<ConnectedClient>,
    pub recent_requests: Vec<RequestHistoryEntry>,
}

#[derive(Debug, Default)]
pub struct ServerObservabilityState {
    inner: Mutex<ObservabilityInner>,
    connection_counter: AtomicU64,
    request_counter: AtomicU64,
}

#[derive(Debug, Default)]
struct ObservabilityInner {
    connected_clients: HashMap<String, ConnectedClient>,
    recent_requests: VecDeque<RequestHistoryEntry>,
}

impl ServerObservabilityState {
    pub fn register_connection(&self, transport: &str) -> String {
        let connection_id = if transport == "stdio" {
            "stdio".to_owned()
        } else {
            let id = self.connection_counter.fetch_add(1, Ordering::SeqCst) + 1;
            format!("{transport}-{id}")
        };
        let now = now_unix_ms();
        let mut inner = self.inner.lock().expect("observability mutex poisoned");
        prune_stale_clients(&mut inner, now);
        inner.connected_clients.insert(
            connection_id.clone(),
            ConnectedClient {
                connection_id: connection_id.clone(),
                name: None,
                version: None,
                transport: transport.to_owned(),
                connected_at_unix_ms: now,
                last_seen_unix_ms: now,
                request_count: 0,
                last_tool: None,
            },
        );
        connection_id
    }

    pub fn deregister_connection(&self, connection_id: &str) {
        let mut inner = self.inner.lock().expect("observability mutex poisoned");
        inner.connected_clients.remove(connection_id);
    }

    pub fn set_client_info(
        &self,
        connection_id: &str,
        name: impl Into<String>,
        version: impl Into<String>,
    ) {
        let now = now_unix_ms();
        let mut inner = self.inner.lock().expect("observability mutex poisoned");
        prune_stale_clients(&mut inner, now);
        if let Some(client) = inner.connected_clients.get_mut(connection_id) {
            client.name = Some(name.into());
            client.version = Some(version.into());
            client.last_seen_unix_ms = now;
        }
    }

    pub fn record_request(&self, request: RequestRecord<'_>) {
        let now = now_unix_ms();
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let mut inner = self.inner.lock().expect("observability mutex poisoned");
        prune_stale_clients(&mut inner, now);
        if let Some(client) = inner.connected_clients.get_mut(request.connection_id) {
            client.last_seen_unix_ms = now;
            client.request_count = client.request_count.saturating_add(1);
            client.last_tool = Some(request.tool_name.to_owned());
        }
        inner.recent_requests.push_back(RequestHistoryEntry {
            id,
            connection_id: request.connection_id.to_owned(),
            tool_name: request.tool_name.to_owned(),
            started_at_unix_ms: request.started_at_unix_ms,
            duration_ms: request.duration_ms,
            ok: request.ok,
            error_code: request.error_code,
            summary: safe_summary(request.tool_name, request.args, request.result),
        });
        while inner.recent_requests.len() > REQUEST_HISTORY_LIMIT {
            inner.recent_requests.pop_front();
        }
    }

    pub fn snapshot(&self) -> ObservabilitySnapshot {
        let now = now_unix_ms();
        let mut inner = self.inner.lock().expect("observability mutex poisoned");
        prune_stale_clients(&mut inner, now);
        let mut connected_clients: Vec<_> = inner.connected_clients.values().cloned().collect();
        connected_clients.sort_by(|left, right| {
            left.connected_at_unix_ms
                .cmp(&right.connected_at_unix_ms)
                .then_with(|| left.connection_id.cmp(&right.connection_id))
        });
        let recent_requests = inner.recent_requests.iter().cloned().collect();
        ObservabilitySnapshot {
            connected_clients,
            recent_requests,
        }
    }
}

pub struct RequestRecord<'a> {
    pub connection_id: &'a str,
    pub tool_name: &'a str,
    pub started_at_unix_ms: u64,
    pub duration_ms: u64,
    pub ok: bool,
    pub error_code: Option<String>,
    pub args: &'a Value,
    pub result: Option<&'a Value>,
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn call_tool_result_ok(result: &CallToolResult) -> bool {
    if let Some(ok) = result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("ok"))
        .and_then(Value::as_bool)
    {
        return ok;
    }
    !result.is_error.unwrap_or(false)
}

pub fn call_tool_error_code(result: &CallToolResult) -> Option<String> {
    result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn safe_summary(tool_name: &str, args: &Value, result: Option<&Value>) -> Value {
    let mut summary = Map::new();
    summary.insert("tool".to_owned(), Value::String(tool_name.to_owned()));
    summary.insert("args".to_owned(), summarize_root_value(args));
    if let Some(result) = result {
        summary.insert("result".to_owned(), summarize_root_value(result));
    }
    Value::Object(summary)
}

fn prune_stale_clients(inner: &mut ObservabilityInner, now: u64) {
    inner
        .connected_clients
        .retain(|_, client| now.saturating_sub(client.last_seen_unix_ms) <= CLIENT_STALE_AFTER_MS);
}

fn summarize_root_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => summarize_object(object),
        Value::Array(values) => json!({ "count": values.len() }),
        Value::Null => Value::Null,
        Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(_) => Value::String("<redacted_scalar>".to_owned()),
    }
}

fn summarize_object(object: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (key, value) in object {
        if is_allowed_summary_key(key) {
            out.insert(key.clone(), summarize_allowed_value(value));
            continue;
        }
        if let Some(count_key) = safe_collection_count_key(key, value) {
            out.insert(count_key, json!(value.as_array().map_or(0, Vec::len)));
        }
    }
    Value::Object(out)
}

fn summarize_allowed_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => summarize_object(object),
        Value::Array(values) => json!({ "count": values.len() }),
        Value::String(value) => Value::String(truncate_summary_string(value)),
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

fn safe_collection_count_key(key: &str, value: &Value) -> Option<String> {
    if !value.is_array() || is_sensitive_key(key) {
        return None;
    }
    if matches!(
        key,
        "windows"
            | "processes"
            | "candidates"
            | "matches"
            | "items"
            | "results"
            | "artifacts"
            | "events"
            | "entries"
            | "tools"
            | "notifications"
    ) {
        Some(format!("{key}_count"))
    } else {
        None
    }
}

fn is_allowed_summary_key(key: &str) -> bool {
    matches!(
        key,
        "ok" | "bound_id"
            | "pid"
            | "parent_pid"
            | "launch_id"
            | "hwnd"
            | "hwnd_hex"
            | "display_index"
            | "x"
            | "y"
            | "width"
            | "height"
            | "timeout_ms"
            | "duration_ms"
            | "elapsed_ms"
            | "tool_name"
            | "action_kind"
            | "session_id"
            | "run_id"
            | "recording_id"
            | "id"
            | "kind"
            | "status"
            | "state"
            | "count"
            | "total"
            | "matched"
            | "passed"
            | "exited"
            | "exit_code"
            | "force"
            | "kill_tree"
            | "error_code"
            | "code"
            | "transport"
    )
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key,
        "text"
            | "value"
            | "secret"
            | "secret_ref"
            | "clipboard"
            | "content"
            | "contents"
            | "stdout"
            | "stderr"
            | "base64"
            | "image"
            | "bytes"
            | "manifest"
            | "args"
            | "arguments"
            | "password"
            | "token"
    )
}

fn truncate_summary_string(value: &str) -> String {
    if value.chars().count() <= MAX_SUMMARY_STRING_LEN {
        return value.to_owned();
    }
    let truncated = value
        .chars()
        .take(MAX_SUMMARY_STRING_LEN.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redactor_omits_sensitive_fields_for_representative_tools() {
        let cases = [
            (
                "input.type_text",
                json!({
                    "bound_id": "hwnd:0x0000000000000001",
                    "text": "super secret typed text",
                    "x": 10,
                    "y": 20
                }),
                json!({"ok": true, "typed": 24, "value": "leaked"}),
            ),
            (
                "macro.type_secret",
                json!({
                    "bound_id": "hwnd:0x0000000000000001",
                    "secret_ref": "login-password",
                    "value": "plaintext"
                }),
                json!({"ok": false, "error": {"code": "missing_secret", "message": "secret_ref login-password"}}),
            ),
            (
                "clipboard.write",
                json!({"text": "clipboard contents", "value": "also sensitive"}),
                json!({"ok": true, "clipboard": "clipboard contents"}),
            ),
            (
                "memory.remember",
                json!({"kind": "procedure", "text": "memory text", "tags": ["login"]}),
                json!({"ok": true, "id": "mem-1", "text": "memory text"}),
            ),
        ];

        for (tool, args, result) in cases {
            let summary = safe_summary(tool, &args, Some(&result));
            let rendered = serde_json::to_string(&summary).unwrap();
            assert!(!rendered.contains("super secret typed text"));
            assert!(!rendered.contains("login-password"));
            assert!(!rendered.contains("clipboard contents"));
            assert!(!rendered.contains("memory text"));
            assert!(!rendered.contains("secret_ref"));
            assert!(!rendered.contains("\"text\""));
            assert!(!rendered.contains("\"value\""));
        }
    }

    #[test]
    fn request_history_evicts_oldest_entries() {
        let state = ServerObservabilityState::default();
        let connection_id = state.register_connection("http");
        for index in 0..(REQUEST_HISTORY_LIMIT + 5) {
            state.record_request(RequestRecord {
                connection_id: &connection_id,
                tool_name: "server.ping",
                started_at_unix_ms: index as u64,
                duration_ms: 1,
                ok: true,
                error_code: None,
                args: &json!({}),
                result: Some(&json!({"ok": true})),
            });
        }
        let snapshot = state.snapshot();
        assert_eq!(snapshot.recent_requests.len(), REQUEST_HISTORY_LIMIT);
        assert_eq!(snapshot.recent_requests[0].started_at_unix_ms, 5);
        assert_eq!(
            snapshot.connected_clients[0].request_count,
            (REQUEST_HISTORY_LIMIT + 5) as u64
        );
    }

    #[test]
    fn client_registry_updates_initialize_metadata_and_last_tool() {
        let state = ServerObservabilityState::default();
        let connection_id = state.register_connection("stdio");
        state.set_client_info(&connection_id, "codex", "1.2.3");
        state.record_request(RequestRecord {
            connection_id: &connection_id,
            tool_name: "windows.list",
            started_at_unix_ms: 42,
            duration_ms: 7,
            ok: true,
            error_code: None,
            args: &json!({}),
            result: Some(&json!({"ok": true, "windows": [1, 2, 3]})),
        });

        let snapshot = state.snapshot();
        let client = &snapshot.connected_clients[0];
        assert_eq!(client.connection_id, "stdio");
        assert_eq!(client.name.as_deref(), Some("codex"));
        assert_eq!(client.version.as_deref(), Some("1.2.3"));
        assert_eq!(client.request_count, 1);
        assert_eq!(client.last_tool.as_deref(), Some("windows.list"));
        assert_eq!(
            snapshot.recent_requests[0].summary["result"]["windows_count"],
            3
        );
    }

    #[test]
    fn call_tool_result_helpers_read_structured_ok_and_error_code() {
        let result = CallToolResult::structured(json!({
            "ok": false,
            "error": {"code": "control_consent_required"}
        }));
        assert!(!call_tool_result_ok(&result));
        assert_eq!(
            call_tool_error_code(&result).as_deref(),
            Some("control_consent_required")
        );
    }
}
