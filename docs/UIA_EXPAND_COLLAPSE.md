# uia.expand_collapse

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Expand or collapse a revalidated UI Automation element when supported.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `expand_collapse_action`: optional `expand`, `collapse`, or `toggle`; defaults to `toggle`.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Uses UI Automation `ExpandCollapsePattern` against a freshly revalidated element. Unsupported patterns, stale references, disabled elements, and offscreen elements fail closed; responses include before/after state and coordinate fallback hints.
