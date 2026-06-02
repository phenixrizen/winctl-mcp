# assert.element

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert UI Automation element existence, enabled state, and name conditions.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `exists`, `enabled`, `name`, `name_contains`: optional assertions.
- `max_depth`, `max_elements`: optional snapshot limits.

## Notes

Returns `passed`, `failures`, match count, matching elements, and snapshot summary.
