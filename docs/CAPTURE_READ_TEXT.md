# capture.read_text

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Extract readable text from a bound window using UI Automation.

## Inputs

- `bound_id`: bound window ID.
- `max_depth`, `max_elements`: optional UIA snapshot limits.

## Notes

Returns text fragments from UIA name, automation ID, and class-name fields.
