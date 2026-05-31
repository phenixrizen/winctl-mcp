# web.style.inspect

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return style-inspection provider diagnostics for a browser or WebView target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Style inspection requires the future CDP WebSocket bridge.
