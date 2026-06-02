# browser.assert

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert browser kind and window title/class conditions against a revalidated bound browser window.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `browser`: optional expected browser kind, one of `chrome`, `edge`, or `firefox`.
- `title_contains`: optional expected title substring.
- `class_name_contains`: optional expected window class substring.

## Notes

Assertions fail closed if the bound target no longer validates as the same browser window.
