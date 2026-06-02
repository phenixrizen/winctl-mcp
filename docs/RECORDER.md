# Recorder

[Back to tool index](INDEX.md)

The local recorder is a foundation for recording UI automation into auditable macro manifests. It exposes:

- Dashboard Recorder tab: start, pause, stop, review inferred launch/bind metadata, inspect redacted recorded steps, and promote a reviewed manifest.
- `/recorder`: local read-only legacy recorder UI.
- `/recorder/state`: windows and recording session JSON.
- `recorder.*` MCP tools for session lifecycle and manifest export.

The recorder stores structured MCP tool steps. It does not bypass target identity validation; exported manifests run through the normal macro engine. Recorded text is redacted by default, password fields emit `macro.type_secret` placeholders, and inferred launch/bind metadata is marked as requiring review before replay.
