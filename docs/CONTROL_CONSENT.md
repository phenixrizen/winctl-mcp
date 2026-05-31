# control.consent

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Record an `allow_once`, `allow_session`, `deny`, or `revoke_session` decision for desktop-control actions.

## Inputs

- `decision`: `allow_once`, `allow_session`, `deny`, or `revoke_session`.
- `session_id`: optional client or run session identifier.
- `bound_id`: optional bound window ID.
- `allow_for_ms`: optional duration for `allow_session`.

## Notes

Consent events are logged and exposed through `control.state` for dashboard or tray display.
