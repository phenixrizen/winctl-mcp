# Windows Runbook

## Build For Windows

The default Windows target is `x86_64-pc-windows-msvc`. Use this target for release-compatible binaries, MSI packaging, and runtime integration testing.

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build -p winctl-mcp-server --target x86_64-pc-windows-msvc --release
cargo build -p winctl-tray --target x86_64-pc-windows-msvc --release
```

Build the full workspace from a Windows Rust/MSVC environment or through the GitHub release workflow:

```powershell
cargo build --workspace --target x86_64-pc-windows-msvc --release
```

From WSL/Linux, use `cargo check --workspace --target x86_64-pc-windows-msvc` to validate Windows code without linking. If you need a local WSL cross-linked binary for development, explicitly opt into GNU/MinGW:

```bash
make build-win-server WINDOWS_TARGET=x86_64-pc-windows-gnu
```

The Makefile intentionally fails fast when a Linux/WSL shell tries to link the default MSVC target, because server/tray dependencies require the Windows MSVC linker.

Package the server binary plus this runbook:

```bash
make package-win
```

Run `make package-win` from Windows/MSVC or CI for release-compatible packages. For a WSL-only local compatibility package, pass `WINDOWS_TARGET=x86_64-pc-windows-gnu`.

The packaged release is written under `dist/winctl-mcp-<version>-windows-<target>/` and includes binaries, docs, scripts, examples, `VERSION.txt`, `RELEASE.json`, and `CHECKSUMS.sha256`.

Install or update the package on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-winctl-mcp.ps1
```

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
target\x86_64-pc-windows-msvc\release\winctl-mcp-server.exe
```

Set `RUST_LOG=info` for operational logs. The server writes tracing logs to stderr so stdout remains reserved for MCP protocol messages. Use `--log-file <path>` with either `serve` transport when the MCP client hides stderr.

HTTP binds to loopback by default. If you bind to anything other than loopback, pass `--auth-token <token>` and send `Authorization: Bearer <token>` on MCP requests. Tool routes are not exposed over unauthenticated non-loopback HTTP.

See [Client Configs](CLIENT_CONFIGS.md) for Codex, Claude Desktop, and Streamable HTTP examples.

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
- MSVC build fails on Windows: install the Rust MSVC toolchain and Visual Studio Build Tools with the C++ workload.
- MSVC check fails from WSL/Linux: install the target with `rustup target add x86_64-pc-windows-msvc`; use `cargo check` unless a full MSVC linker environment is available.
- GNU compatibility build fails from WSL: install the Rust GNU target and ensure MinGW link tools are available.

Packaged installs include a diagnostic helper:

```powershell
powershell -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\winctl-mcp\scripts\diagnose-winctl-mcp.ps1" -RunSelfTest
```
