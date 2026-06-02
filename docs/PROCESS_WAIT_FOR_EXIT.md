# process.wait_for_exit

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Wait for a process identified by PID or MCP launch ID to exit and return lifecycle timing metadata.

## Inputs

- `pid`: optional process ID.
- `launch_id`: optional launch ID returned by `process.launch`.
- `timeout_ms`: optional timeout in milliseconds.
- `poll_interval_ms`: optional poll interval in milliseconds.

## Notes

When a tracked `launch_id` exits, the server forgets that launch record.
