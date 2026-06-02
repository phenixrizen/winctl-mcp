# input.key_up

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Focus a bound window and dispatch a virtual key-up event after identity revalidation.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `key`: key name, such as `ctrl`, `shift`, `enter`, `escape`, `a`, or `f5`.

## Notes

Only use this for explicit key-hold workflows. Prefer `input.shortcut` for ordinary shortcuts.
