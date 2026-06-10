# Dashboard Server-Config Editor + Tray-Supervised Restart — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the redundant dashboard "Config" tab with a real editor that edits the MCP server's operational TOML config, persists it, and restarts the server via the tray to apply.

**Architecture:** Pure config-IO helpers (load/merge/validate/serialize/atomic-write + tray discovery/restart-command) live in a new `tools/config_editor.rs`. Three gated axum handlers in `main.rs` (`GET/POST /dashboard/config`, `POST /dashboard/restart`) wire them to the dashboard. The handlers reuse the existing `DashboardState`/`HttpAuthRegistry`; restart is delegated to `winctl-tray restart --config <file>` spawned detached. The frontend `config` tab is rebuilt as a structured Settings form.

**Tech Stack:** Rust (axum 0.x, serde, toml, anyhow), Vue 3 + Vite + Tailwind/daisyUI dashboard.

**Spec:** `docs/superpowers/specs/2026-06-06-dashboard-config-restart-design.md`

---

## File Structure

- **Create** `crates/winctl-mcp-server/src/tools/config_editor.rs` — pure, unit-tested helpers: `ConfigSaveBody`, `merge_editable`, `validate_editable`, `serialize_config`, `write_atomic`, `sections_from_policy`, `find_tray_binary`, `build_restart_command`, `spawn_restart`.
- **Modify** `crates/winctl-mcp-server/src/tools/mod.rs` — register the module.
- **Modify** `crates/winctl-mcp-server/src/main.rs` — add `Serialize` + `skip_serializing_if` to the config-file structs; add `config_file` to `DashboardState`; add three handlers + a shared guard + three routes.
- **Modify** `crates/winctl-mcp-server/dashboard/src/main.js` — delete old config-tab code; add Settings form, data, fetch helpers, restart/reconnect; rename tab label to `Settings`.
- **Create** `crates/winctl-mcp-server/tests/config_http.rs` — HTTP integration test (load/save/persist/gating). Does **not** exercise the real restart path.

**Editable sections:** `[policy] [paths] [logging] [embedding] [macro_execution]`. `[transport]`/`listen` and `[auth]` token are read-only.

---

## Task 1: Add `Serialize` to the config-file structs

**Files:**
- Modify: `crates/winctl-mcp-server/src/main.rs:5250-5313` (struct derives) — add a unit test module near them.

- [ ] **Step 1: Write the failing test** (add at the end of `main.rs`, before any existing trailing `#[cfg(test)]` or at EOF)

```rust
#[cfg(test)]
mod config_serde_tests {
    use super::*;

    #[test]
    fn round_trips_and_omits_none() {
        let mut file = WinctlConfigFile::default();
        file.policy = Some(PolicyFileConfig {
            enable_filesystem_mutation: Some(true),
            tool_denylist: Some(vec!["registry.write".to_string()]),
            ..Default::default()
        });
        let text = toml::to_string_pretty(&file).expect("serialize");
        assert!(text.contains("[policy]"), "got: {text}");
        assert!(text.contains("enable_filesystem_mutation = true"), "got: {text}");
        assert!(!text.contains("[transport]"), "None sections must be omitted: {text}");
        assert!(!text.contains("enable_clipboard_write"), "None fields must be omitted: {text}");

        let parsed: WinctlConfigFile = toml::from_str(&text).expect("parse");
        assert_eq!(
            parsed.policy.unwrap().enable_filesystem_mutation,
            Some(true)
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p winctl-mcp-server --bin winctl-mcp-server round_trips_and_omits_none`
Expected: FAIL to compile — `WinctlConfigFile` does not implement `Serialize` (and `toml::to_string_pretty` requires it).

- [ ] **Step 3: Add `Serialize` + `skip_serializing_if` to all seven config-file structs**

For each of `WinctlConfigFile`, `TransportFileConfig`, `AuthFileConfig`, `LoggingFileConfig`, `PathsFileConfig`, `PolicyFileConfig`, `EmbeddingFileConfig`, `MacroExecutionFileConfig` (main.rs:5250-5313): change the derive line from
`#[derive(Debug, Clone, Default, Deserialize)]` to `#[derive(Debug, Clone, Default, Serialize, Deserialize)]`,
and add `#[serde(default, skip_serializing_if = "Option::is_none")]` above **every** field (all fields are `Option<_>`). Example for `WinctlConfigFile`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WinctlConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transport: Option<TransportFileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<AuthFileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logging: Option<LoggingFileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paths: Option<PathsFileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy: Option<PolicyFileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedding: Option<EmbeddingFileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    macro_execution: Option<MacroExecutionFileConfig>,
}
```

Apply the identical treatment (derive + per-field attribute) to the other six structs. `PolicyFileConfig` has twelve `Option<_>` fields, including `tool_allowlist: Option<Vec<String>>` and `tool_denylist: Option<Vec<String>>` — attribute each.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p winctl-mcp-server --bin winctl-mcp-server round_trips_and_omits_none`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/winctl-mcp-server/src/main.rs
git commit -m "feat: make config-file structs serializable for the config editor"
```

---

## Task 2: Config-IO helpers (merge / validate / serialize / atomic write)

**Files:**
- Create: `crates/winctl-mcp-server/src/tools/config_editor.rs`
- Modify: `crates/winctl-mcp-server/src/tools/mod.rs` (register module)

- [ ] **Step 1: Register the module**

In `crates/winctl-mcp-server/src/tools/mod.rs`, add (alongside the other `pub mod` lines):

```rust
pub mod config_editor;
```

- [ ] **Step 2: Write the failing tests + the module skeleton with signatures**

Create `crates/winctl-mcp-server/src/tools/config_editor.rs`:

```rust
//! Dashboard server-config editor: pure helpers for loading the editable view of
//! the server config, merging dashboard edits into the existing TOML, validating,
//! serializing, and atomically persisting it — plus locating the tray binary and
//! building the `winctl-tray restart` command. No I/O happens except in
//! `write_atomic`/`spawn_restart`; everything else is pure for testability.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    EmbeddingFileConfig, LoggingFileConfig, MacroExecutionFileConfig, PathsFileConfig,
    PolicyFileConfig, SecurityPolicy, ServeConfig, WinctlConfigFile,
};

/// Editable operational sections accepted from the dashboard. Reuses the
/// existing file-config section types so there is one source of truth.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ConfigSaveBody {
    #[serde(default)]
    pub policy: Option<PolicyFileConfig>,
    #[serde(default)]
    pub paths: Option<PathsFileConfig>,
    #[serde(default)]
    pub logging: Option<LoggingFileConfig>,
    #[serde(default)]
    pub embedding: Option<EmbeddingFileConfig>,
    #[serde(default)]
    pub macro_execution: Option<MacroExecutionFileConfig>,
}

/// A single validation problem, surfaced per-field to the UI.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct FieldError {
    pub field: String,
    pub message: String,
}

/// Overlay the editable sections from `edits` onto a copy of `existing`,
/// leaving `[transport]`, `[auth]`, and any other untouched fields intact.
pub(crate) fn merge_editable(existing: &WinctlConfigFile, edits: &ConfigSaveBody) -> WinctlConfigFile {
    let mut merged = existing.clone();
    if edits.policy.is_some() {
        merged.policy = edits.policy.clone();
    }
    if edits.paths.is_some() {
        merged.paths = edits.paths.clone();
    }
    if edits.logging.is_some() {
        merged.logging = edits.logging.clone();
    }
    if edits.embedding.is_some() {
        merged.embedding = edits.embedding.clone();
    }
    if edits.macro_execution.is_some() {
        merged.macro_execution = edits.macro_execution.clone();
    }
    merged
}

/// Validate a candidate config file. Returns the field errors (empty = valid).
pub(crate) fn validate_editable(file: &WinctlConfigFile) -> Vec<FieldError> {
    let mut errors = Vec::new();

    if let Some(embedding) = &file.embedding {
        if matches!(embedding.dimension, Some(0)) {
            errors.push(FieldError {
                field: "embedding.dimension".to_string(),
                message: "embedding dimension must be greater than zero".to_string(),
            });
        }
    }
    if let Some(paths) = &file.paths {
        if let Some(roots) = &paths.filesystem_roots {
            for (idx, root) in roots.iter().enumerate() {
                if root.as_os_str().is_empty() {
                    errors.push(FieldError {
                        field: format!("paths.filesystem_roots[{idx}]"),
                        message: "filesystem root must not be empty".to_string(),
                    });
                }
            }
        }
    }

    // Whole-file sanity: it must still resolve to a valid ServeConfig
    // (catches an unparsable listen, etc., even though those fields are locked).
    if let Err(error) = ServeConfig::from_file(file) {
        errors.push(FieldError {
            field: "config".to_string(),
            message: format!("{error:#}"),
        });
    }

    errors
}

/// Serialize a config file to pretty TOML.
pub(crate) fn serialize_config(file: &WinctlConfigFile) -> anyhow::Result<String> {
    Ok(toml::to_string_pretty(file)?)
}

/// Atomically replace `path` with `contents` (temp sibling + rename).
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path) // MoveFileExW with REPLACE_EXISTING on Windows
}

/// Build the four editable operational sections from the live in-memory policy,
/// used for the read-only view when the server has no `--config` file.
pub(crate) fn sections_from_policy(
    policy: &SecurityPolicy,
    capture_dir: &Path,
) -> (
    PolicyFileConfig,
    PathsFileConfig,
    EmbeddingFileConfig,
    MacroExecutionFileConfig,
) {
    let policy_section = PolicyFileConfig {
        enable_filesystem_mutation: Some(policy.enable_filesystem_mutation),
        enable_clipboard_write: Some(policy.enable_clipboard_write),
        enable_registry_mutation: Some(policy.enable_registry_mutation),
        allow_private_network: Some(policy.allow_private_network),
        memory_mutation_enabled: Some(policy.memory_mutation_enabled),
        macro_execution_enabled: Some(policy.macro_execution_enabled),
        macro_destructive_tools_allowed: Some(policy.macro_destructive_tools_allowed),
        max_macro_runtime_ms: policy.max_macro_runtime_ms,
        max_macro_steps: policy.max_macro_steps,
        screenshot_retention_count: policy.screenshot_retention_count,
        tool_allowlist: Some(policy.tool_allowlist.clone()),
        tool_denylist: Some(policy.tool_denylist.clone()),
    };
    let paths_section = PathsFileConfig {
        capture_dir: Some(capture_dir.to_path_buf()),
        artifact_dir: policy.artifact_dir.clone(),
        filesystem_roots: Some(policy.filesystem_roots.clone()),
        memory_db: None,
    };
    let embedding_section = EmbeddingFileConfig {
        model_path: policy.embedding_model_path.clone(),
        dimension: policy.embedding_dimension,
    };
    let macro_section = MacroExecutionFileConfig {
        enabled: Some(policy.macro_execution_enabled),
        allow_destructive_tools: Some(policy.macro_destructive_tools_allowed),
        max_runtime_ms: policy.max_macro_runtime_ms,
        max_steps: policy.max_macro_steps,
    };
    (policy_section, paths_section, embedding_section, macro_section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_file() -> WinctlConfigFile {
        let text = "\
[transport]
mode = \"http\"
listen = \"127.0.0.1:8765\"

[auth]
token = \"keep-me\"

[policy]
enable_filesystem_mutation = false
";
        toml::from_str(text).expect("base file parses")
    }

    #[test]
    fn merge_preserves_transport_and_auth_and_replaces_policy() {
        let existing = base_file();
        let edits = ConfigSaveBody {
            policy: Some(PolicyFileConfig {
                enable_filesystem_mutation: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge_editable(&existing, &edits);
        assert_eq!(
            merged.transport.as_ref().unwrap().listen.as_deref(),
            Some("127.0.0.1:8765")
        );
        assert_eq!(merged.auth.as_ref().unwrap().token.as_deref(), Some("keep-me"));
        assert_eq!(
            merged.policy.as_ref().unwrap().enable_filesystem_mutation,
            Some(true)
        );
    }

    #[test]
    fn serialize_round_trips_and_keeps_locked_sections() {
        let existing = base_file();
        let edits = ConfigSaveBody {
            policy: Some(PolicyFileConfig {
                enable_registry_mutation: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge_editable(&existing, &edits);
        let text = serialize_config(&merged).expect("serialize");
        assert!(text.contains("[auth]"));
        assert!(text.contains("keep-me"));
        let reparsed: WinctlConfigFile = toml::from_str(&text).expect("reparse");
        assert_eq!(
            reparsed.policy.unwrap().enable_registry_mutation,
            Some(true)
        );
    }

    #[test]
    fn validate_rejects_zero_dimension() {
        let mut file = base_file();
        file.embedding = Some(EmbeddingFileConfig {
            model_path: None,
            dimension: Some(0),
        });
        let errors = validate_editable(&file);
        assert!(errors.iter().any(|e| e.field == "embedding.dimension"));
    }

    #[test]
    fn validate_accepts_clean_file() {
        assert!(validate_editable(&base_file()).is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p winctl-mcp-server --bin winctl-mcp-server config_editor::`
Expected: PASS for all four tests (`merge_preserves_…`, `serialize_round_trips_…`, `validate_rejects_zero_dimension`, `validate_accepts_clean_file`).

> Note: the `use crate::{...}` imports reference crate-root-private structs; child modules can see ancestor-private items, so this compiles. If the compiler reports a name is private across an intervening module, change that struct in `main.rs` from `struct X` to `pub(crate) struct X` (no behavior change).

- [ ] **Step 4: Commit**

```bash
git add crates/winctl-mcp-server/src/tools/config_editor.rs crates/winctl-mcp-server/src/tools/mod.rs
git commit -m "feat: add config-editor merge/validate/serialize helpers"
```

---

## Task 3: Tray discovery + restart command

**Files:**
- Modify: `crates/winctl-mcp-server/src/tools/config_editor.rs`

- [ ] **Step 1: Write the failing tests** (append to the `tests` module in `config_editor.rs`)

```rust
    #[test]
    fn restart_command_targets_tray_restart_with_config() {
        let tray = Path::new("/opt/winctl/winctl-tray");
        let config = Path::new("/etc/winctl/config.toml");
        let cmd = build_restart_command(tray, config);
        let prog = cmd.get_program().to_string_lossy().to_string();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(prog.ends_with("winctl-tray"));
        assert_eq!(args, vec!["restart", "--config", "/etc/winctl/config.toml"]);
    }

    #[test]
    fn find_tray_binary_is_none_next_to_test_harness() {
        // The test harness exe lives in target/.../deps/, which has no winctl-tray sibling.
        assert!(find_tray_binary().is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p winctl-mcp-server --bin winctl-mcp-server config_editor::tests::restart_command`
Expected: FAIL to compile — `build_restart_command` / `find_tray_binary` not defined.

- [ ] **Step 3: Implement the three functions** (add above the `#[cfg(test)]` module in `config_editor.rs`)

```rust
/// Name of the tray binary as built by the workspace (sibling of the server exe).
#[cfg(windows)]
const TRAY_BINARY: &str = "winctl-tray.exe";
#[cfg(not(windows))]
const TRAY_BINARY: &str = "winctl-tray";

/// Locate the tray binary next to the running server executable.
pub(crate) fn find_tray_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(TRAY_BINARY);
    candidate.is_file().then_some(candidate)
}

/// Build (but do not spawn) `winctl-tray restart --config <config_file>`.
pub(crate) fn build_restart_command(tray: &Path, config_file: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(tray);
    command.arg("restart").arg("--config").arg(config_file);
    command
}

/// Spawn the tray restart command fully detached so it survives this process
/// being killed by the very restart it triggers.
pub(crate) fn spawn_restart(tray: &Path, config_file: &Path) -> anyhow::Result<()> {
    use anyhow::Context as _;
    use std::process::Stdio;

    let mut command = build_restart_command(tray, config_file);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    command
        .spawn()
        .with_context(|| format!("failed to spawn {}", tray.display()))?;
    Ok(())
}

#[cfg(windows)]
fn detach(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    command.creation_flags(DETACHED_PROCESS);
}

#[cfg(not(windows))]
fn detach(_command: &mut std::process::Command) {}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p winctl-mcp-server --bin winctl-mcp-server config_editor::`
Expected: PASS for all six `config_editor` tests.

- [ ] **Step 5: Commit**

```bash
git add crates/winctl-mcp-server/src/tools/config_editor.rs
git commit -m "feat: add tray-restart command builder and detached spawn"
```

---

## Task 4: Thread the config-file path into `DashboardState`

**Files:**
- Modify: `crates/winctl-mcp-server/src/main.rs:4497-4501` (struct), `:4254-4258` (construction)

- [ ] **Step 1: Add the field to `DashboardState`** (main.rs:4497)

```rust
struct DashboardState {
    app_state: AppState,
    auth: Arc<HttpAuthRegistry>,
    listen: SocketAddr,
    config_file: Option<PathBuf>,
}
```

- [ ] **Step 2: Populate it where `DashboardState` is constructed** in `run_mcp_http` (main.rs:4254)

```rust
    let dashboard_state = DashboardState {
        app_state: state.clone(),
        auth: auth_registry.clone(),
        listen: config.listen,
        config_file: config.config_file.clone(),
    };
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p winctl-mcp-server`
Expected: success (no other constructor of `DashboardState` exists; if the compiler flags one, add `config_file: None` there).

- [ ] **Step 4: Commit**

```bash
git add crates/winctl-mcp-server/src/main.rs
git commit -m "feat: expose config-file path to dashboard handlers"
```

---

## Task 5: `GET /dashboard/config` handler + route

**Files:**
- Modify: `crates/winctl-mcp-server/src/main.rs` (new handler near the other `dashboard_*` handlers ~4528; route in the dashboard router ~4263)

- [ ] **Step 1: Add the handler** (place near `dashboard_connect_json`, main.rs:4528)

```rust
async fn dashboard_config_json(State(state): State<DashboardState>) -> impl IntoResponse {
    let transport_mode;
    let listen_label = state.listen.to_string();
    let auth_required = state.auth.auth_required();

    let (editable, auth_token_set, sections) = match &state.config_file {
        Some(path) => match std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str::<WinctlConfigFile>(&text).ok())
        {
            Some(file) => {
                transport_mode = file
                    .transport
                    .as_ref()
                    .and_then(|t| t.mode.clone())
                    .unwrap_or_else(|| "http".to_string());
                let token_set = file
                    .auth
                    .as_ref()
                    .and_then(|a| a.token.as_ref())
                    .is_some();
                let sections = serde_json::json!({
                    "policy": file.policy,
                    "paths": file.paths,
                    "logging": file.logging,
                    "embedding": file.embedding,
                    "macro_execution": file.macro_execution,
                });
                (true, token_set, sections)
            }
            None => {
                transport_mode = "http".to_string();
                (false, false, serde_json::Value::Null)
            }
        },
        None => {
            transport_mode = "http".to_string();
            let (policy, paths, embedding, macro_execution) =
                tools::config_editor::sections_from_policy(
                    state.app_state.policy.as_ref(),
                    state.app_state.capture_dir.as_ref(),
                );
            let sections = serde_json::json!({
                "policy": policy,
                "paths": paths,
                "logging": serde_json::Value::Null,
                "embedding": embedding,
                "macro_execution": macro_execution,
            });
            (false, false, sections)
        }
    };

    AxumJson(serde_json::json!({
        "ok": true,
        "editable": editable,
        "config_file": state.config_file.as_ref().map(|p| p.display().to_string()),
        "tray_available": tools::config_editor::find_tray_binary().is_some(),
        "connection": {
            "transport": transport_mode,
            "listen": listen_label,
            "auth_required": auth_required,
            "auth_token_set": auth_token_set,
        },
        "sections": sections,
    }))
}
```

> The auth token **value** is never included — only `auth_token_set`/`auth_required` booleans.

- [ ] **Step 2: Register the route** in the dashboard router (main.rs, after the `/dashboard/connect` route ~4263)

```rust
        .route("/dashboard/config", get(dashboard_config_json))
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p winctl-mcp-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/winctl-mcp-server/src/main.rs
git commit -m "feat: add GET /dashboard/config endpoint"
```

---

## Task 6: `POST /dashboard/config` handler + shared guard + route

**Files:**
- Modify: `crates/winctl-mcp-server/src/main.rs` (guard helper + handler near Task 5's; route)

- [ ] **Step 1: Add the shared guard helper** (place just above `dashboard_config_json`)

```rust
/// Gate for config-mutating dashboard endpoints: loopback-only, and a valid
/// bearer token in the `Authorization` header (header-only — no query token),
/// required even on loopback. Returns the rejection response on failure.
fn config_endpoint_guard(
    state: &DashboardState,
    headers: &HeaderMap,
) -> Result<(), Response> {
    if !state.listen.ip().is_loopback() {
        return Err((
            StatusCode::FORBIDDEN,
            AxumJson(serde_json::json!({"ok": false, "reason": "non_loopback"})),
        )
            .into_response());
    }
    let authorized = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(|token| state.auth.authorize(token))
        .unwrap_or(false);
    if !authorized {
        return Err((
            StatusCode::FORBIDDEN,
            AxumJson(serde_json::json!({"ok": false, "reason": "unauthorized"})),
        )
            .into_response());
    }
    Ok(())
}
```

- [ ] **Step 2: Add the POST handler**

```rust
async fn dashboard_config_save(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    AxumJson(body): AxumJson<tools::config_editor::ConfigSaveBody>,
) -> Response {
    if let Err(rejection) = config_endpoint_guard(&state, &headers) {
        return rejection;
    }
    let Some(path) = state.config_file.clone() else {
        return (
            StatusCode::CONFLICT,
            AxumJson(serde_json::json!({"ok": false, "reason": "no_config_file"})),
        )
            .into_response();
    };

    let existing = match std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str::<WinctlConfigFile>(&text).ok())
    {
        Some(file) => file,
        None => WinctlConfigFile::default(),
    };
    let merged = tools::config_editor::merge_editable(&existing, &body);

    let errors = tools::config_editor::validate_editable(&merged);
    if !errors.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            AxumJson(serde_json::json!({"ok": false, "errors": errors})),
        )
            .into_response();
    }

    let serialized = match tools::config_editor::serialize_config(&merged) {
        Ok(text) => text,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                AxumJson(serde_json::json!({"ok": false, "error": error.to_string()})),
            )
                .into_response();
        }
    };
    if let Err(error) = tools::config_editor::write_atomic(&path, &serialized) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(serde_json::json!({"ok": false, "error": error.to_string()})),
        )
            .into_response();
    }
    tracing::info!(config_file = %path.display(), "dashboard config saved");
    AxumJson(serde_json::json!({"ok": true, "restart_required": true})).into_response()
}
```

- [ ] **Step 3: Register the route by MODIFYING the line added in Task 5** (do not add a second `.route()` for `/dashboard/config` — two routes on the same path panic axum at startup). Change:

```rust
        .route("/dashboard/config", get(dashboard_config_json))
```

to:

```rust
        .route("/dashboard/config", get(dashboard_config_json).post(dashboard_config_save))
```

- [ ] **Step 4: Verify it compiles** (confirm `HeaderMap`, `StatusCode`, `Response`, `post` are imported — they are used by existing handlers/middleware)

Run: `cargo check -p winctl-mcp-server`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/winctl-mcp-server/src/main.rs
git commit -m "feat: add gated POST /dashboard/config save endpoint"
```

---

## Task 7: `POST /dashboard/restart` handler + route

**Files:**
- Modify: `crates/winctl-mcp-server/src/main.rs`

- [ ] **Step 1: Add the handler**

```rust
async fn dashboard_config_restart(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    if let Err(rejection) = config_endpoint_guard(&state, &headers) {
        return rejection;
    }
    let Some(path) = state.config_file.clone() else {
        return (
            StatusCode::CONFLICT,
            AxumJson(serde_json::json!({"ok": false, "reason": "no_config_file"})),
        )
            .into_response();
    };
    match tools::config_editor::find_tray_binary() {
        Some(tray) => match tools::config_editor::spawn_restart(&tray, &path) {
            Ok(()) => {
                tracing::info!(config_file = %path.display(), "dashboard restart requested via tray");
                AxumJson(serde_json::json!({"restarting": true, "poll_url": "/healthz"}))
                    .into_response()
            }
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                AxumJson(serde_json::json!({"restarting": false, "error": error.to_string()})),
            )
                .into_response(),
        },
        None => AxumJson(serde_json::json!({
            "restarting": false,
            "manual": true,
            "instructions": format!("Run: winctl-tray restart --config {}", path.display()),
        }))
        .into_response(),
    }
}
```

- [ ] **Step 2: Register the route**

```rust
        .route("/dashboard/restart", post(dashboard_config_restart))
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p winctl-mcp-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/winctl-mcp-server/src/main.rs
git commit -m "feat: add gated POST /dashboard/restart endpoint"
```

---

## Task 8: HTTP integration test (load / save / persist / gating)

**Files:**
- Create: `crates/winctl-mcp-server/tests/config_http.rs`

> Does NOT call `POST /dashboard/restart` for real (the workspace `winctl-tray` is a build-sibling and would actually restart the server). Restart correctness is covered by the unit tests in Task 3.

- [ ] **Step 1: Write the integration test**

```rust
use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().expect("addr").port()
}

struct ServerGuard(Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_health(base: &str) {
    let started = Instant::now();
    let client = reqwest::blocking::Client::new();
    while started.elapsed() < Duration::from_secs(10) {
        if client
            .get(format!("{base}/healthz"))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("server did not become healthy");
}

#[test]
fn config_endpoint_loads_saves_and_gates() {
    let port = free_port();
    let cfg_path = std::env::temp_dir().join(format!("winctl-config-test-{}.toml", std::process::id()));
    std::fs::write(
        &cfg_path,
        format!("[transport]\nmode = \"http\"\nlisten = \"127.0.0.1:{port}\"\n\n[policy]\nenable_filesystem_mutation = false\n"),
    )
    .expect("write config");

    let exe = env!("CARGO_BIN_EXE_winctl-mcp-server");
    let child = Command::new(exe)
        .arg("serve")
        .arg("--transport")
        .arg("http")
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--config")
        .arg(&cfg_path)
        .spawn()
        .expect("spawn server");
    let _guard = ServerGuard(child);
    let base = format!("http://127.0.0.1:{port}");
    wait_for_health(&base);

    let client = reqwest::blocking::Client::new();

    // 1. GET returns an editable view and never leaks a token value.
    let get: serde_json::Value = client
        .get(format!("{base}/dashboard/config"))
        .send()
        .expect("get config")
        .json()
        .expect("json");
    assert_eq!(get["ok"], true);
    assert_eq!(get["editable"], true);
    assert_eq!(get["sections"]["policy"]["enable_filesystem_mutation"], false);
    assert!(get.to_string().to_lowercase().find("token").map_or(true, |_| !get.to_string().contains("\"token\":\"")));

    // 2. POST without a bearer token is refused (token required even on loopback).
    let unauth = client
        .post(format!("{base}/dashboard/config"))
        .json(&serde_json::json!({"policy": {"enable_filesystem_mutation": true}}))
        .send()
        .expect("post unauth");
    assert_eq!(unauth.status().as_u16(), 403);

    // 3. Bootstrap a token via the open (tokenless-loopback) connect endpoint.
    let token: serde_json::Value = client
        .post(format!("{base}/dashboard/connect/token"))
        .json(&serde_json::json!({"label": "test"}))
        .send()
        .expect("create token")
        .json()
        .expect("token json");
    let bearer = token["token"].as_str().expect("token string").to_string();

    // 4. Authorized save succeeds.
    let save = client
        .post(format!("{base}/dashboard/config"))
        .bearer_auth(&bearer)
        .json(&serde_json::json!({"policy": {"enable_filesystem_mutation": true}}))
        .send()
        .expect("post save");
    assert_eq!(save.status().as_u16(), 200);

    // 5. The change persisted and the locked [transport] section is preserved.
    let on_disk = std::fs::read_to_string(&cfg_path).expect("read back");
    assert!(on_disk.contains("enable_filesystem_mutation = true"), "got: {on_disk}");
    assert!(on_disk.contains("[transport]"), "transport must be preserved: {on_disk}");

    let _ = std::fs::remove_file(&cfg_path);
}
```

> Delete the unused `temp_config()`/`ServerGuard`-via-`temp_config` helper if your linter flags it; the test above inlines config creation. (Kept minimal — remove the stray `temp_config` fn before committing if `cargo test` warns.)

- [ ] **Step 2: Ensure `reqwest` blocking is available for tests**

Check `crates/winctl-mcp-server/Cargo.toml`. `reqwest` is already a dependency. If the `blocking` feature isn't enabled, add a dev-dependency:

```toml
[dev-dependencies]
reqwest = { workspace = true, features = ["blocking", "json"] }
```

(If `stdio_smoke.rs` already uses async `reqwest`, prefer matching that style instead — rewrite the test with `#[tokio::test]` and async `reqwest`. Pick the style the existing tests use; do not introduce a second HTTP-client paradigm.)

- [ ] **Step 3: Run the test**

Run: `cargo test -p winctl-mcp-server --test config_http`
Expected: PASS (server boots, GET editable, POST 403 without token, save 200 with token, change persisted, transport preserved).

- [ ] **Step 4: Commit**

```bash
git add crates/winctl-mcp-server/tests/config_http.rs crates/winctl-mcp-server/Cargo.toml
git commit -m "test: cover dashboard config load/save/gating over http"
```

---

## Task 9: Frontend — delete the old config-tab code

**Files:**
- Modify: `crates/winctl-mcp-server/dashboard/src/main.js`

- [ ] **Step 1: Remove the old config-tab pieces**

Delete exactly these (verified locations; line numbers will shift as you delete top-down — delete bottom-up to keep them stable):

1. Template: the entire `<section v-if="selectedTab === 'config'" class="space-y-5"> … </section>` block (lines ~2668-2786).
2. Method `copyConfigText` (lines ~1077-1083).
3. Computed properties `configServerId`, `configHttpEndpoint`, `configServerCommand`, `configTokenEnvName`, `configTokenLiteral`, `configAuthHeaderValue`, `configTokenCommandPrefix`, `configSelectedExample` (lines ~671-837).
4. Data properties `configClient`, `configTransport`, `configServerName`, `configHttpUrl`, `configTokenMode`, `configTokenEnv`, `configTokenValue`, `configCommand`, `copiedConfigId` (lines ~255-263).
5. In the `mounted()` / watch logic, remove the `config`-specific triggers: `|| tab === 'config'` (line ~968) and `|| this.selectedTab === 'config'` (line ~977) that call `loadConnect`. (Leave `loadConnect` itself — it's shared with the Connect tab.)

Do **not** remove the `{ id: 'config', label: 'Config' }` tab entry yet — Task 10 reuses the id and renames the label.

- [ ] **Step 2: Verify the dashboard still builds**

Run: `cd crates/winctl-mcp-server/dashboard && npm run build`
Expected: build succeeds (the `config` tab now renders nothing; that's fixed in Task 10).

- [ ] **Step 3: Commit**

```bash
git add crates/winctl-mcp-server/dashboard
git commit -m "refactor: remove redundant client-snippet config tab"
```

---

## Task 10: Frontend — build the Settings form + save/restart/reconnect

**Files:**
- Modify: `crates/winctl-mcp-server/dashboard/src/main.js`

- [ ] **Step 1: Rename the tab label** (line ~77)

```javascript
  { id: 'config', label: 'Settings' },
```

- [ ] **Step 2: Add Settings data properties** (in `data()`, where the old config props were)

```javascript
      settingsLoading: false,
      settingsError: null,
      settingsSaving: false,
      settingsRestarting: false,
      settingsEditable: false,
      settingsTrayAvailable: false,
      settingsConfigFile: null,
      settingsConnection: { transport: '', listen: '', auth_required: false, auth_token_set: false },
      settingsConfirmOpen: false,
      settingsNotice: '',
      settingsFieldErrors: [],
      settingsForm: {
        policy: {
          enable_filesystem_mutation: false,
          enable_clipboard_write: false,
          enable_registry_mutation: false,
          allow_private_network: false,
          memory_mutation_enabled: true,
          macro_execution_enabled: true,
          macro_destructive_tools_allowed: false,
          max_macro_runtime_ms: null,
          max_macro_steps: null,
          screenshot_retention_count: null,
          tool_allowlist: '',
          tool_denylist: '',
        },
        paths: { capture_dir: '', artifact_dir: '', filesystem_roots: '', memory_db: '' },
        logging: { log_file: '' },
        embedding: { model_path: '', dimension: null },
        macro_execution: { enabled: true, allow_destructive_tools: false, max_runtime_ms: null, max_steps: null },
      },
```

- [ ] **Step 3: Add the Settings methods** (in `methods`)

```javascript
    settingsListToText(value) {
      return Array.isArray(value) ? value.join('\n') : '';
    },
    settingsTextToList(value) {
      return String(value || '')
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0);
    },
    settingsNumOrNull(value) {
      if (value === null || value === undefined || value === '') return null;
      const n = Number(value);
      return Number.isFinite(n) ? n : null;
    },
    applySettingsResponse(payload) {
      this.settingsEditable = payload.editable === true;
      this.settingsTrayAvailable = payload.tray_available === true;
      this.settingsConfigFile = payload.config_file ?? null;
      this.settingsConnection = payload.connection ?? this.settingsConnection;
      const s = payload.sections ?? {};
      const p = s.policy ?? {};
      const paths = s.paths ?? {};
      const logging = s.logging ?? {};
      const emb = s.embedding ?? {};
      const macro = s.macro_execution ?? {};
      this.settingsForm = {
        policy: {
          enable_filesystem_mutation: !!p.enable_filesystem_mutation,
          enable_clipboard_write: !!p.enable_clipboard_write,
          enable_registry_mutation: !!p.enable_registry_mutation,
          allow_private_network: !!p.allow_private_network,
          memory_mutation_enabled: p.memory_mutation_enabled !== false,
          macro_execution_enabled: p.macro_execution_enabled !== false,
          macro_destructive_tools_allowed: !!p.macro_destructive_tools_allowed,
          max_macro_runtime_ms: p.max_macro_runtime_ms ?? null,
          max_macro_steps: p.max_macro_steps ?? null,
          screenshot_retention_count: p.screenshot_retention_count ?? null,
          tool_allowlist: this.settingsListToText(p.tool_allowlist),
          tool_denylist: this.settingsListToText(p.tool_denylist),
        },
        paths: {
          capture_dir: paths.capture_dir ?? '',
          artifact_dir: paths.artifact_dir ?? '',
          filesystem_roots: this.settingsListToText(paths.filesystem_roots),
          memory_db: paths.memory_db ?? '',
        },
        logging: { log_file: logging.log_file ?? '' },
        embedding: { model_path: emb.model_path ?? '', dimension: emb.dimension ?? null },
        macro_execution: {
          enabled: macro.enabled !== false,
          allow_destructive_tools: !!macro.allow_destructive_tools,
          max_runtime_ms: macro.max_runtime_ms ?? null,
          max_steps: macro.max_steps ?? null,
        },
      };
    },
    async loadSettings() {
      this.settingsLoading = true;
      this.settingsError = null;
      try {
        const response = await fetch('/dashboard/config', { headers: { ...authHeaders() } });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || payload.ok === false) {
          throw new Error(payload.reason || `HTTP ${response.status}`);
        }
        this.applySettingsResponse(payload);
      } catch (error) {
        this.settingsError = String(error);
      } finally {
        this.settingsLoading = false;
      }
    },
    settingsPayload() {
      const f = this.settingsForm;
      const orNull = (v) => (v.trim() === '' ? null : v.trim());
      return {
        policy: {
          ...f.policy,
          max_macro_runtime_ms: this.settingsNumOrNull(f.policy.max_macro_runtime_ms),
          max_macro_steps: this.settingsNumOrNull(f.policy.max_macro_steps),
          screenshot_retention_count: this.settingsNumOrNull(f.policy.screenshot_retention_count),
          tool_allowlist: this.settingsTextToList(f.policy.tool_allowlist),
          tool_denylist: this.settingsTextToList(f.policy.tool_denylist),
        },
        paths: {
          capture_dir: orNull(f.paths.capture_dir),
          artifact_dir: orNull(f.paths.artifact_dir),
          filesystem_roots: this.settingsTextToList(f.paths.filesystem_roots),
          memory_db: orNull(f.paths.memory_db),
        },
        logging: { log_file: orNull(f.logging.log_file) },
        embedding: {
          model_path: orNull(f.embedding.model_path),
          dimension: this.settingsNumOrNull(f.embedding.dimension),
        },
        macro_execution: {
          enabled: f.macro_execution.enabled,
          allow_destructive_tools: f.macro_execution.allow_destructive_tools,
          max_runtime_ms: this.settingsNumOrNull(f.macro_execution.max_runtime_ms),
          max_steps: this.settingsNumOrNull(f.macro_execution.max_steps),
        },
      };
    },
    async saveSettings() {
      this.settingsSaving = true;
      this.settingsError = null;
      this.settingsFieldErrors = [];
      this.settingsNotice = '';
      try {
        const response = await fetch('/dashboard/config', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
          body: JSON.stringify(this.settingsPayload()),
        });
        const payload = await response.json().catch(() => ({}));
        if (response.status === 422) {
          this.settingsFieldErrors = payload.errors ?? [];
          throw new Error('Validation failed');
        }
        if (!response.ok || payload.ok === false) {
          throw new Error(payload.reason || `HTTP ${response.status}`);
        }
        this.settingsNotice = 'Saved. Restart required to apply.';
        return true;
      } catch (error) {
        this.settingsError = String(error);
        return false;
      } finally {
        this.settingsSaving = false;
      }
    },
    async restartServer() {
      this.settingsRestarting = true;
      this.settingsError = null;
      try {
        const response = await fetch('/dashboard/restart', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
        });
        const payload = await response.json().catch(() => ({}));
        if (payload.restarting) {
          this.settingsNotice = 'Restarting server…';
          await this.waitForServerBack();
        } else if (payload.manual) {
          this.settingsNotice = payload.instructions || 'Restart manually via the tray.';
        } else {
          throw new Error(payload.reason || payload.error || `HTTP ${response.status}`);
        }
      } catch (error) {
        this.settingsError = String(error);
      } finally {
        this.settingsRestarting = false;
      }
    },
    async waitForServerBack() {
      const deadline = Date.now() + 20000;
      // brief grace period so we poll the NEW process, not the dying one
      await new Promise((r) => window.setTimeout(r, 1500));
      while (Date.now() < deadline) {
        try {
          const response = await fetch('/healthz', { cache: 'no-store' });
          if (response.ok) {
            this.settingsNotice = 'Server restarted.';
            await this.loadSettings();
            return;
          }
        } catch (_) {
          /* server still down; keep polling */
        }
        await new Promise((r) => window.setTimeout(r, 750));
      }
      this.settingsError = 'Server did not come back within 20s; check the tray and logs.';
    },
    async saveAndRestart() {
      this.settingsConfirmOpen = false;
      if (await this.saveSettings()) {
        await this.restartServer();
      }
    },
```

> `authHeaders()` and the `authToken` it reads are already defined at module scope (main.js:57-95). No new auth plumbing is needed.

- [ ] **Step 4: Trigger `loadSettings` when the tab opens**

Find where the dashboard reacts to `selectedTab` changes (the same place the old code called `loadConnect` for `config`, ~line 968-977). Add a `config`-tab trigger that calls `this.loadSettings()`. If there's a `watch: { selectedTab(tab) { … } }`, add:

```javascript
        if (tab === 'config') this.loadSettings();
```

and in `mounted()`, after initial load:

```javascript
        if (this.selectedTab === 'config') this.loadSettings();
```

- [ ] **Step 5: Add the Settings template** (where the old `config` section was deleted in Task 9)

```html
        <section v-if="selectedTab === 'config'" class="space-y-5">
          <section class="winctl-card">
            <div class="winctl-card-header flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <h2 class="text-sm font-semibold">Server settings</h2>
                <div class="text-xs text-slate-500">Edits the server's config file; a restart applies them.</div>
              </div>
              <button type="button" class="btn btn-xs" :disabled="settingsLoading" @click="loadSettings">Refresh</button>
            </div>

            <div v-if="!settingsEditable" class="alert alert-warning mx-4 mt-4 text-xs">
              Server started without a <code>--config</code> file; settings are read-only.
            </div>
            <div v-if="settingsError" class="alert alert-error mx-4 mt-4 text-sm">{{ settingsError }}</div>
            <div v-if="settingsNotice" class="alert alert-info mx-4 mt-4 text-sm">{{ settingsNotice }}</div>

            <dl class="grid gap-2 px-4 py-3 text-xs sm:grid-cols-2">
              <div><dt class="font-semibold">Transport</dt><dd>{{ settingsConnection.transport }}</dd></div>
              <div><dt class="font-semibold">Listen</dt><dd>{{ settingsConnection.listen }}</dd></div>
              <div><dt class="font-semibold">Auth required</dt><dd>{{ settingsConnection.auth_required ? 'yes' : 'no' }}</dd></div>
              <div><dt class="font-semibold">Auth token set</dt><dd>{{ settingsConnection.auth_token_set ? 'yes' : 'no' }}</dd></div>
            </dl>
          </section>

          <fieldset :disabled="!settingsEditable" class="space-y-5">
            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Policy</h3>
              <div class="grid gap-2 sm:grid-cols-2">
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.enable_filesystem_mutation" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Filesystem mutation</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.enable_clipboard_write" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Clipboard write</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.enable_registry_mutation" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Registry mutation</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.allow_private_network" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Allow private network</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.memory_mutation_enabled" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Memory mutation</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.macro_execution_enabled" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Macro execution</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.macro_destructive_tools_allowed" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Destructive macro tools</span></label>
              </div>
              <div class="mt-3 grid gap-3 sm:grid-cols-3">
                <label class="form-control"><span class="label-text text-xs font-semibold">Max macro runtime (ms)</span><input v-model="settingsForm.policy.max_macro_runtime_ms" type="number" min="0" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Max macro steps</span><input v-model="settingsForm.policy.max_macro_steps" type="number" min="0" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Screenshot retention</span><input v-model="settingsForm.policy.screenshot_retention_count" type="number" min="0" class="input input-sm input-bordered" /></label>
              </div>
              <div class="mt-3 grid gap-3 sm:grid-cols-2">
                <label class="form-control"><span class="label-text text-xs font-semibold">Tool allowlist (one per line)</span><textarea v-model="settingsForm.policy.tool_allowlist" rows="3" class="textarea textarea-bordered textarea-sm"></textarea></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Tool denylist (one per line)</span><textarea v-model="settingsForm.policy.tool_denylist" rows="3" class="textarea textarea-bordered textarea-sm"></textarea></label>
              </div>
            </section>

            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Paths</h3>
              <div class="grid gap-3 sm:grid-cols-3">
                <label class="form-control"><span class="label-text text-xs font-semibold">Capture dir</span><input v-model="settingsForm.paths.capture_dir" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Artifact dir</span><input v-model="settingsForm.paths.artifact_dir" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Memory DB</span><input v-model="settingsForm.paths.memory_db" type="text" class="input input-sm input-bordered" /></label>
              </div>
              <label class="form-control mt-3"><span class="label-text text-xs font-semibold">Filesystem roots (one per line)</span><textarea v-model="settingsForm.paths.filesystem_roots" rows="3" class="textarea textarea-bordered textarea-sm"></textarea></label>
            </section>

            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Logging &amp; embedding</h3>
              <div class="grid gap-3 sm:grid-cols-3">
                <label class="form-control"><span class="label-text text-xs font-semibold">Log file</span><input v-model="settingsForm.logging.log_file" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Embedding model path</span><input v-model="settingsForm.embedding.model_path" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Embedding dimension</span><input v-model="settingsForm.embedding.dimension" type="number" min="1" class="input input-sm input-bordered" /></label>
              </div>
            </section>

            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Macro execution</h3>
              <div class="grid gap-2 sm:grid-cols-2">
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.macro_execution.enabled" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Enabled</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.macro_execution.allow_destructive_tools" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Allow destructive tools</span></label>
              </div>
              <div class="mt-3 grid gap-3 sm:grid-cols-2">
                <label class="form-control"><span class="label-text text-xs font-semibold">Max runtime (ms)</span><input v-model="settingsForm.macro_execution.max_runtime_ms" type="number" min="0" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Max steps</span><input v-model="settingsForm.macro_execution.max_steps" type="number" min="0" class="input input-sm input-bordered" /></label>
              </div>
            </section>

            <ul v-if="settingsFieldErrors.length" class="alert alert-error text-xs">
              <li v-for="err in settingsFieldErrors" :key="err.field"><strong>{{ err.field }}</strong>: {{ err.message }}</li>
            </ul>

            <div class="flex flex-wrap gap-2">
              <button type="button" class="btn btn-sm" :disabled="settingsSaving || !settingsEditable" @click="saveSettings">Save</button>
              <button type="button" class="btn btn-sm btn-info" :disabled="settingsSaving || settingsRestarting || !settingsEditable" @click="settingsConfirmOpen = true">Save &amp; Restart</button>
            </div>
          </fieldset>

          <div v-if="settingsConfirmOpen" class="modal modal-open">
            <div class="modal-box">
              <h3 class="text-sm font-semibold">Restart the server?</h3>
              <p class="py-2 text-xs">This saves your changes and restarts the MCP server via the tray. The dashboard will reconnect when it's back.</p>
              <p v-if="!settingsTrayAvailable" class="text-xs text-warning">Tray binary not found next to the server — you'll get manual restart instructions instead.</p>
              <div class="modal-action">
                <button type="button" class="btn btn-sm" @click="settingsConfirmOpen = false">Cancel</button>
                <button type="button" class="btn btn-sm btn-info" @click="saveAndRestart">Save &amp; Restart</button>
              </div>
            </div>
          </div>
        </section>
```

- [ ] **Step 6: Build the dashboard**

Run: `cd crates/winctl-mcp-server/dashboard && npm run build`
Expected: build succeeds.

- [ ] **Step 7: Commit**

```bash
git add crates/winctl-mcp-server/dashboard
git commit -m "feat: add dashboard server-settings editor with save and restart"
```

---

## Task 11: Full verification + final build

**Files:** none (verification only)

- [ ] **Step 1: Workspace check (Linux host)**

Run: `cargo check --workspace`
Expected: success.

- [ ] **Step 2: Windows target check**

Run: `cargo check --workspace --target x86_64-pc-windows-msvc`
Expected: success.

- [ ] **Step 3: Run the test suite**

Run: `cargo test -p winctl-mcp-server`
Expected: all unit tests (`config_serde_tests`, `config_editor::tests::*`) and the `config_http` integration test PASS.

- [ ] **Step 4: Manual smoke (Windows, optional but recommended)**

Build and run the server with a config file, open `/dashboard`, go to **Settings**, toggle a policy gate, click **Save** (expect "Restart required"), then **Save & Restart** (expect the dashboard to reconnect and reflect the change). Confirm the `[transport]`/`[auth]` sections in the TOML are untouched.

- [ ] **Step 5: Final commit if anything was fixed up**

```bash
git add -A
git commit -m "chore: finalize dashboard config-editor verification"
```

---

## Self-Review Notes (carried into execution)

- **Gating nuance:** config/restart require a valid bearer token **even on loopback**. On a tokenless-loopback server the registry is empty, so editing is locked until a token exists; bootstrap by creating one on the (open) Connect tab. Task 8 verifies both the 403 lock and the bootstrap-then-save path.
- **Restart safety:** never call `POST /dashboard/restart` for real in automated tests — `winctl-tray` is a build-sibling and would actually restart the server. Restart logic is unit-tested via `build_restart_command`/`find_tray_binary`.
- **Locked-field coherence:** because `[transport]`/`[auth]` are read-only, the args `winctl-tray restart` reconstructs from the file stay consistent with the persisted file — no drift.
- **No-config-file:** GET returns `editable:false` with the live policy for read-only display; POST returns `409`.
