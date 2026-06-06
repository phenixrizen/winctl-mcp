# assert.window_count

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert the number of current windows matching a selector.

## Inputs

- `selector`: `WindowSelector` fields.
- `expected`: optional exact count.
- `min`: optional minimum count.
- `max`: optional maximum count.
- `expect`: `present` or `absent` when no count predicate is supplied.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.

## Notes

Read-only; no armed control session is needed. The selector is diagnostic only; this tool does not bind or control a window. Assertion failures return `ok: false`.
