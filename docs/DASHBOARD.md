# Dashboard

[Back to tool index](INDEX.md)

The Streamable HTTP server exposes a read-only Vue dashboard:

- `/dashboard`: browser UI for server status, policy, bound windows, launched processes, memory, macros, control state, UIA inspection, and screenshots.
- `/dashboard/state`: JSON diagnostics used by the dashboard.
- `/dashboard/uia`: authenticated JSON endpoint for live UIA snapshots by `bound_id`.
- `/dashboard/screenshot`: authenticated JSON endpoint for live bound-window screenshots.
- `/dashboard/capture-file`: authenticated image endpoint for capture files under the configured capture directory.
- `/dashboard/assets/*`: embedded static Vue/Tailwind/daisyUI assets.

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

The Windows tray opens the same URL inside a native WebView2 window using `wry`.

## Development

Dashboard source lives in `crates/winctl-mcp-server/dashboard`. The Rust server embeds the built files from `dashboard/dist`, so rebuild those assets before compiling a release binary after UI changes:

```bash
make dashboard-build
```
