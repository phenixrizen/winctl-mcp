# process.describe

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Describe one process with executable metadata, tracked launch status, children, and top-level windows.

## Inputs

- `pid`: target process ID.

## Notes

The response includes partial metadata when some process fields are inaccessible, plus warnings for unavailable fields.
