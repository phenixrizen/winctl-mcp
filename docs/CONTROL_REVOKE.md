# control.revoke

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Emergency-stop desktop control and reject future sensitive actions until rearmed.

## Inputs

- `session_id`: optional client or run session identifier.
- `reason`: optional audit text.

## Notes

Revocation is fail-closed for sensitive control tools. Call `control.arm` or `control.consent` to allow control again.
