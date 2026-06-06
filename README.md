<p align="center">
  <img src="assets/brand/winctl-wordmark.svg" alt="winctl-mcp" width="520">
</p>

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

End-user Windows releases are built with the MSVC target:

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build -p winctl-mcp-server --target x86_64-pc-windows-msvc --release
```

The default Makefile Windows target is also `x86_64-pc-windows-msvc`. Build release artifacts on Windows with the Rust MSVC toolchain, or through the GitHub release workflow. GNU/MinGW remains a local development compatibility override when a direct WSL cross-link path is useful:

```bash
make build-win-server WINDOWS_TARGET=x86_64-pc-windows-gnu
```

The Makefile fails fast if a Linux/WSL shell tries to link the MSVC target. Use `cargo check --target x86_64-pc-windows-msvc` from WSL for validation, or build/package the MSVC binaries from Windows/CI.

## Package and Install

Build a Windows package with binaries, docs, scripts, examples, version metadata, and SHA-256 checksums:

```bash
make package-win
```

Run packaging from Windows/MSVC or through CI. Published end-user packages should use the MSVC build, signed release binaries, and the MSI/ZIP assets produced by CI. The MSI installs Start Menu shortcuts through the no-console `winctl-launcher.exe` entrypoint. End users should not need Rust, WSL, MSYS2, MinGW, or PowerShell execution-policy workarounds.

Install or update from the package on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-winctl-mcp.ps1
```

See [docs/DISTRIBUTION.md](docs/DISTRIBUTION.md), [docs/DIRECTORY_LAYOUT.md](docs/DIRECTORY_LAYOUT.md), and [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## Runbook

See [docs/windows-runbook.md](docs/windows-runbook.md) for Windows host setup, permissions, troubleshooting, packaging, and the integration harness.

## Tool Reference

See [docs/INDEX.md](docs/INDEX.md) for the MCP tool list. Current tools cover strict window/process control, input, capture, UI Automation snapshots, and explicit local memory operations. See [docs/MACRO_MANIFEST.md](docs/MACRO_MANIFEST.md) for the versioned macro manifest format.

## Server Commands

Preferred Windows/WSL development transport:

```powershell
winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765 --capture-dir "$env:LOCALAPPDATA\winctl-mcp\captures"
```

By default screenshots are written under the user's local app-data directory on Windows, or under the system temp directory when that is unavailable. Override it with `--capture-dir <path>` or `WINCTL_CAPTURE_DIR`.

The local memory database defaults to `%LOCALAPPDATA%\winctl-mcp\memory.sqlite` on Windows, or the system temp directory when local app-data is unavailable. Override it with `WINCTL_MEMORY_DB`.

Saved macro workflows are explicit: call `macro.promote` or `memory.remember`, find them later with `memory.search`, then run them by manifest or memory ID with `macro.dry_run` and `macro.run`.

Configuration file:

```powershell
winctl-mcp-server.exe serve --config "$env:LOCALAPPDATA\winctl-mcp\config.toml"
```

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for transport, auth, logging, filesystem root, mutation policy, memory, embedding, and macro execution settings.

MCP client examples for Codex, Claude Desktop, and generic Streamable HTTP clients are in [docs/CLIENT_CONFIGS.md](docs/CLIENT_CONFIGS.md). The canonical MCP server name is `winctl-mcp`; stale client sessions using `winctl-cmp` need the old server entry removed and the client restarted.

With HTTP transport, the local diagnostic dashboard is available at `/dashboard`. See [docs/DASHBOARD.md](docs/DASHBOARD.md).

The recorder UI is available at `/recorder` and records MCP tool steps into macro manifests. See [docs/RECORDER.md](docs/RECORDER.md).

Test manifests use `winctl.test.v1` and run through the macro engine. See [docs/TEST_MANIFEST.md](docs/TEST_MANIFEST.md).

Optional server controller:

```powershell
winctl-tray.exe status
winctl-tray.exe start --server-exe C:\winctl\winctl-mcp-server.exe
winctl-tray.exe run
```

See [docs/TRAY_CONTROLLER.md](docs/TRAY_CONTROLLER.md).

Compatibility stdio transport:

```powershell
winctl-mcp-server.exe serve --transport stdio
```

Self-test:

```powershell
winctl-mcp-server.exe self-test windows-list
```
