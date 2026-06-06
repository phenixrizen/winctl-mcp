# assert.no_dialog

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert that no native foreground dialog or secure-desktop prompt is blocking automation.

## Inputs

- `max_depth`, `max_elements`: optional UI Automation scan limits for dialog inspection.
- `include_non_foreground`: include non-foreground dialog-like windows.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.

## Notes

Read-only; no armed control session is needed. UAC secure desktop is reported as blocking/not automatable and causes the assertion to fail.
