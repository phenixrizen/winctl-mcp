# registry.read

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Read a Windows registry value from a selected hive.

## Inputs

- `hive`: one of `current_user`, `local_machine`, `classes_root`, `users`, or `current_config`.
- `path`: key path under the hive.
- `name`: optional value name; omit or use empty string for the default value.

## Notes

String, expand string, multi-string, DWORD, QWORD, and binary values are decoded into JSON.
