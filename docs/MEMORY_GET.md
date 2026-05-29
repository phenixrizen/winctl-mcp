# memory.get

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Fetch one memory item by ID and update its explicit use metadata.

## Inputs

- `id`: memory item ID.

## Notes

Fetching an item increments `use_count` and updates `last_used_at`.
