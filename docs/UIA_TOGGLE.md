# uia.toggle

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Toggle a revalidated UI Automation element when safe fallback semantics are available.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

The tool fails closed if the element cannot be resolved, is disabled, is offscreen, or has no usable bounds.
