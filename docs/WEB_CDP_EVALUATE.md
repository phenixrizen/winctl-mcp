# web.cdp.evaluate

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Prepare a CDP JavaScript evaluation request and return target diagnostics.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `expression`: JavaScript expression.
- `timeout_ms`: optional HTTP timeout.

## Notes

Target discovery is implemented; WebSocket command execution is reported as unavailable.
