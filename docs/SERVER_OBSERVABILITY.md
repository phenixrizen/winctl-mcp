# Server Observability

Phase 20 adds in-memory MCP server observability for the dashboard. It does not
add a new MCP tool; the data is exposed through the authenticated
`/dashboard/state` endpoint and rendered in the dashboard Observability tab.

## Connected Clients

`connected_clients` lists active MCP sessions known to the current server
process.

Fields:

- `connection_id`: server-local session identifier.
- `name`: client implementation name from MCP `initialize`, when available.
- `version`: client implementation version from MCP `initialize`, when
  available.
- `transport`: `http`, `stdio`, or local test transport.
- `connected_at_unix_ms`: first-seen timestamp.
- `last_seen_unix_ms`: last initialize or tool-call timestamp.
- `request_count`: number of recorded `tools/call` requests for the connection.
- `last_tool`: most recent tool called by the connection.

Connections are removed when the MCP session closes. A last-seen staleness
prune is also applied as a fallback for transports that do not promptly drop a
session.

## Recent Requests

`recent_requests` is a bounded in-memory ring of the latest tool calls. It is
lost on server restart.

Fields:

- `id`: monotonically increasing server-local request history id.
- `connection_id`: connection that made the call.
- `tool_name`: MCP tool name.
- `started_at_unix_ms`: start timestamp.
- `duration_ms`: elapsed tool-call duration.
- `ok`: result status, derived from structured `{ "ok": ... }` when present.
- `error_code`: structured tool error code when present.
- `summary`: redacted allowlisted metadata.

## Redaction

Request summaries are allowlist-based. They may include stable identifiers,
small numeric fields, status fields, and collection counts. They do not include
typed text, clipboard contents, secret references, memory text, stdout/stderr,
image data, byte blobs, or raw manifests.

Use the Raw tab only when inspecting the full dashboard payload. The
Observability tab renders summaries as compact fields instead of raw JSON.
