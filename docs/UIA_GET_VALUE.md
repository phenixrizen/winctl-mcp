# uia.get_value

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Read value-like UI Automation properties from a revalidated element snapshot.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `max_depth`, `max_elements`: optional snapshot limits.
- `allow_offscreen`: allow offscreen targets.

## Notes

Returns snapshot properties such as name, automation ID, class, role, focused, and enabled state.
