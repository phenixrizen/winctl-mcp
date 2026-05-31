# uia.range_value

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Set a numeric value on a revalidated UI Automation range element when supported.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `value`: numeric range value.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Uses UI Automation `RangeValuePattern.SetValue` and rejects read-only or out-of-range values before dispatching. Responses include before/after element state and range metadata when available.
