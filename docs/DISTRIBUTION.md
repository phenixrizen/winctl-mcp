# Distribution

[Back to tool index](INDEX.md)

Phase 16 packages the Windows runtime as a directory that can be copied, installed, checksummed, and diagnosed without relying on WSL paths.

## Build and Package

From WSL/Linux:

```bash
make setup-win-target
make package-win
```

The package is written to `dist/winctl-mcp-<version>-windows-<target>/` and contains:

| Path | Purpose |
| --- | --- |
| `bin/winctl-mcp-server.exe` | MCP server binary. |
| `bin/winctl-tray.exe` | Optional local controller binary. |
| `docs/` | Operational docs copied into the package. |
| `examples/` | Client and server config examples. |
| `scripts/` | Install, diagnose, and integration helper scripts. |
| `VERSION.txt` | Human-readable version, target, profile, git revision, and build time. |
| `RELEASE.json` | Machine-readable release metadata. |
| `CHECKSUMS.sha256` | SHA-256 checksums for every packaged file. |

Release builds also produce `winctl-mcp-<version>-windows-x64.msi` with WiX. The MSI installs the packaged binaries/docs under `Program Files\winctl-mcp` and adds a Start Menu shortcut for the tray app.

## Install or Update

Copy the package directory to Windows, then run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-winctl-mcp.ps1
```

The installer copies binaries, docs, examples, and scripts to `%LOCALAPPDATA%\winctl-mcp`, creates the default directory layout, and writes `config.toml` when one does not already exist.

To overwrite an existing generated config:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-winctl-mcp.ps1 -Force
```

## Run

```powershell
$base = "$env:LOCALAPPDATA\winctl-mcp"
& "$base\bin\winctl-mcp-server.exe" serve --config "$base\config.toml"
```

Health check:

```powershell
Invoke-RestMethod http://127.0.0.1:8765/healthz
```

## Verify Checksums

From the package root:

```powershell
Get-Content .\CHECKSUMS.sha256
Get-FileHash .\bin\winctl-mcp-server.exe -Algorithm SHA256
```

The server checksum should match the corresponding line in `CHECKSUMS.sha256`.

## Release Automation

GitHub Actions contains two workflows:

| Workflow | Purpose |
| --- | --- |
| `CI` | Runs formatting, workspace tests, and a Windows cross-build on every branch and pull request. |
| `Release` | Computes SemVer, builds Windows binaries, signs release assets when signing secrets are configured, packages ZIP and MSI assets, attests provenance, and publishes a GitHub release. |

Signing configuration is managed through the protected GitHub release environment.
