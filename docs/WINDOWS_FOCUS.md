# windows.focus

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Focus a previously bound window after revalidating HWND, PID, and executable identity.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.

## Notes

Focus fails closed if the bound HWND is stale or now belongs to a different process/executable identity.
