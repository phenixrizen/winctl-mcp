# clipboard.read

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Read Unicode clipboard text with optional truncation.

## Inputs

- `max_chars`: optional maximum number of characters to return.

## Notes

This tool reads `CF_UNICODETEXT` only. It returns a warning when the clipboard does not currently contain Unicode text.
