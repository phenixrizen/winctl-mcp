# windows.close

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Post `WM_CLOSE` to a bound window after revalidating stable identity.

## Inputs

- `bound_id`: bound window ID.

## Notes

This asks the application window to close. It is separate from `process.kill`, which is restricted to MCP-launched processes.
