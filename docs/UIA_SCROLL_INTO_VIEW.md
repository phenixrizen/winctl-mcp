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

Uses UI Automation `ScrollItemPattern.ScrollIntoView` on a freshly revalidated element reference. Unsupported patterns, stale references, disabled elements, and offscreen elements fail closed; responses include before/after element state and coordinate fallback hints.
