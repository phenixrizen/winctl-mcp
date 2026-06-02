# uia.select

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Select a revalidated UI Automation element with strict target resolution.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.
- `mode`: optional `replace`, `add`, or `remove`; defaults to `replace`.

## Notes

Uses UI Automation `SelectionItemPattern.Select`, `AddToSelection`, or `RemoveFromSelection` after resolving exactly one fresh target. `add` and `remove` are idempotent when the item is already in the requested selected/unselected state. Coordinate fallback is reported as a hint only and is not dispatched silently.
