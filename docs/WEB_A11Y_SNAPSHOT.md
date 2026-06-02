# web.a11y.snapshot

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Capture a CDP accessibility tree for a browser or WebView target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Connects to the target WebSocket and dispatches `Accessibility.getFullAXTree`.
