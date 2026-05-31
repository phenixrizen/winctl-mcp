# diagnostics.crash_report

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Collect process, window, screenshot, and platform diagnostic context.

## Inputs

- `pid`: optional process ID.
- `bound_id`: optional bound window ID to capture.

## Notes

On Windows, the tool queries recent Application error events through `wevtutil.exe` and scans standard WER crash-dump locations. It also keeps the existing process/window/screenshot context.
