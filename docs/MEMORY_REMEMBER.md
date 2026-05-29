# memory.remember

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Explicitly store a structured memory item with searchable text, tags, app/target identity, and sqlite-vec embedding metadata.

## Inputs

- `kind`: memory kind, such as `test_procedure`, `app_workflow`, `ui_observation`, `troubleshooting_note`, `macro`, `assertion_recipe`, or `cleanup_recipe`.
- `title`: short human-readable title.
- `text`: searchable procedure, observation, or recipe text.
- `manifest_json`: optional structured manifest payload.
- `tags`: optional tag list.
- `app_identity_json`: optional app identity metadata.
- `target_identity_json`: optional target identity metadata.

## Notes

Memory mutations are explicit. The server does not silently remember procedures without this tool call.
