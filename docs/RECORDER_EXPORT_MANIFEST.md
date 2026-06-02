# recorder.export_manifest

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Export the active or completed recording session as a macro manifest.

## Inputs

- `session_id`: optional completed or active recording session ID. When omitted, the active session is exported.

## Notes

When recorded steps include window identity metadata, the exported manifest includes inferred launch/bind review metadata. The inference is a draft: callers should review `manifest.replay.extra.recorder.launch_inference` before promoting or running the macro.
