# assert.text_visible

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert literal or regex text visibility through window metadata or a scoped UI Automation tree.

## Inputs

- `bound_id`: bound window ID.
- `text`: optional literal text to find.
- `text_regex`: optional regular expression to match.
- `element_ref` or `selector`: optional UI Automation scope; when omitted, window title/class and the full UIA tree are searched.
- `expect`: `present` or `absent`.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `max_depth`, `max_elements`: optional UIA snapshot limits.

## Notes

Read-only; no armed control session is needed. Request text and regex are redacted in results. Matches report sources and element references, not the matched text payload. Assertion failures return `ok: false`.
