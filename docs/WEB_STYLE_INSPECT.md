# web.style.inspect

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Inspect computed CSS style for a selector in a browser or WebView target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Connects to the target WebSocket, resolves the selector through the DOM domain, and dispatches `CSS.getComputedStyleForNode`.
