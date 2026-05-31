# uia.set_value

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Set text for a revalidated UI Automation element with before/after state diagnostics.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `value`: text to type.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Uses UI Automation `ValuePattern.SetValue` and rejects read-only elements. The legacy `replace_existing` input is ignored by the direct pattern path because no keyboard fallback is dispatched silently.
