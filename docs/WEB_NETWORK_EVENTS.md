# web.network.events

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Collect CDP network events from a debugger target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional context selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Connects to the target WebSocket, enables the Network domain, and buffers matching `Network.*` events for the requested timeout window.
