# uia.toggle

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Toggle a revalidated UI Automation element with `TogglePattern`.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.
- `desired_state`: optional `off`, `on`, or `indeterminate`.

## Notes

Uses UI Automation `TogglePattern.Toggle` and returns pre/post toggle state. When `desired_state` is provided, the action is idempotent: if the element is already in that state, no toggle is dispatched and `toggle_count` is `0`. Unsupported, disabled, offscreen, stale, or unreachable target states fail closed.
