# Dashboard Server-Config Editor + Tray-Supervised Restart — Design

## Problem

The dashboard's **Config** tab (`{ id: 'config', label: 'Config' }`, `dashboard/src/main.js:77`)
is a redundant client-connection snippet generator. It overlaps with the real **Connect**
tab (`main.js:68`, backed by `GET /dashboard/connect`) and cannot edit the server's own
configuration or restart the server. It is to be removed and replaced.

We want an operator to be able to **edit the MCP server's operational configuration from the
dashboard and restart the server to apply it**, without leaving the web UI.

## Goals

- Replace the Config tab's contents in place with a real server-config editor.
- Edit the operational config sections and persist them to the server's `--config` TOML file.
- Restart the server (via the tray supervisor) to apply changes.
- Gate the editing/restart endpoints behind bearer token + loopback + explicit confirmation.

## Non-Goals

- Live/hot config apply without a restart. (All changes require a restart.)
- Editing connection-defining fields (`[transport]` mode/listen) or the `[auth]` token **value**
  from the web UI — these are shown read-only.
- Config editing when the server was started **without** a `--config` file (read-only + notice).
- Windows-service or non-tray supervision; self-restart of the server process.
- Preserving TOML comments/formatting across a round-trip (the file is treated as
  machine-managed).

## Locked Decisions

1. **Restart mechanism (option A):** the dashboard restart handler spawns the existing tray
   command `winctl-tray restart` as a **detached** child and returns. That child stops the
   current server and starts a fresh one with the same `--config` file (the tray's proven
   stop+start + pid-tracking path). No new IPC, no tray polling timer, no new tray code.
   If the tray binary can't be located, the endpoint returns a manual-instruction fallback.
2. **Editable scope:** `[policy]`, `[paths]`, `[logging]`, `[embedding]`, `[macro_execution]`
   are editable. `[transport]` (mode/listen) and `[auth]` token are **read-only** (changing them
   from the web UI would strand the dashboard session or write a plaintext token).
3. **Gating:** `POST /dashboard/config` and `POST /dashboard/restart` require a valid dashboard
   bearer token (header only — never accepted via query string), a loopback server binding, and
   an explicit in-UI confirmation. Any unauthenticated or non-loopback request → `403`.

## Architecture & Components

### Backend

1. **State plumbing.** Thread the server's config path and connection metadata into the
   dashboard handler state so handlers can read/write it:
   - `ServeConfig.config_file: Option<PathBuf>` is already known at startup
     (`main.rs`, set from `--config`). Carry it (plus `listen` and an `auth_configured: bool`)
     into `DashboardState` / `AppState` (whichever the dashboard handlers receive).
   - `GET /dashboard/connect` already exposes `listen`; reuse the same plumbing pattern.

2. **Config IO module** (`crates/winctl-mcp-server/src/tools/config_editor.rs`, new):
   - Add `#[derive(Serialize)]` to `WinctlConfigFile` and its section structs
     (`TransportFileConfig`, `AuthFileConfig`, `LoggingFileConfig`, `PathsFileConfig`,
     `PolicyFileConfig`, `EmbeddingFileConfig`, `MacroExecutionFileConfig`) — they currently
     derive `Deserialize` only.
   - `load_editable(path) -> EditableConfig`: read + parse the existing TOML to
     `WinctlConfigFile`, project the editable sections.
   - `apply_and_serialize(path, edits) -> String`: read current file → overlay edited operational
     sections onto the parsed `WinctlConfigFile` (leaving `[transport]`/`[auth]`/unknown fields
     untouched) → re-serialize to a TOML string.
   - `write_atomic(path, contents)`: write to a sibling temp file, then rename over the target.

3. **Validation.** Before persisting, validate the candidate file produces a valid `ServeConfig`
   (reuse the existing `ServeConfig::from_file` conversion + checks: listen parseable, paths
   well-formed, numeric limits in range). Return structured per-field errors (mirroring the
   `macro.validate` error shape). Never write an invalid file; never restart into one.

4. **Restart launcher** (in the config module):
   - `find_tray_binary() -> Option<PathBuf>`: look beside `std::env::current_exe()` for
     `winctl-tray` / `winctl-tray.exe`.
   - `build_restart_command(tray_path) -> Command`: pure, unit-testable construction of
     `winctl-tray restart`.
   - `spawn_restart()`: spawn the command **detached** (so it survives the parent being killed),
     return success/fallback.

5. **New endpoints** (registered in `run_mcp_http`'s dashboard router, gated):
   - `GET  /dashboard/config`
   - `POST /dashboard/config`
   - `POST /dashboard/restart`
   A small auth guard (reusing the existing bearer-token check) wraps the two mutating routes and
   additionally refuses when `listen` is non-loopback and rejects query-string tokens.

### Frontend

6. **Settings form** — rebuild the `config` tab body in `dashboard/src/main.js` (and styles in
   `dashboard/src/styles.css`). Keep the tab id `config`; rename its label to `Settings`. The
   Connect tab is untouched.

## Data Flow

- **Load:** tab mount → `GET /dashboard/config` → render editable sections + read-only Connection
  panel + `editable`/`tray_available` flags.
- **Save:** Save button → `POST /dashboard/config` with edited sections → server validates →
  atomic write → `{ ok: true, restart_required: true }`. No restart.
- **Save & Restart:** confirm modal → `POST /dashboard/config` then, on success,
  `POST /dashboard/restart` → server spawns `winctl-tray restart` → `{ restarting: true,
  poll_url }`. Frontend polls `GET /healthz` until the new server answers, then reloads state.
  If `{ restarting: false, manual: true }`, show the `winctl-tray restart` instruction.

## Endpoint Contracts

### `GET /dashboard/config`
Response:
```json
{
  "ok": true,
  "editable": true,
  "config_file": "C:/.../winctl-config.toml",
  "tray_available": true,
  "connection": { "transport": "http", "listen": "127.0.0.1:8765", "auth_configured": true },
  "policy": { "...editable policy fields..." },
  "paths": { "capture_dir": "...", "artifact_dir": "...", "filesystem_roots": ["..."], "memory_db": "..." },
  "logging": { "log_file": "..." },
  "embedding": { "model_path": "...", "dimension": 384 },
  "macro_execution": { "enabled": true, "allow_destructive_tools": false, "max_runtime_ms": 300000, "max_steps": 200 }
}
```
When no config file: `"editable": false`, sections reflect the live effective values, and the UI
shows a read-only notice. The auth token **value is never included** — only `auth_configured`.

### `POST /dashboard/config`
Body: `{ "policy": {...}, "paths": {...}, "logging": {...}, "embedding": {...}, "macro_execution": {...} }`
(all sections optional; only provided fields are overlaid).
- `200 { "ok": true, "restart_required": true }` on success.
- `422 { "ok": false, "errors": [ { "field": "paths.filesystem_roots[1]", "message": "..." } ] }`
  on validation failure (nothing written).
- `403` if unauthenticated, non-loopback, or query-token used.
- `409 { "ok": false, "reason": "no_config_file" }` if the server has no `--config` file.

### `POST /dashboard/restart`
- `200 { "restarting": true, "poll_url": "/healthz" }` when the tray restart was spawned.
- `200 { "restarting": false, "manual": true, "instructions": "Run: winctl-tray restart" }`
  when the tray binary wasn't found.
- `403` under the same gating as above.

## Editable Field Catalog

| Section | Field | Control | Validation |
|---|---|---|---|
| policy | enable_filesystem_mutation / enable_clipboard_write / enable_registry_mutation / allow_private_network / memory_mutation_enabled / macro_execution_enabled / macro_destructive_tools_allowed | toggle | bool |
| policy | max_macro_runtime_ms / max_macro_steps / screenshot_retention_count | number | optional, ≥ 0 |
| policy | tool_allowlist / tool_denylist | string-list editor | each non-empty |
| paths | capture_dir / artifact_dir / memory_db | path text | non-empty when set |
| paths | filesystem_roots | path-list editor | each non-empty |
| logging | log_file | path text | optional |
| embedding | model_path | path text | optional |
| embedding | dimension | number | optional, > 0 |
| macro_execution | enabled / allow_destructive_tools | toggle | bool |
| macro_execution | max_runtime_ms / max_steps | number | optional, ≥ 0 |

(`filesystem_roots` is the authorization boundary for filesystem tools — editing it directly
changes what the server may touch, which is the point.)

## Security

- Mutating endpoints require the bearer token in the `Authorization` header; query-string tokens
  are rejected so the token can't leak into request logs.
- Config edits are refused entirely when `listen` is non-loopback.
- The auth token value is never returned by `GET /dashboard/config`.
- Each successful save is logged (tool/endpoint, config path, changed sections — never values that
  could include the token, which is not editable here anyway).

## Persistence Details

- Atomic write: temp file in the same directory as the target, then rename.
- `[transport]`, `[auth]`, and any fields not in the editable set are preserved from the existing
  file (overlay, not full rewrite from the editable subset).
- Because the connection fields are locked, the args the tray reconstructs on
  `winctl-tray restart` (`serve --transport http --config <file> [--listen ...] [--auth-token ...]`)
  remain consistent with the persisted file — no drift between the two.

## Testing

- **Unit (config IO):** round-trip preserves `[transport]`/`[auth]`; overlay produces expected
  values; `write_atomic` replaces the file; loading a no-config-file server reports `editable:false`.
- **Unit (validation):** bad paths / out-of-range limits → structured errors; valid edits pass.
- **Unit (restart command):** `build_restart_command` yields `winctl-tray restart`;
  `find_tray_binary` returns `None` → endpoint takes the manual-fallback branch (no real spawn).
- **Endpoint (http smoke):** `GET /dashboard/config` never leaks the token; `POST` without token →
  403; non-loopback bind → 403; invalid body → 422; valid body writes to a temp config file.
- **Frontend smoke:** Settings tab renders, Save posts, 422 errors surface per-field, Save & Restart
  opens the confirm modal.

## Edge Cases & Failure Modes

- **No `--config` file:** read-only Settings + notice; `POST /dashboard/config` → 409.
- **Tray binary absent:** restart endpoint returns the manual-instruction fallback.
- **Non-loopback binding:** all config mutation refused (403).
- **Invalid candidate config:** 422, nothing written, no restart.
- **Server doesn't come back after restart:** frontend `/healthz` poll times out → show a
  "restart may have failed; check the tray / logs" message.
- **Concurrent edits:** last-write-wins (single-operator assumption); documented, not arbitrated.

## Affected Files

- `crates/winctl-mcp-server/src/main.rs` — state plumbing, route registration, `Serialize` derives
  on the config-file structs (or move them), auth guard for the two mutating routes.
- `crates/winctl-mcp-server/src/tools/config_editor.rs` — **new**: load/overlay/validate/atomic-write,
  tray discovery + restart command.
- `crates/winctl-mcp-server/src/tools/mod.rs` — register the new module.
- `crates/winctl-mcp-server/dashboard/src/main.js` — rebuild the `config` tab as the Settings form;
  label rename to `Settings`.
- `crates/winctl-mcp-server/dashboard/src/styles.css` — form styling.
- `crates/winctl-mcp-server/tests/stdio_smoke.rs` (or a new test file) — endpoint + gating tests.

## Open Risks

- **Tray non-default config discovery:** `winctl-tray restart` loads the tray's own default config
  to reconstruct server args. Deployments that run the tray with a non-default `--config` path may
  need that path made discoverable to the server; documented as a known limitation for the MVP.
