# control.arm

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Arm desktop control for a session or bound target before sensitive actions.

## Inputs

- `session_id`: optional client or run session identifier.
- `bound_id`: optional bound window ID.
- `allow_for_ms`: optional arm duration.
- `reason`: optional audit text.

## Notes

Arming clears emergency-stop state and records an auditable control event tied to the supplied bound target identity when available.
