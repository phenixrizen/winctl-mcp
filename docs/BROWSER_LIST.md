# browser.list

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: List Chrome, Edge, and Firefox process/window state with explicit PID/HWND identity metadata.

## Inputs

- `browser`: optional browser kind, one of `chrome`, `edge`, or `firefox`.
- `pid`: optional process ID filter.
- `include_windows`: include recognized top-level browser windows.
- `only_mcp_launched`: restrict results to processes launched and tracked by this server session.

## Notes

This tool returns process, profile hint, executable, PID, HWND, and window metadata for diagnostics. It does not select a browser tab by title.
