# filesystem.delete

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Delete an allowlisted filesystem path only when filesystem mutation is explicitly enabled.

## Inputs

- `path`: file or directory path to delete.
- `recursive`: allow recursive directory deletion.

## Policy

Denied by default. Set `WINCTL_ENABLE_FILESYSTEM_MUTATION=1`; deletion is still limited to allowlisted roots.
