# process.metrics

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return process metric diagnostics and platform availability for resource counters.

## Inputs

- `pid`: process ID.

## Notes

This first implementation returns process identity metadata and marks native CPU/memory/handle counters as unavailable when no provider is enabled.
