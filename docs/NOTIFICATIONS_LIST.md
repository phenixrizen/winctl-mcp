# notifications.list

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return Windows notification inspection status and any available provider-backed notifications.

## Inputs

- `max_items`: optional item limit.

## Notes

This first implementation reports provider availability and returns an empty list when no notification provider is enabled.
