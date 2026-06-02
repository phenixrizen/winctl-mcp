# recorder.state

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return active and completed local recording sessions plus native capture status.

## Inputs

None.

## Output Notes

- `status`: current recorder status (`idle`, `recording`, or `paused`).
- `native_capture`: provider, hook/hotkey registration state, capture counters, ignored-event count, and last native capture error when available.
