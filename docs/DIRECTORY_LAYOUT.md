# Directory Layout

[Back to tool index](INDEX.md)

Default Windows install root:

```text
%LOCALAPPDATA%\winctl-mcp
```

| Path | Purpose | Config key or environment override |
| --- | --- | --- |
| `bin/` | Installed Windows binaries. | Installer-managed. |
| `config.toml` | Server transport, auth, paths, policy, memory, and macro settings. | `serve --config <path>` |
| `logs/server.log` | Persistent tracing output when stderr is hidden by a client. | `[logging].log_file`, `--log-file` |
| `captures/` | Screenshot output. | `[paths].capture_dir`, `WINCTL_CAPTURE_DIR`, `--capture-dir` |
| `artifacts/` | Exported artifacts from tools and macro runs. | `[paths].artifact_dir`, `WINCTL_ARTIFACT_DIR` |
| `memory/memory.sqlite` | Local memory database. | `[paths].memory_db`, `WINCTL_MEMORY_DB` |
| `models/minilm.onnx` | MiniLM-compatible embedding model location. | `[embedding].model_path` |
| `manifests/` | Saved macro and test manifests. | Filesystem policy root. |
| `replays/` | Replay outputs and diagnostics. | Artifact policy root. |
| `exports/` | User-directed exported manifests and artifacts. | Filesystem policy root. |
| `docs/` | Packaged operational docs. | Installer-managed. |
| `examples/` | Client and server config examples. | Installer-managed. |
| `scripts/` | Install, diagnose, and integration helpers. | Installer-managed. |

Filesystem tools are limited to configured roots plus the capture directory and system temp directory. Add only the directories you actually need to `[paths].filesystem_roots`.
