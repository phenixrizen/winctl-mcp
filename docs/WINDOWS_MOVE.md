# windows.move

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Move a bound window after revalidating HWND, PID, and executable identity.

## Inputs

- `bound_id`: bound window ID.
- `x`: target virtual desktop X coordinate.
- `y`: target virtual desktop Y coordinate.

## Notes

The response includes before/after virtual desktop rect metadata for replay diagnostics.
