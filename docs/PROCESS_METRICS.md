# process.metrics

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return native Windows process resource counters.

## Inputs

- `pid`: process ID.

## Notes

On Windows, the tool samples CPU time and returns working set, peak working set, pagefile usage, handle count, and GDI/USER object counts. On non-Windows runtimes it returns an unsupported-platform error.
