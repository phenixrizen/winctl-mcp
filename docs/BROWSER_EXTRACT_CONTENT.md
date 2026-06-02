# browser.extract_content

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return safe browser window content hints and identity metadata for a revalidated bound browser window.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.

## Notes

The current implementation returns window-level content hints such as title and class name. DOM extraction is intentionally disabled until explicit browser debugging/session integration is added.
