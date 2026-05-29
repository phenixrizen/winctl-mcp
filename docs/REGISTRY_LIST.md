# registry.list

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: List Windows registry subkeys and optional values from a selected hive.

## Inputs

- `hive`: one of `current_user`, `local_machine`, `classes_root`, `users`, or `current_config`.
- `path`: key path under the hive.
- `include_values`: include value metadata and decoded data.

## Policy

This is a read-only registry inspection tool. Registry mutation tools are denied unless explicitly enabled.
