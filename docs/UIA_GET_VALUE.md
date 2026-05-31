# uia.get_value

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Read a UI Automation `ValuePattern` value from a revalidated element.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Uses UI Automation `ValuePattern.CurrentValue` and returns read-only state. Unsupported patterns fail closed instead of returning unrelated snapshot fields as a fake value.
