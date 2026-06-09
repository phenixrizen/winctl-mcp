# Troubleshooting

[Back to tool index](INDEX.md)

Run the packaged diagnostic script first:

```powershell
$base = "$env:LOCALAPPDATA\winctl-mcp"
powershell -ExecutionPolicy Bypass -File "$base\scripts\diagnose-winctl-mcp.ps1" -RunSelfTest
```

## Transport Startup

- Check the server process: `Get-Process winctl-mcp-server`.
- Check health from Windows: `Invoke-RestMethod http://127.0.0.1:8765/healthz`.
- Check logs: `%LOCALAPPDATA%\winctl-mcp\logs\server.log`.
- If Codex shows a `winctl-cmp` tool prefix, rename the MCP server entry to `winctl-mcp` and restart Codex so it creates a fresh MCP session.

## WSL to Windows HTTP

Windows and WSL can have different loopback scopes. The installed launcher binds HTTP on `0.0.0.0:8765` with a bearer token so WSL can reach the Windows server through the Windows host address. Read the token on Windows:

```powershell
Get-Content "$env:LOCALAPPDATA\winctl-mcp\http-auth-token"
```

Then use `http://<windows-host-ip>:8765/mcp` with `Authorization: Bearer <token>` from WSL.

Then configure the WSL client with:

```toml
[mcp_servers.winctl-mcp]
url = "http://<windows-host-ip>:8765/mcp"
http_headers = { Authorization = "Bearer replace-me" }
tool_timeout_sec = 120.0
```

Never bind HTTP to a non-loopback address without a token. The server fails startup rather than expose sensitive tools unauthenticated.

## Windows Permissions

- Run the server in the same Windows desktop session and user account as the target app.
- Keep the desktop unlocked.
- Do not automate elevated apps unless the server is also elevated.
- Secure desktops, UAC prompts, lock screens, and protected apps can block capture and input.

## Capture Availability

- Use an explicit capture directory under `%LOCALAPPDATA%\winctl-mcp\captures`.
- Confirm the directory is writable with `diagnose-winctl-mcp.ps1`.
- Some minimized, protected, or secure windows cannot be captured.
- If a screenshot tool crashes the transport, inspect the log file and rerun `diagnose-winctl-capture.ps1` against a known bound window.

## Target Discovery

- Use `process.launch`, then `windows.wait_for_window` with the returned PID or launch ID.
- Do not bind by title alone.
- Use `process.list` and `process.describe` to distinguish a real app window from Windows Terminal, PowerShell, browser tabs, and WebView child processes.

## Memory Database

- Default path: `%LOCALAPPDATA%\winctl-mcp\memory\memory.sqlite`.
- If the memory DB is missing or inaccessible, the server logs the error and can fall back to an in-memory store.
- Use `memory.reindex` after restoring a backup or changing embedding settings.

## Embedding Model

- Expected model path: `%LOCALAPPDATA%\winctl-mcp\models\minilm.onnx`.
- The current memory schema stores MiniLM-compatible 384-dimensional vectors and records model metadata.
- If a model is missing, diagnostics should report it; memory operations still use the built-in local embedding fallback.

## Macro and Test Replay

- Validate first with `macro.validate` or `test.validate`.
- Use `macro.dry_run` or `test.dry_run` before UI mutation.
- Replay failures should produce step status, target identity diagnostics, wait/assertion details, and artifact paths.
- Export results with `macro.export_result` or `test.export_result` for review.
