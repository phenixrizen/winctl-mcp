# memory.update

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Explicitly update a remembered item and rebuild its FTS5 and sqlite-vec indexes.

## Inputs

- `id`: memory item ID.
- `kind`: optional replacement kind.
- `title`: optional replacement title.
- `text`: optional replacement searchable text.
- `manifest_json`: optional replacement structured manifest.
- `tags`: optional replacement tag list.
- `app_identity_json`: optional replacement app identity.
- `target_identity_json`: optional replacement target identity.

## Notes

Only fields included in the request are changed.
