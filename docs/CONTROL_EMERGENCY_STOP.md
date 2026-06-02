# control.emergency_stop

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Alias for `control.revoke`.

## Inputs

- `session_id`: optional client or run session identifier.
- `reason`: optional audit text.

## Notes

Use this as the tray or dashboard stop action. On Windows, the server also registers `Ctrl+Alt+Esc` as a global emergency-stop hotkey and records the same revoked control state when it fires.
