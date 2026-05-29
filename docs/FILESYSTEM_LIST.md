# filesystem.list

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: List files and directories beneath an allowlisted filesystem root.

## Inputs

- `path`: directory path to list.
- `recursive`: recursively list child directories.
- `max_entries`: optional entry limit.

## Policy

Paths must be under an allowlisted root. Configure additional roots with `WINCTL_FS_ROOTS` using semicolon-separated paths.
