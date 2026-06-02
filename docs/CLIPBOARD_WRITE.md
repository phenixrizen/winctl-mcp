# clipboard.write

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Write Unicode clipboard text only when clipboard mutation is explicitly enabled.

## Inputs

- `text`: Unicode text to place on the clipboard.

## Policy

Denied by default. Set `WINCTL_ENABLE_CLIPBOARD_WRITE=1` to allow this direct MCP tool. Macro replay does not include clipboard mutation tools.
