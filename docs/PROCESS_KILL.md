# process.kill

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Terminate only a process launched and tracked by this MCP server session.

## Inputs

- `pid`: optional process ID.
- `launch_id`: optional launch ID returned by `process.launch`.
- `force`: terminate the tracked process.
- `kill_tree`: terminate the tracked process and its current child-process tree.

## Notes

The tool refuses to kill arbitrary existing user or system processes. Ownership and current process identity are checked before termination.

When `kill_tree` is true, the server snapshots the current process parent graph, terminates descendants deepest-first, and terminates the tracked root last. The root process must still belong to the launch tracked by this server session; untracked PIDs, kill-by-name, and kill-by-title are refused.
