# macro.run

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Execute a macro manifest through the existing MCP tool implementations with target revalidation before control actions.

## Inputs

- `manifest`: optional `winctl.macro.v1` JSON manifest.
- `memory_id`: optional memory item ID containing a macro manifest.
- `max_steps`: optional execution step limit.

## Notes

Provide either `manifest` or `memory_id`. The runner dispatches to the same underlying tools used by direct MCP calls, so bound-window actions still perform identity revalidation. A successful memory-backed run updates the source memory item's use metadata.
