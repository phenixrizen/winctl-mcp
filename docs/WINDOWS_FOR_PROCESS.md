# windows.for_process

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: List visible top-level windows for a PID or MCP launch ID, with explicit child-process policy.

## Inputs

- `pid`: optional process ID.
- `launch_id`: optional MCP launch ID.
- `include_child_process_windows`: include child-process windows as separate candidates.

## Notes

This returns candidates instead of guessing. Use exact PID/HWND metadata for binding.
