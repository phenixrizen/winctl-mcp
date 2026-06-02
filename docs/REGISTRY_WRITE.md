# registry.write

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Write a Windows registry value only when registry mutation is explicitly enabled.

## Inputs

- `hive`: one of `current_user`, `local_machine`, `classes_root`, `users`, or `current_config`.
- `path`: key path under the hive.
- `name`: optional value name.
- `kind`: `string`, `expand_string`, `dword`, `qword`, `multi_string`, or `binary`.
- `data`: JSON data matching `kind`.

## Policy

Denied by default. Set `WINCTL_ENABLE_REGISTRY_MUTATION=1` to allow this direct MCP tool. Macro replay does not include registry mutation tools.
