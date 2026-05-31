# web.a11y.snapshot

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return accessibility-tree provider diagnostics for a browser or WebView target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

Accessibility tree extraction requires the future CDP WebSocket bridge.
