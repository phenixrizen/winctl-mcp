# diagnostics.crash_report

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Collect process, window, screenshot, and platform diagnostic context.

## Inputs

- `pid`: optional process ID.
- `bound_id`: optional bound window ID to capture.

## Notes

Windows Event Log and WER dump discovery are reported as provider-unavailable in this first implementation.
