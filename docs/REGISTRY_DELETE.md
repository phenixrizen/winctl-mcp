# registry.delete

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Delete a Windows registry value only when registry mutation is explicitly enabled.

## Inputs

- `hive`: one of `current_user`, `local_machine`, `classes_root`, `users`, or `current_config`.
- `path`: key path under the hive.
- `name`: optional value name.

## Policy

Denied by default. Set `WINCTL_ENABLE_REGISTRY_MUTATION=1` to allow this direct MCP tool. It deletes values only, not keys or trees.
