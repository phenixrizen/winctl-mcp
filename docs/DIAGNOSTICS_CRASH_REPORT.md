# diagnostics.crash_report

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Collect process, window, screenshot, and platform diagnostic context.

## Inputs

- `pid`: optional process ID.
- `bound_id`: optional bound window ID to capture.

## Notes

On Windows, the tool queries recent Application error events through the native Event Log API (`EvtQuery`, `EvtNext`, and `EvtRender`) and scans standard WER crash-dump locations. It returns structured event fields plus raw event XML for diagnostics, while keeping the existing process/window/screenshot context.
