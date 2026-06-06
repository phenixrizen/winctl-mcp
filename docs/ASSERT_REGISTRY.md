# assert.registry

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert a Windows registry value exists, is absent, or has expected kind/data.

## Inputs

- `hive`: registry hive such as `current_user` or `local_machine`.
- `path`: key path within the hive.
- `name`: optional value name; omit or null for the default value.
- `expect`: `present` or `absent`.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `value`: expected registry value data as JSON. Compared data is redacted in results.
- `kind`: expected registry value kind.

## Notes

Read-only; no armed control session is needed and registry mutation policy is not used. Results summarize existence, kind, value kind, and match status without returning registry data.
