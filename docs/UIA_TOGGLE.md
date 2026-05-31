# uia.toggle

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Toggle a revalidated UI Automation element with `TogglePattern`.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Uses UI Automation `TogglePattern.Toggle` and returns pre/post toggle state when available. Unsupported, disabled, offscreen, or stale targets fail closed.
