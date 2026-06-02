# control.notify

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Record a pending desktop-control notification event for tray or dashboard display.

## Inputs

- `tool_name`: MCP tool that is about to control the desktop.
- `bound_id`: optional bound window ID.
- `action_kind`: optional category such as `pointer_input`, `text_input`, or `process_kill`.
- `session_id`: optional client or run session identifier.
- `countdown_ms`: optional cancelable countdown duration to display.

## Notes

The server records the notification event even when no native toast provider is enabled.
