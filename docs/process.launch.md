# process.launch

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Launch a Windows executable via `CreateProcessW` and optionally wait for visible PID-owned window candidates.

## Inputs

- `exe`: Windows executable path.
- `args`: optional argv-style argument list.
- `cwd`: optional working directory.
- `env`: optional environment overrides.
- `wait_for_window`: wait for a visible top-level window owned by the launched PID.
- `timeout_ms`: optional window wait timeout.
- `allow_child_process_windows`: include child-process window candidates when explicitly enabled.

## Notes

This tool does not invoke `cmd.exe` or PowerShell by default. It tracks launched processes in memory for the current MCP server session and returns a `launch_id`.
