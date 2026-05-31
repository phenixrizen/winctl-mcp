# uia.scroll_into_view

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Scroll a revalidated UI Automation element into view when supported.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Direct ScrollItemPattern support is not enabled in this build, so the tool returns a fail-closed diagnostic.
