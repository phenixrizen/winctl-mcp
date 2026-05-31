# uia.set_focus

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Focus a revalidated UI Automation element with strict target resolution.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

The first implementation uses center-click fallback after resolving exactly one enabled, onscreen element.
