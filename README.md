# winctl-mcp

Reliable Windows MCP control server in Rust, with strict bound-window identity checks before focus, input, and capture actions.

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
