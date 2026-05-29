# windows.resize

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Resize a bound window after revalidating HWND, PID, and executable identity.

## Inputs

- `bound_id`: bound window ID.
- `width`: target window width.
- `height`: target window height.

## Notes

The response includes before/after virtual desktop rect metadata for replay diagnostics.
