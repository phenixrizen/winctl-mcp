# assert.dialog

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert that a specific native dialog is present or absent by title, text, and buttons.

## Inputs

- `expect`: `present` or `absent`.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `max_depth`, `max_elements`: optional UI Automation scan limits.
- `include_non_foreground`: include non-foreground dialog-like windows.
- `title`, `title_contains`, `title_regex`: dialog title predicates.
- `text_contains`: text fragment expected somewhere in the dialog UIA tree.
- `button_names`: button names expected in the dialog UIA tree.

## Notes

Read-only; no armed control session is needed. This tool never clicks buttons; use `dialogs.invoke_button` for explicit, gated dialog control.
