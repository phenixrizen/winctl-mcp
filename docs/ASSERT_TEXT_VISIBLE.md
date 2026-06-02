# assert.text_visible

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert text is visible through window metadata or the UI Automation tree.

## Inputs

- `bound_id`: bound window ID.
- `text`: text to find.
- `max_depth`, `max_elements`: optional UIA snapshot limits.

## Notes

Checks the window title/class first, then UIA element name, automation ID, and class text.
