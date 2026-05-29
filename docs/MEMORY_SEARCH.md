# memory.search

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Search remembered procedures, observations, macros, and recipes using hybrid sqlite-vec, FTS5, tag, identity, recency, and usefulness ranking.

## Inputs

- `query`: optional natural-language search query.
- `tags`: optional required tags.
- `kind`: optional memory kind filter.
- `app_identity_json`: optional app identity filter signal.
- `target_identity_json`: optional target identity filter signal.
- `limit`: optional result limit.

## Notes

Results include score components for vector similarity, keyword match, tag match, identity match, and usefulness.
