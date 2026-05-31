# uia.wait_for_element

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Wait for a UI Automation selector or element reference to resolve in fresh snapshots.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `timeout_ms`: optional timeout.
- `poll_interval_ms`: optional polling interval.
- `max_depth`, `max_elements`: optional snapshot limits.
- `require_enabled`: optional enabled-state condition.
- `require_visible`: optional visibility condition.
- `name_contains`: optional name substring condition.

## Notes

The tool does not trust old element data; every poll captures a fresh bound-window UIA snapshot.
