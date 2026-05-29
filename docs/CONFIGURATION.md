# Configuration

[Back to tool index](INDEX.md)

`winctl-mcp-server` accepts a TOML configuration file with `serve --config <path>`. CLI flags override the loaded file for transport, listen address, auth token, capture directory, and log file.

## Example

```toml
[transport]
mode = "http"
listen = "127.0.0.1:8765"

[auth]
token = "replace-me"

[logging]
log_file = "C:/Users/nater/AppData/Local/winctl-mcp/server.log"

[paths]
capture_dir = "C:/Users/nater/AppData/Local/winctl-mcp/captures"
artifact_dir = "C:/Users/nater/AppData/Local/winctl-mcp/artifacts"
filesystem_roots = [
  "C:/Users/nater/AppData/Local/winctl-mcp/captures",
  "C:/Users/nater/AppData/Local/winctl-mcp/artifacts"
]
memory_db = "C:/Users/nater/AppData/Local/winctl-mcp/memory.sqlite"

[policy]
enable_filesystem_mutation = false
enable_clipboard_write = false
enable_registry_mutation = false
allow_private_network = false
memory_mutation_enabled = true
macro_execution_enabled = true
macro_destructive_tools_allowed = false
max_macro_runtime_ms = 300000
max_macro_steps = 200
screenshot_retention_count = 200
tool_denylist = []

[embedding]
model_path = "C:/Users/nater/AppData/Local/winctl-mcp/models/minilm.onnx"
dimension = 384

[macro_execution]
enabled = true
allow_destructive_tools = false
max_runtime_ms = 300000
max_steps = 200
```

## Policy Defaults

- HTTP binds to loopback by default.
- Non-loopback HTTP still requires an auth token.
- Filesystem mutation, clipboard write, registry mutation, and private-network fetches are disabled by default.
- Memory mutation and macro execution are enabled by default for loopback/local use.
- Filesystem access is limited to configured roots plus the capture directory and system temp directory.

The environment variables `WINCTL_FS_ROOTS`, `WINCTL_ENABLE_FILESYSTEM_MUTATION`, `WINCTL_ENABLE_CLIPBOARD_WRITE`, `WINCTL_ENABLE_REGISTRY_MUTATION`, `WINCTL_ALLOW_PRIVATE_NETWORK`, `WINCTL_ARTIFACT_DIR`, and `WINCTL_MEMORY_DB` remain supported as compatibility defaults.

See [Directory layout](DIRECTORY_LAYOUT.md) for the default Windows install paths used by packaged releases.

## Dashboard

When HTTP transport is enabled, the server also exposes:

- `/dashboard`: local read-only dashboard HTML.
- `/dashboard/state`: dashboard JSON state.

Loopback dashboard access is unauthenticated by default. Non-loopback dashboard access uses the same bearer-token policy as the MCP endpoint.
