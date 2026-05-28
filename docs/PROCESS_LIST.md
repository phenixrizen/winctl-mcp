# process.list

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: List Windows process metadata, with optional top-level window candidates and MCP-launched process markers.

## Inputs

- `name_contains`: optional process-name filter.
- `exe_path_contains`: optional executable-path filter.
- `include_windows`: include owned top-level visible windows.
- `only_mcp_launched`: only return processes launched by this MCP server session.

## Notes

Terminal-like processes such as Windows Terminal, `cmd.exe`, PowerShell, and `pwsh.exe` are marked so clients can avoid confusing terminal windows with target apps.
