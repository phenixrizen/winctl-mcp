# input.type_text

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Focus a bound window and type Unicode text with `SendInput` after identity revalidation.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `text`: Unicode text to type.

## Notes

The bound window is revalidated and focused before text input is sent.
