# recorder.start

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Start a local recording session that can be exported as a macro manifest.

## Inputs

- `title`: recording title.
- `description`: optional description.
- `tags`: optional tags.
- `app_identity`: optional macro app identity metadata.
- `capture_input`: optional boolean. When omitted or true, the native Windows hook recorder passively captures human input into steps; when false, the session accepts only explicit `recorder.record_step` calls until resumed.
