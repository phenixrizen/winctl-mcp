# winctl-mcp

Reliable Windows MCP control server in Rust, with strict bound-window identity checks before focus, input, and capture actions.

## Why this instead of CursorTouch/Windows-MCP?

[CursorTouch/Windows-MCP](https://github.com/CursorTouch/Windows-MCP/) is a broad Windows automation MCP server. It is useful when you want a Python/uv-installed toolset for general Windows UI automation, app control, input, state capture, and optional browser DOM mode.

`winctl-mcp` is narrower by design, but better for Codex-driven Windows app testing where controlling the wrong window is worse than doing nothing:

| Area | `winctl-mcp` advantage |
| --- | --- |
| Window identity | Binds a target by stable identity and revalidates HWND + PID + executable before focus, click, typing, and window screenshot actions. |
| Title safety | Treats title-only matching as diagnostic input, not a trusted control selector. This avoids accidentally driving Windows Terminal, browser tabs, WebViews, or similarly titled windows. |
| Process launch flow | Can launch an executable directly, return the PID and launch ID, wait for PID-owned top-level windows, and bind by returned PID + HWND instead of fuzzy title matching. |
| Process cleanup | `process.kill` only terminates processes launched and tracked by this server session, and verifies current process identity before killing. |
| Windows/WSL development | Provides a loopback Streamable HTTP transport so the Windows server can stay alive independently while Codex or another agent runs from WSL. |
| Stdio robustness | In stdio mode, stdout is reserved for MCP JSON-RPC only; logs go to stderr and/or `--log-file`. |
| Diagnostics | Prefers explicit structured failures and candidate lists over guessing. Screenshot responses include exact virtual desktop region metadata. |
| Runtime shape | Rust workspace with reproducible WSL cross-build commands and a single Windows server binary. |

Windows-MCP may still be a better fit if you want broad UI Automation coverage, browser DOM extraction, PyPI/`uvx` distribution, or a large general-purpose Windows automation surface. This project is optimized for strict, auditable control of a known Windows app target.

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
winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765
```

Compatibility stdio transport:

```powershell
winctl-mcp-server.exe serve --transport stdio
```

Self-test:

```powershell
winctl-mcp-server.exe self-test windows-list
```
