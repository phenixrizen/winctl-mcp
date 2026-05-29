# windows.wait_for_window

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Wait for visible top-level window candidates owned by a PID or MCP launch ID without title-only selection.

## Inputs

- `pid`: optional target process ID.
- `launch_id`: optional launch ID returned by `process.launch`.
- `timeout_ms`: optional timeout in milliseconds.
- `title_contains`: optional title filter applied after PID/launch matching.
- `class_name_contains`: optional class-name filter applied after PID/launch matching.
- `allow_child_process_windows`: include child-process window candidates when explicitly enabled.

## Notes

This tool returns all matching candidates rather than guessing. It never binds by title alone.
