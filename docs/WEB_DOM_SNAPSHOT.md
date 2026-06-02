# web.dom.snapshot

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Capture a CDP DOM snapshot for a debugger target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Connects to the target WebSocket and dispatches `DOMSnapshot.captureSnapshot` with DOM rect and paint-order data.
