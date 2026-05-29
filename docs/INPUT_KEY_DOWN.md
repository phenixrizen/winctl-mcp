# input.key_down

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Focus a bound window and dispatch a virtual key-down event after identity revalidation.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `key`: key name, such as `ctrl`, `shift`, `enter`, `escape`, `a`, or `f5`.

## Notes

Use `input.key_up` to release keys. Shortcut-style combinations should prefer `input.shortcut`.
