# uia.expand_collapse

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Expand or collapse a revalidated UI Automation element when supported.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Direct ExpandCollapsePattern support is not enabled in this build, so the tool returns a fail-closed diagnostic with fallback coordinates.
