# web.dom.snapshot

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return CDP DOM snapshot provider diagnostics for a debugger target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `selector`: optional selector.
- `timeout_ms`: optional HTTP timeout.

## Notes

The tool reports target metadata and a provider-unavailable diagnostic until the CDP WebSocket bridge is enabled.
