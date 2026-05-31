# uia.invoke

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Invoke a revalidated UI Automation element with safe fallback diagnostics.

## Inputs

- `bound_id`: bound window ID.
- `element_ref`: optional element reference from `uia.snapshot` or `uia.find`.
- `selector`: optional semantic UIA selector.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

The tool resolves the target in a fresh snapshot and uses center-click fallback when direct UIA pattern control is unavailable.
