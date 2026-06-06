# Distribution

[Back to tool index](INDEX.md)

Phase 16 packages the Windows runtime as a directory that can be copied, installed, checksummed, and diagnosed without relying on WSL paths.

## Build and Package

Release-compatible Windows packages use the MSVC target:

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --workspace --target x86_64-pc-windows-msvc --release
```

Run package builds from a Windows Rust/MSVC environment or through the GitHub release workflow. GNU/MinGW packages are a local development compatibility option only, not the public end-user default. The Makefile still provides `make package-win` for environments with `make` and `bash` available, and it fails fast if a Linux/WSL shell tries to link the MSVC target.

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

Release builds also produce `winctl-mcp-<version>-windows-x64.msi` with WiX. The MSI installs the MSVC-built binaries/docs under `Program Files\winctl-mcp` and adds Start Menu shortcuts for `winctl-mcp Control`, `winctl-mcp Dashboard`, and `winctl-mcp Recorder`. Each shortcut routes through `winctl-tray.exe`; the Control shortcut keeps the tray app running, while Dashboard and Recorder start the MCP server if needed before opening their native WebView windows.

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
| `CI` | Runs formatting, workspace tests, an MSVC Windows core-crate check from Linux, and live MSVC full-workspace runtime integration tests on `windows-latest`. |
| `Release` | Computes SemVer, builds MSVC Windows binaries, signs and verifies release EXEs through Azure Artifact Signing, packages ZIP and MSI assets, signs and verifies the MSI, writes final release-asset checksums, attests provenance, and publishes a GitHub release. |

Signing configuration is managed through the protected GitHub `release` environment. Configure these environment secrets with the values from your approved Azure Artifact Signing setup:

| Secret | Purpose |
| --- | --- |
| `AZURE_CLIENT_ID` | Federated workload identity client ID used by `azure/login`. |
| `AZURE_TENANT_ID` | Entra tenant ID for the workload identity. |
| `AZURE_SUBSCRIPTION_ID` | Subscription containing the Artifact Signing account. |
| `AZURE_ARTIFACT_SIGNING_ENDPOINT` | Region endpoint for the signing account. |
| `AZURE_ARTIFACT_SIGNING_ACCOUNT` | Artifact Signing account name. |
| `AZURE_ARTIFACT_SIGNING_CERT_PROFILE` | Certificate profile name authorized for signing. |

The release workflow does not store these values in repository files. After Azure signing runs, it verifies Authenticode signatures with `scripts/verify-windows-signatures.ps1` before publishing. To verify downloaded release files locally on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-windows-signatures.ps1 `
  .\bin\winctl-mcp-server.exe `
  .\bin\winctl-tray.exe
```

For the MSI asset:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-windows-signatures.ps1 `
  .\winctl-mcp-<version>-windows-x64.msi
```
