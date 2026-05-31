# web.cdp.evaluate

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Evaluate JavaScript in a loopback CDP target.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint.
- `target_id`: optional CDP target ID.
- `expression`: JavaScript expression.
- `timeout_ms`: optional HTTP timeout.

## Notes

Discovers the selected target, connects to its loopback `webSocketDebuggerUrl`, and dispatches `Runtime.evaluate` with `returnByValue` and `awaitPromise`.
