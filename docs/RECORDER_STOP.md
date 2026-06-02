# recorder.stop

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Stop the active recording session and return its macro manifest.

## Inputs

- `save_to_memory`: when true, explicitly store the generated manifest in the memory store if memory mutation policy allows it.

## Notes

The response includes the generated manifest and validation report. When recorded steps include window identity metadata, the manifest includes an inferred `process.launch`, bind strategy, app identity, and `replay.extra.recorder.launch_inference.requires_confirmation=true` so the user can confirm or edit fresh-launch versus bind-existing behavior before replay.
