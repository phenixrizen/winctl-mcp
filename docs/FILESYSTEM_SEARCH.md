# filesystem.search

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Search file names and bounded UTF-8 file content beneath an allowlisted filesystem root.

## Inputs

- `root`: directory path to search.
- `pattern`: literal pattern to search for.
- `max_results`: optional result limit.
- `max_file_bytes`: optional per-file read limit.

## Policy

Search is literal and bounded. It skips files that cannot be read as UTF-8 within the configured byte limit.
