# filesystem.read

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Read a UTF-8 file from an allowlisted filesystem root with bounded size.

## Inputs

- `path`: file path to read.
- `max_bytes`: optional byte limit, capped by the server.

## Policy

Paths must be under `WINCTL_FS_ROOTS`, the capture directory, or the system temp directory. Binary files return `utf8: false` and omit text.
