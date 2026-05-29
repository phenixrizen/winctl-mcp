# macro.dry_run

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Build a dry-run plan for a macro manifest without performing mutating UI actions.

## Inputs

- `manifest`: optional `winctl.macro.v1` JSON manifest.
- `memory_id`: optional memory item ID containing a macro manifest.

## Notes

Provide either `manifest` or `memory_id`. The plan marks mutating steps, bound-window requirements, artifact-producing steps, categories, and target strategies.
