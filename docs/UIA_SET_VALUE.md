# uia.set_value

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Set text for a revalidated UI Automation element with before/after state diagnostics.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `value`: text to type.
- `replace_existing`: selects existing text first; defaults to true.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

This first pass uses strict revalidation plus focus/type fallback and reports direct UIA pattern support as disabled.
