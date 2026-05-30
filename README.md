# winctl-mcp

Reliable Windows MCP control server in Rust, with strict bound-window identity checks before focus, input, and capture actions.

## Inspiration

This project takes inspiration from [CursorTouch/Windows-MCP](https://github.com/CursorTouch/Windows-MCP/) as an example of broad Windows automation through MCP, while focusing on strict, auditable control of known Windows app targets.

`winctl-mcp` prioritizes stable window/process identity, fail-closed behavior, structured diagnostics, Windows/WSL reliability, and reproducible Rust builds. See [ROADMAP.md](ROADMAP.md) for planned expansion areas.

## Naming corrections

Use these canonical crate names in this repository:

- `winctl` (library crate; replaces `windows-control`)
- `winctl-mcp-server` (binary crate; replaces `windows-mcp-server`)

## Build

From WSL/Linux:

```bash
make setup-win-target
make build-win-server
```

The default Windows target is `x86_64-pc-windows-gnu`. Override it when needed:

```bash
make build-win-server WINDOWS_TARGET=x86_64-pc-windows-msvc
```

## Runbook

See [docs/windows-runbook.md](docs/windows-runbook.md) for Windows host setup, permissions, troubleshooting, packaging, and the integration harness.

## Server Commands

Preferred Windows/WSL development transport:

```powershell
winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765 --capture-dir "$env:LOCALAPPDATA\winctl-mcp\captures"
```

By default screenshots are written under the user's local app-data directory on Windows, or under the system temp directory when that is unavailable. Override it with `--capture-dir <path>` or `WINCTL_CAPTURE_DIR`.

Compatibility stdio transport:

```powershell
winctl-mcp-server.exe serve --transport stdio
```

Self-test:

```powershell
winctl-mcp-server.exe self-test windows-list
```
