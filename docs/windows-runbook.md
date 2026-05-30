# Windows Runbook

## Build From WSL

The default Makefile target is `x86_64-pc-windows-gnu`, which is the most direct WSL cross-build path.

```bash
make setup-win-target
make build-win-server
make print-artifacts
```

Use `WINDOWS_TARGET=x86_64-pc-windows-msvc` when building from a Windows Rust/MSVC environment:

```powershell
cargo build -p winctl-mcp-server --target x86_64-pc-windows-msvc --release
```

Package the server binary plus this runbook:

```bash
make package-win
```

The packaged server is written under `dist/winctl-mcp-windows-<target>/`.

## Run On Windows

Run `winctl-mcp-server.exe` from an interactive Windows desktop session.

Preferred HTTP transport for Windows/WSL development:

```powershell
winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765 --capture-dir "$env:LOCALAPPDATA\winctl-mcp\captures"
```

Screenshot files default to `%LOCALAPPDATA%\winctl-mcp\captures` on Windows, with temp-directory fallback. Use `--capture-dir <path>` or `WINCTL_CAPTURE_DIR` when the server is launched from a restricted working directory.

Compatibility stdio transport:

```powershell
winctl-mcp-server.exe serve --transport stdio
```

Self-test mode may write human-readable output to stdout:

```powershell
winctl-mcp-server.exe self-test windows-list
```

In stdio mode, MCP clients should launch the server with no shell wrapper that writes to stdout.

Example MCP command path:

```text
target\x86_64-pc-windows-gnu\release\winctl-mcp-server.exe
```

Set `RUST_LOG=info` for operational logs. The server writes tracing logs to stderr so stdout remains reserved for MCP protocol messages. Use `--log-file <path>` with either `serve` transport when the MCP client hides stderr.

HTTP binds to loopback by default. If you bind to anything other than loopback, pass `--auth-token <token>` and send `Authorization: Bearer <token>` on MCP requests. Tool routes are not exposed over unauthenticated non-loopback HTTP.

## Required Permissions

- Run the server as the same Windows user as the target apps.
- Keep the desktop session unlocked and interactive.
- Do not run target apps elevated unless the server is also elevated.
- Allow Windows Graphics Capture prompts when screenshot tools are first used.
- Expect input and capture to fail for secure desktops, UAC prompts, lock screens, and some protected apps.

## Integration Harness

The deterministic fixture harness launches real Win32 windows and validates binding, preflight, child-window ownership, and screenshot-region metadata.

Run it from Windows:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\run-windows-integration.ps1
```

To force a target triple:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\run-windows-integration.ps1 -Target x86_64-pc-windows-msvc
```

The harness sets `WINCTL_RUN_WINDOWS_INTEGRATION=1`. Normal workspace tests do not pop windows unless that variable is set.

## Troubleshooting

- Empty `windows.list`: confirm the server is running on Windows, not WSL/Linux, and that the desktop is unlocked.
- `ambiguous_match`: add `hwnd`, `pid`, `process_name`, or executable-path selectors. Title-only selectors are intentionally rejected for control actions.
- `identity_mismatch` or `stale_target`: re-bind the window. The HWND may have been reused or the process changed.
- Focus or click fails: check elevation mismatch, minimized windows, remote desktop state, and whether another app is blocking foreground activation.
- Screenshot fails: check Graphics Capture permission, display availability, and whether the target window is minimized or protected.
- Cross-build fails for GNU: install the Rust target with `make setup-win-target` and ensure MinGW link tools are available in WSL.
