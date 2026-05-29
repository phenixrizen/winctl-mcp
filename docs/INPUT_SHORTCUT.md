# input.shortcut

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Focus a bound window and dispatch a virtual-key shortcut after identity revalidation.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `keys`: ordered key names, such as `["ctrl", "shift", "s"]`.
- `hold_ms`: optional time to hold the shortcut before releasing keys in reverse order.

## Notes

The bound window is revalidated and focused before the shortcut is sent.
