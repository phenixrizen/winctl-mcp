# memory.delete

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Explicitly delete one remembered item by ID and remove it from memory indexes.

## Inputs

- `id`: memory item ID.

## Notes

Deletion removes the item from the SQLite table, FTS5 index, and sqlite-vec vector index.
