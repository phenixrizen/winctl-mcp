# web.cdp.list_targets

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: List local Chrome DevTools Protocol targets from a debugger HTTP endpoint.

## Inputs

- `debugger_url`: loopback CDP HTTP endpoint, such as `http://127.0.0.1:9222`.
- `timeout_ms`: optional HTTP timeout.

## Notes

Only loopback debugger endpoints are accepted in this build.
