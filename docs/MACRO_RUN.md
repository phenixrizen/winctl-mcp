# macro.run

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Execute a macro manifest through the existing MCP tool implementations with target revalidation before control actions.

## Inputs

- `manifest`: optional `winctl.macro.v1` JSON manifest.
- `memory_id`: optional memory item ID containing a macro manifest.
- `max_steps`: optional execution step limit.
- `video`: optional run-video capture settings, using the same fields as `capture.video_start`.

## Notes

Provide either `manifest` or `memory_id`. The runner dispatches to the same underlying tools used by direct MCP calls, so bound-window actions still perform identity revalidation. A successful memory-backed run updates the source memory item's use metadata.

When `video` is provided, the runner starts recording before executing steps, stops recording at the end of the run, and attaches the GIF artifact metadata to `result.artifacts`.
