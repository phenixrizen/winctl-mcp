# assert.clipboard

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert current clipboard text equals or contains expected text.

## Inputs

- `expected`: optional exact text.
- `contains`: optional substring.
- `max_chars`: optional read truncation.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.

## Notes

This is read-only and does not require clipboard write policy or an armed session. Clipboard text and expected text are not returned; results include only presence, character count, format, warnings, and pass/fail metadata.
