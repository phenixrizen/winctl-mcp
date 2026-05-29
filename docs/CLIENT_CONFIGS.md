# Client Configs

[Back to tool index](INDEX.md)

Use `winctl-mcp` as the MCP server name. That name controls the visible tool prefix in clients, so the tools should appear as `winctl-mcp.capture.screenshot_window`, not `winctl-cmp.capture.screenshot_window`.

If a client still shows `winctl-cmp`, it is using an old MCP server entry or a stale MCP session. Rename the server entry, remove the old one, and restart the client so it reconnects.

## Start the Windows Server

Loopback-only local development:

```powershell
$base = "$env:LOCALAPPDATA\winctl-mcp"
& "$base\bin\winctl-mcp-server.exe" serve --config "$base\config.toml"
```

Explicit command without config:

```powershell
winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765 --auth-token replace-me --log-file "$env:LOCALAPPDATA\winctl-mcp\logs\server.log"
```

## Codex

Example config:

```toml
[mcp_servers.winctl-mcp]
url = "http://127.0.0.1:8765/mcp"
http_headers = { Authorization = "Bearer replace-me" }
tool_timeout_sec = 120.0
```

When Codex runs in WSL and Windows loopback is not reachable from WSL, bind the server to a Windows host address and require a token:

```powershell
winctl-mcp-server.exe serve --transport http --listen <windows-host-ip>:8765 --auth-token replace-me
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
