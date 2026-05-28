# process.kill

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Terminate only a process launched and tracked by this MCP server session.

## Inputs

- `pid`: optional process ID.
- `launch_id`: optional launch ID returned by `process.launch`.
- `force`: terminate the tracked process.
- `kill_tree`: currently rejected; child processes are not killed by default.

## Notes

The tool refuses to kill arbitrary existing user or system processes. Ownership and current process identity are checked before termination.
