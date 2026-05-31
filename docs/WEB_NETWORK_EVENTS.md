# web.network.events

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return CDP network-event provider diagnostics for a debugger target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional context selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Network interception requires the future CDP WebSocket bridge.
