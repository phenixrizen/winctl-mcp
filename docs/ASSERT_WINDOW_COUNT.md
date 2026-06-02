# assert.window_count

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert the number of current windows matching a selector.

## Inputs

- `selector`: `WindowSelector` fields.
- `expected`: optional exact count.
- `min`: optional minimum count.
- `max`: optional maximum count.

## Notes

The selector is diagnostic only; this tool does not bind or control a window.
