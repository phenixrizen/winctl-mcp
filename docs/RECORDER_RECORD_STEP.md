# recorder.record_step

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Append one recorded MCP tool step with optional target, note, and replay metadata.

## Inputs

- `id`: optional stable step ID.
- `tool`: MCP tool name to record.
- `args`: JSON arguments for the tool.
- `target`: optional semantic macro target.
- `timeout_ms`: optional step timeout.
- `required`: whether failure should fail replay.
- `continue_on_failure`: continue replay after failure.
- `coordinate_fallback`: optional replay coordinate metadata.
- `audit`: optional step audit metadata.
- `note`: optional recorder note.
