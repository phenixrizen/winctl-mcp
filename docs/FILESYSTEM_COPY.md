# filesystem.copy

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Copy a file within allowlisted roots only when filesystem mutation is explicitly enabled.

## Inputs

- `from`: source file path.
- `to`: destination file path.
- `overwrite`: allow replacing an existing destination file.

## Policy

Denied by default. Set `WINCTL_ENABLE_FILESYSTEM_MUTATION=1` and keep both paths under allowlisted roots.
