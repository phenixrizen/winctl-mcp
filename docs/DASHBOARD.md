# Dashboard

[Back to tool index](INDEX.md)

The Streamable HTTP server exposes a small read-only dashboard:

- `/dashboard`: browser UI for server status, policy, bound windows, launched processes, memory, and macros.
- `/dashboard/state`: JSON diagnostics used by the dashboard.

## Policy

The dashboard is intended for local Windows/WSL diagnostics. Loopback access is unauthenticated by default. If HTTP is bound to a non-loopback address, the dashboard requires the same bearer token as `/mcp`.

## Use

```powershell
winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765
```

Then open:

```text
http://127.0.0.1:8765/dashboard
```
