# macro.run

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Execute a macro manifest through the existing MCP tool implementations with target revalidation before control actions.

## Inputs

- `manifest`: a `winctl.macro.v1` JSON manifest.
- `max_steps`: optional execution step limit.

## Notes

The runner dispatches to the same underlying tools used by direct MCP calls, so bound-window actions still perform identity revalidation.
