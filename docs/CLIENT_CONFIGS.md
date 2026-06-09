# Client Configs

[Back to tool index](INDEX.md)

Use `winctl-mcp` as the MCP server name. That name controls the visible tool prefix in clients, so the tools should appear as `winctl-mcp.capture.screenshot_window`, not `winctl-cmp.capture.screenshot_window`.

If a client still shows `winctl-cmp`, it is using an old MCP server entry or a stale MCP session. Rename the server entry, remove the old one, and restart the client so it reconnects.

## Start the Windows Server

Installed Windows control app:

```powershell
$base = "$env:LOCALAPPDATA\winctl-mcp"
& "$base\bin\winctl-launcher.exe" run
```

The launcher binds HTTP on `0.0.0.0:8765` by default so WSL can connect, creates a bearer token at `%LOCALAPPDATA%\winctl-mcp\http-auth-token`, and keeps dashboard windows on `127.0.0.1`.

Explicit command without config:

```powershell
winctl-mcp-server.exe serve --transport http --listen 0.0.0.0:8765 --auth-token replace-me --log-file "$env:LOCALAPPDATA\winctl-mcp\logs\server.log"
```

## Codex

Example config:

```toml
[mcp_servers.winctl-mcp]
url = "http://127.0.0.1:8765/mcp"
http_headers = { Authorization = "Bearer replace-me" }
tool_timeout_sec = 120.0
```

When Codex runs in WSL, point it at the Windows host address and use the generated token from `%LOCALAPPDATA%\winctl-mcp\http-auth-token`:

```powershell
Get-Content "$env:LOCALAPPDATA\winctl-mcp\http-auth-token"
```

Then point Codex at that address:

```toml
[mcp_servers.winctl-mcp]
url = "http://<windows-host-ip>:8765/mcp"
http_headers = { Authorization = "Bearer replace-me" }
tool_timeout_sec = 120.0
```

Do not expose a non-loopback listener without `--auth-token`; the server refuses that configuration.

## Claude Desktop

Use stdio compatibility when the client cannot connect to Streamable HTTP:

```json
{
  "mcpServers": {
    "winctl-mcp": {
      "command": "C:\\Users\\nater\\AppData\\Local\\winctl-mcp\\bin\\winctl-mcp-server.exe",
      "args": [
        "serve",
        "--transport",
        "stdio",
        "--log-file",
        "C:\\Users\\nater\\AppData\\Local\\winctl-mcp\\logs\\server.log"
      ]
    }
  }
}
```

## Generic Streamable HTTP Client

Use `POST /mcp` with optional `Authorization: Bearer <token>` and `GET /healthz` for health checks. The included `examples/streamable-http-client.json` shows the intended connection metadata.
